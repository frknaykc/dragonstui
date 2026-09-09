#!/usr/bin/env python3
"""Opt-in real Docker acceptance. Mutates only a newly created, labelled fixture.

Requires an already-local image containing /bin/sh, setsid, stty, sleep and kill.
Never pulls images, uses host mounts, or touches a pre-existing container.
"""
from __future__ import annotations

import argparse
from collections import deque
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import secrets
import signal
import subprocess
import termios
import tempfile
import time
import uuid

from docker_adapter_package import build
from adapter_conformance_protocol import validate_message
from ecosystem_fixture import query
import showcase_pty_smoke as h
from reference_mock_pty_smoke import assert_ansi_restored

LABEL = "org.dragonstui.r4.fixture"
FULL_ID = re.compile(r"[0-9a-f]{64}")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def docker(*args, timeout=15):
    result = subprocess.run(["docker", *args], capture_output=True, timeout=timeout)
    if result.returncode:
        # Commands contain only task-owned IDs, no user credentials or config.
        raise RuntimeError("fixture Docker command failed: " + result.stderr.decode(errors="replace")[:1024])
    return result.stdout.decode("utf-8", errors="replace").strip()


class DockerFixture:
    def __init__(self, image):
        self.image = image
        self.name = "dragonstui-r4-" + uuid.uuid4().hex
        self.id = None

    def __enter__(self):
        # Check presence before create; --pull=never independently prevents download.
        self.image_id = docker("image", "inspect", "--format", "{{.Id}}", self.image)
        try:
            self.id = docker("create", "--pull=never", "--name", self.name,
                             "--label", LABEL + "=" + self.name,
                             "--init", "--network", "none", "--read-only", "--cap-drop", "ALL",
                             "--security-opt", "no-new-privileges", "--pids-limit", "64",
                             "--memory", "128m", "--cpus", "0.5", "--user", "65534:65534",
                             "--tmpfs", "/tmp:rw,nosuid,nodev,size=8m,mode=1777",
                             "--tmpfs", "/data:rw,nosuid,nodev,size=64k,mode=1777",
                             "--entrypoint", "/bin/sh", self.image, "-c",
                             "printf 'dragonstui-r4-log-ready\\n'; trap 'exit 0' TERM INT; while :; do sleep 1; done").strip()
            require(bool(FULL_ID.fullmatch(self.id)), "Docker create did not return a full container ID")
            docker("start", self.id)
            docker("exec", self.id, "/bin/sh", "-c", "command -v setsid; command -v stty")
        except BaseException:
            if not self.id or not FULL_ID.fullmatch(self.id):
                # An Engine-side create may succeed despite a lost CLI response.
                # Recover only this run's exact name AND private random label.
                recovered = docker("ps", "--all", "--no-trunc", "--filter",
                                   "label=" + LABEL + "=" + self.name, "--filter",
                                   "name=^/" + self.name + "$", "--format", "{{.ID}}").strip()
                require(not recovered or bool(FULL_ID.fullmatch(recovered)),
                        "uncertain fixture creation: ownership lookup was ambiguous")
                self.id = recovered or None
            self.__exit__(None, None, None)
            raise
        return self

    def __exit__(self, *_):
        if self.id is None:
            return
        # Revalidate full identity and ownership before any cleanup mutation.
        owner = docker("inspect", "--format", '{{index .Config.Labels "' + LABEL + '"}}', self.id)
        require(owner == self.name, "refusing cleanup: fixture ownership changed")
        docker("rm", "--force", "--volumes", self.id)
        remaining = docker("ps", "--all", "--quiet", "--filter", "label=" + LABEL + "=" + self.name)
        require(not remaining, "task-owned Docker container leaked")

    def state(self):
        return docker("inspect", "--format", "{{.State.Status}}", self.id)

    def wait_state(self, expected, timeout=6):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.state() == expected:
                return
            time.sleep(0.05)
        raise RuntimeError("fixture state did not become " + expected)

    def require_pid_gone(self, pid, timeout=5):
        assert self.id is not None
        require(bool(re.fullmatch(r"[1-9][0-9]*", str(pid))), "invalid fixture PID")
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            result = subprocess.run(["docker", "exec", self.id, "/bin/sh", "-c",
                                     'test ! -e /proc/"$1"', "r4-probe", str(pid)],
                                    capture_output=True, timeout=3)
            if result.returncode == 0:
                return
            time.sleep(0.05)
        raise RuntimeError("fixture exec process remained after close")


