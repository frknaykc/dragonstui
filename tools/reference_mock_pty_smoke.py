#!/usr/bin/env python3
"""M66: one reference provider through the real controller and showcase PTY."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import pty
import re
import secrets
import signal
import subprocess
import sys
import tempfile
import termios
import time

import showcase_pty_smoke as h
from reference_mock_fixture import create_fixture


def assert_ansi_restored(output: bytearray) -> None:
    transitions = {mode: [] for mode in (1049, 25, 1000, 1002, 1003, 1006, 1015)}
    for match in re.finditer(rb"\x1b\[\?([0-9;]+)([hl])", output):
        for value in match[1].split(b";"):
            mode = int(value)
            if mode in transitions:
                transitions[mode].append(match[2])
    for mode, states in transitions.items():
        expected = b"h" if mode == 25 else b"l"
        h.require(b"h" in states and b"l" in states and states[-1] == expected,
                  f"terminal mode {mode} was not restored to its final state")


def session_terminal_evidence(text: str) -> dict | None:
    code = re.search(r"Interactive session exited with code (-?\d+)", text)
    if code is not None:
        h.require(code[1] == "2", f"unexpected explicit session exit code: {code[1]}")
        return {"kind": "exit_code", "exit_code": 2}
    if "Interactive session is no longer active" in text:
        # M65 explicitly permits authoritative inactivity after event-window
        # loss. It is release evidence, NOT evidence of a particular exit code.
        return {"kind": "authoritative_inactivity", "exit_code": None}
    return None


def stop_showcase(process: subprocess.Popen | None, master: int | None, output: bytearray) -> None:
    """Reap our own process group; keep draining its terminal during shutdown."""
    if process is None or process.poll() is not None:
        return
    for shutdown_signal in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(process.pid, shutdown_signal)
        except ProcessLookupError:
            pass
        deadline = time.monotonic() + 2.0
        while process.poll() is None and time.monotonic() < deadline:
            if master is not None:
                h.read_available(master, output, 0.03)
            else:
                try:
                    process.wait(timeout=0.03)
                except subprocess.TimeoutExpired:
                    pass
        if process.poll() is not None:
            return
    # Direct PID fallback handles group-level cleanup failure without dropping ownership.
    process.kill()
    process.wait(timeout=2.0)


def run(controller: Path, mock: Path, showcase: Path, exit_mode: str) -> dict:
    binaries = {name: path.resolve(strict=True) for name, path in
                (("controller", controller), ("mock", mock), ("showcase", showcase))}
    for name, path in binaries.items():
        h.require(path.is_file() and os.access(path, os.X_OK), f"{name} is not executable")
    identities = {name: hashlib.sha256(path.read_bytes()).hexdigest()
                  for name, path in binaries.items()}
    fixture = tempfile.TemporaryDirectory(prefix="dragonstui-m66-pty-")
    root = Path(fixture.name) / "adapters"
    daemon = process = None
    endpoint = None
    master = slave = None
    output = bytearray()
    control = root / ".reference-control"
    checks = []
    try:
        create_fixture(root, binaries["mock"], gated=True)
        daemon = subprocess.Popen(
            [str(binaries["controller"]), "--root", str(root), "controller-daemon"],
            env={**os.environ, "DRAGONSTUI_CONTROLLER_TOKEN": secrets.token_hex(32)},
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        endpoint, _ = h.wait_for_controller_endpoint(root, 4.0)
        master, slave = pty.openpty()
        original = termios.tcgetattr(slave)
        slave_name = os.ttyname(slave)
        h.set_size(slave, 160, 55)
        process = subprocess.Popen(
            [str(binaries["showcase"]), "--adapter-root", str(root)],
            stdin=slave, stdout=slave, stderr=slave,
            preexec_fn=h.establish_controlling_terminal, close_fds=True,
        )
        os.close(slave)
        slave = None
        os.set_blocking(master, False)

        def send(data):
            h.send(master, output, data)

        def current(text, timeout=3.0):
            h.wait_for_current_text(master, output, text, timeout, f"missing current frame: {text}")

        def terminal_operation(text, notification):
            # The bounded toast overlay covers Recent Operations. Observe the
            # outcome, then wait for its documented five-second lifetime before
            # requiring underlying current-frame text; don't accept old frames.
            current(notification)
            deadline = time.monotonic() + 6.0
            while h.current_screen_contains(output, "Notifications") and time.monotonic() < deadline:
                h.read_available(master, output, 0.05)
            h.require(not h.current_screen_contains(output, "Notifications"), "notification overlay did not expire")
            current(text)

        def sessions(expected):
            h.wait_for_session_marker_entries(control, expected, 2.0,
                f"provider registry differs from {expected}", master=master, output=output)

        def actions(expected):
            h.wait_for_action_invocations(control, expected, 2.0,
                "reference action dispatch mismatch", master=master, output=output)

        h.wait_for_text(master, output, "we are the recall", 4.0, "splash did not initialize")
        send(b"\r")
        current("Static benchmark context")
        send(b"8")
        current("Reference Mock")
        send(b"s")
        current('Completed: Started { id: AdapterId("reference") }')
        h.wait_for_property(master, output, "Live events received:", "8", 5.0, "first reference batch absent")
        (control / "observations.release").write_text("release\n")
        h.wait_for_property(master, output, "Live events received:", "16", 5.0, "second reference batch absent")
        h.wait_for_property(master, output, "Last live adapter:", "reference", 2.0, "wrong live provider")
        send(b"o")
        current("Observability · Logs")
        current("fixture log startup")
        for key, title, sample in (
            (b"2", "Metrics", "fixture.value"),
            (b"3", "Heatmap", "▓"),
            (b"4", "Status", "fixture-api"),
            (b"5", "Timeline", "fixture deployment"),
            (b"6", "Errors", "fixture.error"),
        ):
            send(key)
            current(f"Observability · {title}")
            current(sample)
            if title == "Heatmap":
                current("░")
                current("▒")
        send(b"\x1b[B")
        current("frame three")
        checks.append("all six observability views from reference provider")
        send(b"o")
        send(b"a")
        current("Adapter Actions")
        current("fixture.action.alpha")
        send(b"\r")
        expected = ["fixture.action.alpha"]
        actions(expected)
        terminal_operation("Alpha · succeeded", "Operation succeeded")
        send(b"\x1b[B\r")
        expected.append("fixture.destroy.everything")
        actions(expected)
        terminal_operation("Inspect · failed", "Operation failed")
        current("fixture_rejected")
        send(b"\x1b[B\r")
        current("Confirm Adapter Action")
        send(b"\x1b")
        current("Adapter Actions")
        actions(expected)
        send(b"\r\r")
        expected.append("fixture.inspect")
        actions(expected)
        terminal_operation("Confirm inspection · succeeded", "Operation succeeded")
        send(b"\x1b[B\r")
        expected.append("fixture.action.delta")
        actions(expected)
        current("Delta · running")
        checks.append("success, rejection, cancelled/confirmed action and held Running state")

        # Hold Delta while exercising sessions on this very same provider.
        send(b"a")
        send(b"h")
        h.wait_for_session_browser(master, output, 2.0, "reference session not discovered")
        send(b"\r")
        h.wait_for_session_host(master, output, 2.0, "reference session did not open while action held")
        sessions(["fixture-session"])
        send(b"alpha")
        current("echo:a")
        # The host action deadline is two seconds. Prove a session round-trip
        # while held, then release immediately; resize/close are independent
        # session checks, not a request to extend the host's timeout policy.
        (control / "actions.release").write_text("release\n")
        h.set_size(master, 100, 30)
        os.killpg(process.pid, signal.SIGWINCH)
        current("resized:22x97")
        send(b"\x1bx")
        h.wait_for_session_browser(master, output, 2.0, "explicit close did not return to browser")
        sessions([])
        h.set_size(master, 160, 55)
        os.killpg(process.pid, signal.SIGWINCH)
        send(b"h")
        send(b"a")
        terminal_operation("Delta · succeeded", "Operation succeeded")
        actions(expected)
        checks.append("same-provider input while action held, then release/success and resize/close")
        send(b"a")
        send(b"h")
        h.wait_for_session_browser(master, output, 2.0, "session browser not restored")
        send(b"\r")
        h.wait_for_session_host(master, output, 2.0, "nonzero session did not open")
        send(b"\x05")
        h.wait_for_session_browser(master, output, 2.0, "output burst/nonzero exit not observed")
        sessions([])
        send(b"h")
        deadline = time.monotonic() + 3.0
        terminal_evidence = None
        while time.monotonic() < deadline:
            terminal_evidence = session_terminal_evidence(h.visible_text(output))
            if terminal_evidence is not None:
                break
            h.read_available(master, output, 0.05)
        if terminal_evidence is None:
            terminal_evidence = session_terminal_evidence(h.fully_reconstructed_visible_text(output))
        h.require(terminal_evidence is not None, "session reported neither expected exit nor authoritative inactivity")
        send(b"h")
        h.wait_for_session_browser(master, output, 2.0, "browser not restored after exit-code check")
        send(b"\r")
        h.wait_for_session_host(master, output, 2.0, "outer-exit session did not open")
        sessions(["fixture-session"])
        if exit_mode in ("q", "ctrl-c"):
            send(b"q" if exit_mode == "q" else b"\x03")
        else:
            os.killpg(process.pid, signal.SIGTERM if exit_mode == "sigterm" else signal.SIGHUP)
        deadline = time.monotonic() + 8.0
        while process.poll() is None and time.monotonic() < deadline:
            h.read_available(master, output, 0.05)
        h.require(process.poll() == 0, f"showcase did not exit successfully: {process.poll()}")
        h.drain_for(master, output, 0.10)
        sessions([])  # Before stopping the daemon: exit must close the hosted session.
        restored = termios.tcgetattr(master)
        h.require(restored == original, "terminal termios state not fully restored")
        assert_ansi_restored(output)
        h.prove_pty_usable_after_exit(master, slave_name, output)
        checks.append("session terminal evidence and empty registry, active-session shutdown, full termios/ANSI restoration, usable PTY")
    except Exception:
        print("M66 PTY current screen:\n" + h.fully_reconstructed_visible_text(output), file=sys.stderr)
        for name in ("sessions", "actions"):
            marker = control / name
            if marker.is_file():
                print(f"{name}: {marker.read_text()!r}", file=sys.stderr)
        raise
    finally:
        failing = sys.exc_info()[0] is not None
        cleanup_errors = []
        try:
            stop_showcase(process, master, output)
        except Exception as error:
            cleanup_errors.append(error)
        for fd in (slave, master):
            if fd is not None:
                try:
                    os.close(fd)
                except OSError as error:
                    cleanup_errors.append(error)
        try:
            h.cleanup_m42_fixture(fixture, daemon, endpoint)
        except Exception as error:
            cleanup_errors.append(error)
        if cleanup_errors:
            print("M66 cleanup failed: " + ", ".join(type(error).__name__ for error in cleanup_errors), file=sys.stderr)
            if not failing:
                raise RuntimeError("M66 fixture cleanup failed") from cleanup_errors[0]
    checks.append("fixture process cleanup")
    h.require(identities == {name: hashlib.sha256(path.read_bytes()).hexdigest()
                            for name, path in binaries.items()}, "binaries changed during acceptance")
    return {"status": "passed", "exit": exit_mode, "binary_sha256": identities,
            "session_terminal_evidence": terminal_evidence, "checks": checks}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--controller", type=Path, required=True)
    parser.add_argument("--mock", type=Path, required=True)
    parser.add_argument("--showcase", type=Path, required=True)
    parser.add_argument("--exit", choices=("q", "ctrl-c", "sigterm", "sighup"), default="q")
    args = parser.parse_args()
    print(json.dumps(run(args.controller, args.mock, args.showcase, args.exit), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
