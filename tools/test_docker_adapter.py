"""Isolated Docker provider tests. Never contact a real Docker daemon/container."""
import importlib.machinery
import importlib.util
import json
import io
import os
from pathlib import Path
import queue
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import MagicMock, patch

ROOT = Path(__file__).resolve().parents[1]
ADAPTER = ROOT / "adapters/docker/dragonstui-docker"
CID = "a" * 64
OTHER = "b" * 64
loader = importlib.machinery.SourceFileLoader("docker_adapter", str(ADAPTER))
spec = importlib.util.spec_from_loader(loader.name, loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)

FAKE = r'''#!/usr/bin/env python3
import fcntl, json, os, signal, struct, subprocess, sys, termios, time
from pathlib import Path
base = Path(os.environ["FAKE_DOCKER_ROOT"])
a = sys.argv[1:]
with (base / "calls").open("a") as f:
    f.write(json.dumps(a) + "\n")
if a[:1] == ["--context"]:
    a = a[2:]
if a[:2] == ["container", "inspect"]:
    if a[3] == "{{.Id}}":
        print("a" * 64)
    elif a[3] == "{{.State.Running}}":
        print("true")
    else:
        assert "Config.Env" not in a[3] and "Labels" not in a[3]
        print(json.dumps({"id": a[-1], "name": "/fixture", "image_id": "sha256:safe",
                          "created": "2026-01-01", "state": {"status": "running", "running": True,
                          "paused": False, "restarting": False, "exit_code": 0}}))
elif a[:2] == ["container", "ls"]:
    if (base / "slow").exists():
        time.sleep(10)
    if (base / "flood").exists():
        sys.stdout.write("x" * 200000)
    else:
        print(json.dumps({"ID": "a" * 64, "Names": "fixture", "Image": "fixture:v1",
                          "State": "running", "Status": "Up", "Ports": ""}))
elif a[:2] == ["container", "stats"]:
    print(json.dumps({"CPUPerc": "2.50%", "MemPerc": "10.00%", "MemUsage": "2MiB / 20MiB",
                      "NetIO": "1kB / 2kB", "BlockIO": "3MB / 4MB", "PIDs": "3"}))
elif a[:2] == ["container", "logs"]:
    print("stdout-log")
    print("stderr-log", file=sys.stderr)
elif a[:2] in (["container", "start"], ["container", "stop"], ["container", "restart"]):
    with (base / "mutations").open("a") as f:
        f.write(json.dumps(a) + "\n")
    print(a[-1])
elif a[:2] == ["exec", "-it"]:
    assert a[2:4] == ["--env", "TERM=dumb"]
    token = a[-1]
    if (base / "startup_slow").exists():
        time.sleep(10)
    remote = r"""
import fcntl, json, os, struct, subprocess, sys, termios
from pathlib import Path
base = Path(os.environ['FAKE_DOCKER_ROOT'])
pid = os.getpid()
(base / 'remote').write_text(str(pid))
stat = str(pid) + ' (fake shell) S ' + ' '.join(['0', str(pid), str(pid)] + ['0'] * 15 + ['123'])
print('\x1eDRAGONSTUI:' + sys.argv[1] + ':' + str(pid) + ':' + stat + '\x1f', flush=True)
print('fixture shell ready TERM=dumb', flush=True)
for line in sys.stdin:
    if line.strip() == 'sleep 60 & wait':
        child = subprocess.Popen(['sleep', '60'])
        (base / 'remote_child').write_text(str(child.pid))
        print('child-ready', flush=True)
        child.wait()
    if line.strip() == 'exit':
        break
    rows, cols, _, _ = struct.unpack('HHHH', fcntl.ioctl(0, termios.TIOCGWINSZ, b'\0' * 8))
    print('echo:' + line.strip() + ':size=' + str(rows) + 'x' + str(cols), flush=True)
"""
    signal.signal(signal.SIGWINCH, lambda *_: (base / "winch").write_text(str(time.monotonic())))
    child = subprocess.Popen([sys.executable, '-u', '-c', remote, token], start_new_session=True)
    sys.exit(child.wait())
elif a[:1] == ["exec"]:
    assert a[-5] == "dragonstui-cleanup"
    pid, start = int(a[-4]), a[-3]
    assert a[-2:] == [str(pid), str(pid)]
    assert str(pid) == (base / "remote").read_text() and start == "123"
    if (base / "cleanup_slow").exists():
        time.sleep(10)
    if (base / "cleanup_fail").exists():
        sys.exit(1)
    try:
        os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    (base / "cleaned").write_text(str(pid))
else:
    print("SECRET-DOCKER-ERROR", file=sys.stderr)
    sys.exit(1)
'''