class Peer:
    """Bounded real protocol peer; test-only, not another application manager."""
    def __init__(self, executable, *args):
        self.process = subprocess.Popen([str(executable), *args], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        assert self.process.stdin is not None
        assert self.process.stdout is not None
        assert self.process.stderr is not None
        self.stdin = self.process.stdin
        self.stdout = self.process.stdout
        self.stderr = self.process.stderr
        self.selector = selectors.DefaultSelector()
        for stream in (self.stdout, self.stderr):
            os.set_blocking(stream.fileno(), False)
            self.selector.register(stream, selectors.EVENT_READ)
        self.buffer = bytearray()
        self.diagnostics = bytearray()
        self.history = deque(maxlen=512)
        self.sequence = 0

    def send(self, kind, **fields):
        data = json.dumps({"type": kind, "protocol": 1, **fields}).encode() + b"\n"
        require(len(data) <= 4096, "test outgoing frame too large")
        self.stdin.write(data)
        self.stdin.flush()

    def next(self, timeout: float = 8):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if b"\n" in self.buffer:
                raw, _, remainder = self.buffer.partition(b"\n")
                self.buffer = bytearray(remainder)
                require(len(raw) <= 65536, "oversized adapter frame")
                message = json.loads(raw)
                validate_message(message)
                self.history.append(message)
                return message
            for key, _ in self.selector.select(max(0, deadline - time.monotonic())):
                chunk = os.read(key.fd, 8192)
                if not chunk:
                    self.selector.unregister(key.fileobj)
                    continue
                if key.fileobj is self.stdout:
                    self.buffer.extend(chunk)
                    require(len(self.buffer) <= 131072, "adapter framing overflow")
                else:
                    self.diagnostics.extend(chunk)
                    self.diagnostics = self.diagnostics[-4096:]
            if not self.selector.get_map() and b"\n" not in self.buffer:
                raise RuntimeError("adapter EOF before expected result; stderr=" + self.diagnostics.decode(errors="replace"))
        raise RuntimeError("adapter protocol deadline expired")

    def expect(self, predicate, timeout=8):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            message = self.next(max(0.01, deadline - time.monotonic()))
            if predicate(message):
                return message
            if message.get("type") == "error" and message.get("id") is None:
                raise RuntimeError("uncorrelated provider error: " + message.get("code", "unknown"))
        raise RuntimeError("expected adapter frame absent")

    def hello(self):
        self.send("hello", host_version="0.1.0")
        return self.expect(lambda m: m["type"] == "adapter_info")

    def request(self, operation, payload=None, action=None):
        self.sequence += 1
        request_id = "r4-" + str(self.sequence)
        fields = dict(id=request_id, operation=operation, payload=payload)
        if action is not None:
            fields["action"] = action
        self.send("request", **fields)
        return self.expect(lambda m: m.get("id") == request_id and m["type"] in ("response", "error"))

    def session_text(self, session_id, needle, timeout=8):
        text = ""
        deadline = time.monotonic() + timeout
        while needle not in text and time.monotonic() < deadline:
            message = self.next(max(0.01, deadline - time.monotonic()))
            if message.get("session_id") == session_id and message["type"] == "session_output":
                text = (text + message["data"])[-32768:]
            elif message.get("session_id") == session_id and message["type"] == "session_exit":
                raise RuntimeError("exec ended before expected output")
        require(needle in text, "expected exec text absent")
        return text

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=5)
        self.selector.close()
        for stream in (self.stdin, self.stdout, self.stderr):
            if not stream.closed:
                stream.close()

    def shutdown(self):
        self.send("shutdown")
        self.stdin.close()
        self.expect(lambda m: m["type"] == "shutdown_ack", timeout=10)
        self.process.wait(timeout=5)
        require(self.process.returncode == 0, "provider shutdown was not successful")
        require(not self.buffer.strip(), "protocol output after shutdown ACK")
        trailing = self.stdout.read()
        require(not trailing, "trailing stdout after ACK")


def session_open(peer):
    peer.sequence += 1
    request_id = "open-" + str(peer.sequence)
    peer.send("session_open", id=request_id, capability="docker.exec", rows=24, columns=80)
    opened = peer.expect(lambda m: m.get("id") == request_id and m["type"] in ("session_opened", "error"))
    require(opened["type"] == "session_opened", "real Docker session open rejected")
    return opened["session_id"]


def shell_pid(peer, session_id):
    peer.send("session_input", session_id=session_id, data="printf '__R4_PID__%s\\n' $$\r")
    text = peer.session_text(session_id, "__R4_PID__")
    deadline = time.monotonic() + 5
    while not re.search(r"__R4_PID__([0-9]+)", text) and time.monotonic() < deadline:
        message = peer.next(5)
        if message.get("session_id") == session_id and message["type"] == "session_output":
            text = (text + message["data"])[-32768:]
    match = re.search(r"__R4_PID__([0-9]+)", text)
    require(match is not None, "exec shell PID marker absent")
    assert match is not None
    return match.group(1)


def protocol_acceptance(executable, fixture, output_dir):
    peer = Peer(executable, "--container", fixture.id)
    checks = []
    try:
        info = peer.hello()
        require(info["id"] == "docker", "wrong Docker adapter identity")
        actions = {a["id"]: a for a in info.get("actions", [])}
        require("docker.exec" in {s["capability"] for s in info.get("sessions", [])}, "PTY capability absent")
        for action in ("docker.start", "docker.stop", "docker.restart"):
            require(actions[action].get("confirmation_required") is True, "mutation action missing UI confirmation")
        for action in ("docker.list", "docker.inspect", "docker.logs", "docker.metrics"):
            reply = peer.request(actions[action]["operation"], {}, action)
            require(reply["type"] == "response", "read action failed: " + action)
        require("dragonstui-r4-log-ready" in json.dumps(list(peer.history)), "real container log absent")
        require("observation" in json.dumps(list(peer.history)), "typed observations absent")
        checks.append("real list/inspect/log/metric via validated protocol and declared actions")
        bad = peer.request("not.declared", {})
        require(bad["type"] == "error", "unknown operation accepted")
        for action, state in (("docker.stop", "exited"), ("docker.start", "running"),
                              ("docker.restart", "running")):
            reply = peer.request(actions[action]["operation"], {}, action)
            require(reply["type"] == "response", "mutation action failed: " + action)
            fixture.wait_state(state)
        checks.append("start/stop/restart against owned fixture; unknown operation rejected")

        session_id = session_open(peer)
        pid = shell_pid(peer, session_id)
        peer.send("session_resize", session_id=session_id, rows=33, columns=101)
        peer.send("session_input", session_id=session_id, data="stty size\r")
        peer.session_text(session_id, "33 101")
        peer.send("session_close", session_id=session_id)
        peer.expect(lambda m: m["type"] == "session_exit" and m["session_id"] == session_id)
        fixture.require_pid_gone(pid)
        checks.append("real PTY shell/input/resize/close; remote shell reaped")

        # A foreground wait and its background child must not outlive explicit close.
        session_id = session_open(peer)
        pid = shell_pid(peer, session_id)
        peer.send("session_input", session_id=session_id,
                  data="sleep 60 & printf '__R4_CHILD__%s\\n' $!; wait\r")
        text = ""
        deadline = time.monotonic() + 5
        while not re.search(r"__R4_CHILD__([0-9]+)", text) and time.monotonic() < deadline:
            message = peer.next(5)
            if message.get("session_id") == session_id and message["type"] == "session_output":
                text = (text + message["data"])[-32768:]
        child = re.search(r"__R4_CHILD__([0-9]+)", text)
        require(child is not None, "exec child PID marker absent")
        assert child is not None
        peer.send("session_close", session_id=session_id)
        peer.expect(lambda m: m["type"] == "session_exit" and m["session_id"] == session_id)
        fixture.require_pid_gone(pid)
        fixture.require_pid_gone(child.group(1))
        checks.append("exec close terminates owned shell and child, not only Docker CLI")

        session_id = session_open(peer)
        pid = shell_pid(peer, session_id)
        peer.shutdown()
        fixture.require_pid_gone(pid)
        checks.append("shutdown while PTY active: remote cleanup and final ACK")
    except BaseException:
        (output_dir / "failed-protocol.json").write_text(json.dumps(list(peer.history), indent=2))
        (output_dir / "failed-provider-stderr.txt").write_bytes(peer.diagnostics)
        (output_dir / "failed-fixture-processes.txt").write_text(
            docker("top", fixture.id, "-eo", "pid,pgid,sid,args"))
        raise
    finally:
        peer.close()
    # EOF is an independent lifecycle boundary, not a second shutdown request.
    peer = Peer(executable, "--container", fixture.id)
    try:
        peer.hello()
        session_id = session_open(peer)
        pid = shell_pid(peer, session_id)
        peer.stdin.close()
        peer.process.wait(timeout=8)
        fixture.require_pid_gone(pid)
        checks.append("stdin EOF closes owned remote exec")
    finally:
        peer.close()
    return checks