class LogProjectionTests(unittest.TestCase):
    def test_log_projection_is_bounded_single_line_control_safe_text(self):
        provider = module.Provider.__new__(module.Provider)
        frames = []
        provider.emit = lambda message_type, **body: frames.append(body)
        provider.observe("docker.logs", {"container": CID, "tail": 100,
                                       "text": "first\nsecond\x1b[2J\tend\n"})
        self.assertEqual(len(frames), 2)
        self.assertEqual(frames[0]["observation"]["text"], "first")
        for frame in frames:
            self.assertFalse(any(ord(c) < 32 or 127 <= ord(c) < 160
                                 for c in frame["observation"]["text"]))
        frames.clear()
        provider.observe("docker.logs", {"container": CID, "tail": 1000,
                                       "text": "line\n" * 1000})
        self.assertLessEqual(len(frames), 32)


class Peer:
    def __init__(self, testcase, *, bound=True, poll=0, timeout=1.5, extra=(), copied=None):
        self.case = testcase
        command = [str(copied or ADAPTER), "--docker-bin", str(testcase.fake),
                   "--poll-interval", str(poll), "--command-timeout", str(timeout)]
        if bound:
            command += ["--container", "fixture"]
        command += list(extra)
        self.proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, env=dict(os.environ, FAKE_DOCKER_ROOT=str(testcase.base)))
        self.frames = queue.Queue()
        self.all_frames = []
        self.stderr = bytearray()
        def reader():
            for raw in self.proc.stdout:
                try:
                    self.frames.put(json.loads(raw))
                except Exception as exc:
                    self.frames.put(exc)
        def errors():
            self.stderr.extend(self.proc.stderr.read())
        self.reader = threading.Thread(target=reader, daemon=True)
        self.errors = threading.Thread(target=errors, daemon=True)
        self.reader.start()
        self.errors.start()
        testcase.addCleanup(self.close)
        self.send(type="hello", host_version="test")
        self.info = self.wait(lambda f: f.get("type") == "adapter_info")

    def send(self, **message):
        self.proc.stdin.write((json.dumps(dict(protocol=1, **message)) + "\n").encode())
        self.proc.stdin.flush()

    def wait(self, predicate, timeout=5):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                frame = self.frames.get(timeout=max(0.01, deadline - time.monotonic()))
            except queue.Empty:
                break
            if isinstance(frame, Exception):
                raise frame
            self.all_frames.append(frame)
            if predicate(frame):
                return frame
        raise AssertionError("missing frame; returncode=%r frames=%r stderr=%r" %
                             (self.proc.poll(), self.all_frames[-8:], bytes(self.stderr)))

    def rpc(self, rid, operation, payload=None, action=None):
        self.send(type="request", id=rid, operation=operation, payload=payload, action=action)
        return self.wait(lambda f: f.get("id") == rid and f["type"] in ("response", "error"))

    def open(self):
        self.send(type="session_open", id="open-1", capability="docker.exec", rows=24, columns=80)
        return self.wait(lambda f: f["type"] == "session_opened")["session_id"]

    def shutdown(self):
        self.send(type="shutdown")
        self.wait(lambda f: f["type"] == "shutdown_ack")
        self.case.assertEqual(self.proc.wait(timeout=5), 0)
        self.reader.join(1)
        remaining = []
        while not self.frames.empty():
            remaining.append(self.frames.get_nowait())
        self.case.assertEqual(remaining, [], "no frames may follow shutdown_ack")

    def close(self):
        if self.proc.poll() is None:
            try:
                self.proc.stdin.close()
            except OSError:
                pass
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        for pipe in (self.proc.stdin, self.proc.stdout, self.proc.stderr):
            pipe.close()
        self.reader.join(1)
        self.errors.join(1)