def controller_operation(endpoint, action_id):
    started = query(endpoint, {"command": "start_operation", "id": "docker",
                               "action_id": action_id, "payload": None})["Operation"]
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        operations = query(endpoint, {"command": "operations"})["Operations"]
        result = next(op for op in operations if op["id"] == started["id"])
        state = result["state"]["state"]
        if state == "succeeded":
            return result
        if state == "failed":
            raise RuntimeError("controller operation failed: " + action_id + ": " +
                               result["state"].get("code", "unknown"))
        time.sleep(0.04)
    raise RuntimeError("controller operation did not reach terminal state")


def showcase_acceptance(showcase, root, endpoint, fixture, output_dir):
    master, slave = os.openpty()
    slave_name = os.ttyname(slave)
    original = termios.tcgetattr(slave)
    h.set_size(slave, 160, 55)
    output = bytearray()
    process = subprocess.Popen([str(showcase), "--adapter-root", str(root)],
        stdin=slave, stdout=slave, stderr=slave,
        preexec_fn=h.establish_controlling_terminal, close_fds=True)
    os.close(slave)
    frames = {}
    try:
        def send(data):
            h.send(master, output, data)

        def current(text, timeout=5):
            h.wait_for_current_text(master, output, text, timeout, "missing Docker frame: " + text)

        def invoke_read(action_id):
            before = {op["id"] for op in query(endpoint, {"command": "operations"})["Operations"]}
            send(b"\r")
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                operations = query(endpoint, {"command": "operations"})["Operations"]
                fresh = [op for op in operations if op["id"] not in before]
                for operation in fresh:
                    require(operation["action_id"] == action_id, "TUI invoked a different action")
                    state = operation["state"]["state"]
                    if state == "succeeded":
                        return
                    require(state != "failed", "TUI read operation failed: " + action_id)
                h.read_available(master, output, 0.05)
            raise RuntimeError("no new completed TUI operation: " + action_id)

        h.wait_for_text(master, output, "we are the recall", 4, "showcase splash absent")
        send(b"\r")
        current("Static benchmark context")
        send(b"8")
        current("Docker")
        frames["adapters"] = h.visible_text(output)
        send(b"a")
        current("Adapter Actions")
        current("docker.list")
        invoke_read("docker.list")
        current("Operation succeeded")
        # Select stop by declaration order, not by parsing domain names in the UI.
        actions = query(endpoint, {"command": "actions", "id": "docker"})["Actions"]
        stop_index = next(i for i, action in enumerate(actions) if action["id"] == "docker.stop")
        for _ in range(stop_index):
            send(b"\x1b[B")
        before = query(endpoint, {"command": "operations"})["Operations"]
        send(b"\r")
        current("Confirm Adapter Action")
        frames["confirmation"] = h.visible_text(output)
        send(b"\x1b")
        current("Adapter Actions")
        after = query(endpoint, {"command": "operations"})["Operations"]
        require(len(after) == len(before) and fixture.state() == "running", "cancelled mutation was dispatched")
        send(b"\r\r")
        current("Operation succeeded")
        fixture.wait_state("exited")
        controller_operation(endpoint, "docker.start")
        fixture.wait_state("running")
        # LiveData drains are consumer-owned; produce logs after this UI attached.
        logs_index = next(i for i, action in enumerate(actions) if action["id"] == "docker.logs")
        for _ in range(stop_index - logs_index):
            send(b"\x1b[A")
        invoke_read("docker.logs")
        current("Operation succeeded")
        send(b"a")
        send(b"o")
        current("Observability · Logs")
        current("dragonstui-r4-log-ready")
        frames["logs"] = h.visible_text(output)
        send(b"2")
        current("Observability · Metrics")
        controller_operation(endpoint, "docker.metrics")
        # The generic chart selects the alphabetically first metric series.
        current("block_read_bytes")
        frames["metrics"] = h.visible_text(output)
        send(b"o")
        send(b"h")
        current("Interactive Sessions")
        current("docker.exec")
        send(b"\r")
        h.wait_for_session_host(master, output, 4, "Docker shell did not open")
        # q and Ctrl-C are forwarded in raw shell mode; use Alt+x for explicit close.
        send(b"printf 'qQ__R4_TUI__%s\\n' $$\r")
        current("__R4_TUI__")
        deadline = time.monotonic() + 5
        match = None
        while time.monotonic() < deadline:
            match = re.search(r"__R4_TUI__([0-9]+)", h.visible_text(output))
            if match:
                break
            h.read_available(master, output, 0.05)
        require(match is not None, "TUI shell PID output absent")
        assert match is not None
        pid = match.group(1)
        frames["shell"] = h.visible_text(output)
        send(b"sleep 60\r")
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            if "sleep 60" in docker("top", fixture.id, "-eo", "pid,args"):
                break
            h.read_available(master, output, 0.05)
        else:
            raise RuntimeError("foreground sleep did not start before Ctrl-C")
        send(b"\x03")
        send(b"printf INT_%s $((6*7))\r")
        current("INT_42")
        require(process.poll() is None, "Ctrl-C quit the showcase instead of reaching the shell")
        frames["interrupt"] = h.visible_text(output)
        send(b"\x1bx")
        current("Interactive Sessions")
        current("docker.exec")
        fixture.require_pid_gone(pid)
        # Exit outside raw session mode, then prove the same PTY remains usable.
        send(b"h")
        send(b"q")
        deadline = time.monotonic() + 6
        while process.poll() is None and time.monotonic() < deadline:
            h.read_available(master, output, 0.05)
        require(process.poll() == 0, "showcase did not exit cleanly")
        h.drain_for(master, output, 0.1)
        require(termios.tcgetattr(master) == original, "showcase did not restore termios")
        assert_ansi_restored(output)
        h.prove_pty_usable_after_exit(master, slave_name, output)
        return ["showcase real action cancel/confirm, logs/metrics, shell/close and terminal restoration"]
    except BaseException:
        (output_dir / "failed-screen.txt").write_text(h.visible_text(output))
        (output_dir / "failed-fixture-state.txt").write_text(fixture.state())
        (output_dir / "failed-operations.json").write_text(json.dumps(
            query(endpoint, {"command": "operations"})["Operations"], indent=2))
        raise
    finally:
        (output_dir / "frames.json").write_text(json.dumps(frames, indent=2))
        (output_dir / "showcase.raw").write_bytes(output)
        try:
            if process.poll() is None:
                process.terminate()
                deadline = time.monotonic() + 4
                while process.poll() is None and time.monotonic() < deadline:
                    h.read_available(master, output, 0.05)
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=2)
        finally:
            os.close(master)


def installed_acceptance(executable, controller, showcase, fixture, output_dir):
    with tempfile.TemporaryDirectory(prefix="dragonstui-r4-") as temp:
        base = Path(temp).resolve()
        package = build(base / "package", executable)
        root = base / "adapters"
        subprocess.run([str(controller), "--root", str(root), "install", "docker",
                        "--registry", package["registry"]], check=True, capture_output=True, timeout=10)
        installed = root / "docker" / "dragonstui-docker"
        require(hashlib.sha256(installed.read_bytes()).hexdigest() == package["sha256"],
                "installed Docker artifact differs from checksum")
        require((root / "docker" / "adapter-install.json").is_file(), "install provenance missing")
        require(not (root / ".controller").exists(), "install unexpectedly started controller")
        # Explicit requests drive acceptance; polling must not evict log evidence
        # from bounded history while switching between the inspector views.
        (root / "docker" / "docker-config.json").write_text(
            json.dumps({"container": fixture.id, "poll_interval": 0}))
        daemon = subprocess.Popen([str(controller), "--root", str(root), "controller-daemon"],
            env={**os.environ, "DRAGONSTUI_CONTROLLER_TOKEN": secrets.token_hex(32)},
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            start_new_session=True)
        endpoint = None
        try:
            endpoint, _ = h.wait_for_controller_endpoint(root, 4)
            query(endpoint, {"command": "management", "request": {"operation": "start", "id": "docker"}})
            diagnostics = query(endpoint, {"command": "diagnostics", "id": "docker"})["Diagnostics"]
            require(diagnostics["state"] == "running", "installed Docker adapter did not start")
            for action in ("docker.list", "docker.inspect", "docker.logs", "docker.metrics", "docker.restart"):
                controller_operation(endpoint, action)
            fixture.wait_state("running")
            checks = ["real checksummed single-file install and controller-owned Docker operations within host deadlines"]
            checks.extend(showcase_acceptance(showcase, root, endpoint, fixture, output_dir))
            h.shutdown_controller(endpoint)
            daemon.wait(timeout=5)
            require(daemon.returncode == 0, "controller shutdown failed")
            h.assert_no_fixture_processes(root)
            return checks
        finally:
            try:
                if endpoint is not None and daemon.poll() is None:
                    h.shutdown_controller(endpoint)
            finally:
                if daemon.poll() is None:
                    try:
                        daemon.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        daemon.terminate()
                        daemon.wait(timeout=2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True, help="already-local disposable fixture image; never pulled")
    parser.add_argument("--output", type=Path, required=True, help="new evidence directory")
    parser.add_argument("--adapter", type=Path, default=Path("adapters/docker/dragonstui-docker"))
    parser.add_argument("--controller", type=Path, default=Path("target/debug/dragonstui-adapter"))
    parser.add_argument("--showcase", type=Path, default=Path("target/debug/dragonstui-showcase"))
    args = parser.parse_args()
    args.output.mkdir(mode=0o700, parents=False, exist_ok=False)
    report = {"status": "failed", "checks": [], "limits": ["text PTY, not terminal emulation",
              "local image only; no pre-existing container mutations"]}
    try:
        executable = args.adapter.resolve(strict=True)
        report["provider_sha256"] = hashlib.sha256(executable.read_bytes()).hexdigest()
        inputs = {"controller": args.controller.resolve(strict=True),
                  "showcase": args.showcase.resolve(strict=True),
                  "showcase_source": Path("src/bin/dragonstui_showcase.rs").resolve(strict=True),
                  "harness": Path(__file__).resolve(strict=True)}
        report["input_sha256"] = {name: hashlib.sha256(path.read_bytes()).hexdigest()
                                  for name, path in inputs.items()}
        with DockerFixture(args.image) as fixture:
            report["image"] = args.image
            report["image_id"] = fixture.image_id
            report["checks"].extend(protocol_acceptance(executable, fixture, args.output))
            report["checks"].extend(installed_acceptance(executable, args.controller.resolve(strict=True),
                                   args.showcase.resolve(strict=True), fixture, args.output))
        report["checks"].append("task-owned fixture removed after acceptance")
        require(hashlib.sha256(executable.read_bytes()).hexdigest() == report["provider_sha256"],
                "provider source changed during acceptance")
        require({name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in inputs.items()}
                == report["input_sha256"], "acceptance source/binaries changed during run")
        report["status"] = "passed"
    except BaseException as exc:
        report["failure"] = str(exc)[:2048]
        raise
    finally:
        (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