class DockerAdapterTests(unittest.TestCase):
    def test_sidecar_poll_interval_rejects_invalid_types_and_bounds(self):
        config = self.base / "docker-config.json"
        for value in (True, None, "0", -1, 0.5, 3601, 10 ** 400):
            with self.subTest(value_type=type(value).__name__):
                config.write_text(json.dumps({"poll_interval": value}))
                with patch.object(module.sys, "argv", [str(ADAPTER)]), \
                        patch.object(module.sys, "stderr", io.StringIO()), \
                        patch.object(module, "__file__", str(self.base / "dragonstui-docker")), \
                        patch.object(module, "Provider") as provider:
                    with self.assertRaises(SystemExit) as caught:
                        module.main()
                    self.assertEqual(caught.exception.code, 2)
                    provider.assert_not_called()

    def test_sidecar_poll_interval_and_cli_precedence(self):
        config = self.base / "docker-config.json"
        config.write_text(json.dumps({"poll_interval": 0}))
        for extra, expected in (([], 0), (["--poll-interval", "60"], 60)):
            with patch.object(module.sys, "argv", [str(ADAPTER), *extra]), \
                    patch.object(module, "__file__", str(self.base / "dragonstui-docker")), \
                    patch.object(module, "Provider") as provider:
                provider.return_value.run.return_value = 0
                self.assertEqual(module.main(), 0)
                self.assertEqual(provider.call_args.args[0].poll_interval, expected)

    def test_cli_cleanup_falls_back_to_owned_pid_when_group_signal_is_denied(self):
        process = MagicMock()
        process.pid = 1234
        process.poll.return_value = None
        process.wait.return_value = -9
        with patch.object(module.os, "killpg", side_effect=PermissionError):
            self.assertEqual(module.stop_cli(process), -9)
        process.kill.assert_called_once_with()
        process.wait.assert_called_once_with(timeout=1)

    def test_cli_cleanup_does_not_signal_an_already_reaped_child(self):
        process = MagicMock()
        process.poll.return_value = 0
        process.wait.return_value = 0
        with patch.object(module.os, "killpg") as signal_group:
            self.assertEqual(module.stop_cli(process), 0)
        signal_group.assert_not_called()
        process.kill.assert_not_called()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dragonstui-docker-unit-")
        self.base = Path(self.temp.name)
        self.fake = self.base / "docker"
        self.fake.write_text(FAKE)
        self.fake.chmod(0o755)
        self.addCleanup(self.temp.cleanup)
        self.addCleanup(self.clean_remote)

    def clean_remote(self):
        state = self.base / "remote"
        if state.exists():
            try:
                os.killpg(int(state.read_text()), signal.SIGKILL)
            except ProcessLookupError:
                pass

    def calls(self):
        path = self.base / "calls"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def test_unbound_handshake_list_and_explicit_read(self):
        peer = Peer(self, bound=False)
        self.assertEqual(peer.info["sessions"], [])
        self.assertEqual([a["id"] for a in peer.info["actions"]], ["docker.list"])
        self.assertNotIn("docker.start", peer.info["capabilities"])
        self.assertEqual(peer.rpc("list", "docker.list")["payload"]["containers"][0]["ID"], CID)
        self.assertEqual(peer.rpc("missing", "docker.inspect")["type"], "error")
        result = peer.rpc("inspect", "docker.inspect", {"container": OTHER})
        self.assertEqual(result["payload"]["id"], OTHER)
        peer.shutdown()

    def test_bound_actions_are_immutable_confirmed_and_deduplicated(self):
        peer = Peer(self)
        self.assertEqual([a["id"] for a in peer.info["actions"]],
                         [*module.READS, *module.MUTATIONS])
        for action in peer.info["actions"]:
            self.assertEqual(action["confirmation_required"], action["id"] in ("docker.start", "docker.stop", "docker.restart"))
        self.assertEqual(peer.rpc("no-action", "docker.start")["type"], "error")
        self.assertEqual(peer.rpc("wrong-action", "docker.stop", action="docker.start")["type"], "error")
        self.assertEqual(peer.rpc("redirect", "docker.start", {"container": OTHER}, "docker.start")["type"], "error")
        self.assertEqual(peer.rpc("once", "docker.start", action="docker.start")["type"], "response")
        self.assertEqual(peer.rpc("once", "docker.start", action="docker.start")["type"], "error")
        self.assertEqual(peer.rpc("stop", "docker.stop", action="docker.stop")["type"], "response")
        self.assertEqual(peer.rpc("restart", "docker.restart", action="docker.restart")["type"], "response")
        mutations = [json.loads(x) for x in (self.base / "mutations").read_text().splitlines()]
        self.assertEqual(len(mutations), 3)
        self.assertTrue(all(call[-1] == CID for call in mutations))
        self.assertEqual(mutations[1][2:4], ["--time", "1"])
        self.assertEqual(self.calls()[0][-1], "fixture")
        peer.shutdown()

    def test_inspect_projection_metrics_and_explicit_both_log_streams(self):
        peer = Peer(self)
        result = peer.rpc("inspect", "docker.inspect", action="docker.inspect")["payload"]
        self.assertNotIn("Config", result)
        self.assertNotIn("Env", json.dumps(result))
        metrics = peer.rpc("metrics", "docker.metrics")["payload"]["metrics"]
        self.assertEqual(metrics["memory_bytes"]["value"], 2097152)
        self.assertEqual(metrics["network_rx_bytes"]["value"], 1000)
        frame = peer.wait(lambda f: f.get("observation", {}).get("name") == "cpu_percent")
        self.assertEqual(frame["observation"]["value"], 2.5)
        self.assertIsInstance(frame["observation"]["value"], float)
        self.assertFalse(any("logs" in call for call in self.calls()))
        logs = peer.rpc("logs", "docker.logs", {"tail": 2})["payload"]
        self.assertIn("stdout-log", logs["text"])
        self.assertIn("stderr-log", logs["text"])
        self.assertEqual(peer.rpc("bad-tail", "docker.logs", {"tail": True})["type"], "error")
        peer.shutdown()

    def test_default_observation_polling_never_fetches_logs(self):
        peer = Peer(self, poll=1)
        peer.wait(lambda f: f.get("observation", {}).get("type") == "metric")
        self.assertFalse(any("logs" in call for call in self.calls()))
        peer.shutdown()

    def test_pty_input_resize_close_verifies_remote_shell(self):
        peer = Peer(self)
        sid = peer.open()
        peer.send(type="session_input", session_id=sid, data="hello\n")
        peer.wait(lambda f: "echo:hello:size=24x80" in f.get("data", ""))
        self.assertTrue((self.base / "winch").exists(), "initial size must notify Docker CLI")
        first_winch = (self.base / "winch").read_text()
        peer.send(type="session_resize", session_id=sid, rows=33, columns=101)
        time.sleep(0.1)  # One bounded PTY worker tick, not a retry loop.
        peer.send(type="session_input", session_id=sid, data="resized\n")
        peer.wait(lambda f: "echo:resized:size=33x101" in f.get("data", ""))
        self.assertNotEqual((self.base / "winch").read_text(), first_winch)
        peer.send(type="session_close", session_id=sid)
        peer.wait(lambda f: f["type"] == "session_exit")
        self.assertTrue((self.base / "cleaned").exists())
        peer.shutdown()

    def test_pty_eof_and_signals_cleanup_remote_process(self):
        for ending in ("eof", signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
            with self.subTest(ending=ending):
                (self.base / "cleaned").unlink(missing_ok=True)
                peer = Peer(self)
                peer.open()
                if ending == "eof":
                    peer.proc.stdin.close()
                else:
                    peer.proc.send_signal(ending)
                self.assertEqual(peer.proc.wait(timeout=5), 0)
                self.assertTrue((self.base / "cleaned").exists())
                peer.close()

    def test_shutdown_active_session_and_natural_exit(self):
        peer = Peer(self)
        sid = peer.open()
        peer.send(type="session_input", session_id=sid, data="exit\n")
        peer.wait(lambda f: f["type"] == "session_exit")
        peer.send(type="session_open", id="open-2", capability="docker.exec", rows=12, columns=40)
        peer.wait(lambda f: f["type"] == "session_opened")
        peer.shutdown()
        self.assertTrue((self.base / "cleaned").exists())

    def test_ordinary_background_child_cleanup_on_close_eof_shutdown(self):
        for ending in ("close", "eof", "shutdown"):
            with self.subTest(ending=ending):
                peer = Peer(self)
                sid = peer.open()
                peer.send(type="session_input", session_id=sid, data="sleep 60 & wait\n")
                peer.wait(lambda f: "child-ready" in f.get("data", ""))
                child = int((self.base / "remote_child").read_text())
                if ending == "close":
                    peer.send(type="session_close", session_id=sid)
                    peer.wait(lambda f: f["type"] == "session_exit")
                    peer.shutdown()
                elif ending == "eof":
                    peer.proc.stdin.close()
                    self.assertEqual(peer.proc.wait(timeout=2), 0)
                else:
                    peer.shutdown()
                # The fake CLI owns only this isolated group. On macOS launchd
                # reaps the orphan; a brief bounded wait accounts for scheduling.
                deadline = time.monotonic() + 1
                while time.monotonic() < deadline:
                    try:
                        os.kill(child, 0)
                    except ProcessLookupError:
                        break
                    time.sleep(0.02)
                else:
                    self.fail("ordinary child survived group cleanup")
                peer.close()

    def test_shutdown_during_unidentified_startup_is_bounded_and_honest(self):
        peer = Peer(self, timeout=8)
        (self.base / "startup_slow").touch()
        peer.send(type="session_open", id="open-slow", capability="docker.exec", rows=24, columns=80)
        started = time.monotonic()
        peer.send(type="shutdown")
        peer.wait(lambda f: f.get("code") == "cleanup_unverified", timeout=2)
        self.assertEqual(peer.proc.wait(timeout=0.5), 1)
        self.assertLess(time.monotonic() - started, 2)
        self.assertFalse(any(f["type"] == "shutdown_ack" for f in peer.all_frames))

    def test_cleanup_daemon_stall_respects_host_shutdown_budget(self):
        peer = Peer(self, timeout=8)
        peer.open()
        (self.base / "cleanup_slow").touch()
        started = time.monotonic()
        peer.send(type="shutdown")
        peer.wait(lambda f: f.get("code") == "cleanup_unverified", timeout=2)
        self.assertEqual(peer.proc.wait(timeout=0.5), 1)
        self.assertLess(time.monotonic() - started, 2)

    def test_cleanup_failure_never_claims_session_exit_or_shutdown_ack(self):
        peer = Peer(self, timeout=0.4)
        peer.open()
        (self.base / "cleanup_fail").touch()
        peer.send(type="shutdown")
        peer.wait(lambda f: f.get("code") == "cleanup_unverified")
        self.assertEqual(peer.proc.wait(timeout=5), 1)
        peer.reader.join(1)
        while not peer.frames.empty():
            peer.all_frames.append(peer.frames.get_nowait())
        self.assertFalse(any(f["type"] in ("shutdown_ack", "session_exit") for f in peer.all_frames))

    def test_slow_command_keeps_shutdown_responsive_and_output_is_bounded(self):
        peer = Peer(self, bound=False, timeout=0.3)
        (self.base / "slow").touch()
        peer.send(type="request", id="slow", operation="docker.list", payload=None)
        started = time.monotonic()
        peer.shutdown()
        self.assertLess(time.monotonic() - started, 2)
        (self.base / "slow").unlink()
        (self.base / "flood").touch()
        peer2 = Peer(self, bound=False)
        outcome = peer2.rpc("flood", "docker.list")
        self.assertEqual(outcome["type"], "error")
        self.assertIn("limit", outcome["message"])
        peer2.shutdown()

    def test_malformed_frame_and_strict_geometry_payload_and_identifiers(self):
        peer = Peer(self)
        for index, payload in enumerate(({"container": "-x"}, {"container": "short"}, {"args": ["rm"]}, [])):
            self.assertEqual(peer.rpc("bad-%d" % index, "docker.inspect", payload)["type"], "error")
        peer.send(type="session_open", id="bad-size", capability="docker.exec", rows=True, columns=80)
        self.assertEqual(peer.wait(lambda f: f.get("id") == "bad-size")["type"], "error")
        peer.proc.stdin.write(b'{"type":"hello","protocol":1,"protocol":1}\n')
        peer.proc.stdin.flush()
        self.assertEqual(peer.wait(lambda f: f.get("code") == "invalid_request")["type"], "error")
        peer.shutdown()

    def test_sidecar_configuration_and_cli_override_context(self):
        copied = self.base / "dragonstui-docker"
        shutil.copy2(ADAPTER, copied)
        (self.base / "docker-config.json").write_text(json.dumps({"container": "sidecar-target", "context": "sidecar-context",
                                                                "docker_bin": str(self.fake)}))
        peer = Peer(self, bound=False, copied=copied, extra=("--context", "cli-context"))
        self.assertTrue(peer.info["sessions"])
        self.assertEqual(self.calls()[0][:2], ["--context", "cli-context"])
        self.assertEqual(self.calls()[0][-1], "sidecar-target")
        peer.shutdown()
        (self.base / "docker-config.json").write_text('{"unknown":"value"}')
        result = subprocess.run([str(copied)], input=b"", capture_output=True, timeout=3)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, b"")

    def test_lifetime_admission_does_not_evict_replay_protection(self):
        from argparse import Namespace
        provider = module.Provider(Namespace(docker_bin=str(self.fake), command_timeout=1, context=None,
                                             container=None, poll_interval=0))
        try:
            for n in range(module.REQUEST_LIMIT):
                provider.admit_id({"id": "r-%d" % n})
            with self.assertRaises(module.ProviderError):
                provider.admit_id({"id": "new"})
            with self.assertRaises(module.ProviderError):
                provider.admit_id({"id": "r-0"})
            self.assertEqual(len(provider.seen), module.REQUEST_LIMIT)
        finally:
            provider.pool.shutdown()

    def test_decoder_numbers_unicode_and_units(self):
        for text in ('{"x":NaN}', '{"x":1e999}', '{"x":"\\ud800"}', '{"x":1,"x":2}'):
            with self.assertRaises((ValueError, UnicodeError)):
                module.json_load(text)
        self.assertEqual(module.quantity("2.5GiB"), 2.5 * 1024 ** 3)
        for units, base in ((('B', 'kB', 'MB', 'GB', 'TB', 'PB', 'EB'), 1000),
                            (('B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB', 'EiB'), 1024)):
            for power, unit in enumerate(units):
                with self.subTest(unit=unit):
                    self.assertEqual(module.quantity(" 2.5" + unit + " "), 2.5 * base ** power)
        with self.assertRaises(module.ProviderError):
            module.quantity("NaNB")


if __name__ == "__main__":
    unittest.main()
