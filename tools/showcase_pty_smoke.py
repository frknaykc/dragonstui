#!/usr/bin/env python3
"""Exercise the release DragonsTUI showcase in a real POSIX pseudo-terminal."""

from __future__ import annotations

import argparse
import codecs
import fcntl
import json
import os
import pty
import re
import secrets
import select
import signal
import shlex
import socket
import struct
import subprocess
import sys
import termios
import tempfile
import time
from pathlib import Path

RESIZE_SEQUENCE = ((160, 55), (120, 40), (80, 24), (40, 15), (20, 8), (5, 3), (1, 1), (80, 24))
ANSI_SEQUENCE = re.compile(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|[@-_][0-?]*[ -/]*[@-~])")
SESSION_HOST_READY_MARKER = "(session output pending)"
SESSION_BROWSER_MARKER = "Interactive fixture"
ACTIVE_SESSION_TITLE_MARKER = "Interactive Session ·"


class PtyScreen:
    """Minimal ANSI screen reconstruction for this fixed-size smoke harness."""

    def __init__(self, width: int = 160, height: int = 55) -> None:
        self.width = width
        self.height = height
        self.cells = [[" "] * width for _ in range(height)]
        self.x = 0
        self.y = 0
        self.saved_cursor = (0, 0)
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.pending = bytearray()

    def feed(self, data: bytes) -> None:
        if self.pending:
            data = bytes(self.pending) + data
            self.pending.clear()
        index = 0
        while index < len(data):
            byte = data[index]
            if byte == 0x1B:
                if index + 1 >= len(data):
                    self.pending.extend(data[index:])
                    return
                if data[index + 1] == ord("["):
                    end = index + 2
                    while end < len(data) and not 0x40 <= data[end] <= 0x7E:
                        end += 1
                    if end >= len(data):
                        self.pending.extend(data[index:])
                        return
                    self._csi(data[index + 2 : end].decode("ascii", "ignore"), chr(data[end]))
                    index = end + 1
                    continue
                if data[index + 1] == ord("]"):
                    end = index + 2
                    while end < len(data) and data[end] != 0x07:
                        if data[end : end + 2] == b"\x1b\\":
                            end += 2
                            break
                        end += 1
                    else:
                        self.pending.extend(data[index:])
                        return
                    if end < len(data) and data[end] == 0x07:
                        end += 1
                    index = end
                    continue
                index += 2
                continue
            if byte == 0x0D:
                self.x = 0
            elif byte == 0x0A:
                self.y = min(self.height - 1, self.y + 1)
            elif byte == 0x08:
                self.x = max(0, self.x - 1)
            elif byte >= 0x20:
                for character in self.decoder.decode(bytes((byte,)), final=False):
                    self._write(character)
            index += 1

    def _csi(self, parameters: str, command: str) -> None:
        private = parameters.startswith("?")
        values = parameters.lstrip("?").split(";") if parameters else []
        numbers = [int(value) if value else 0 for value in values]
        if command in {"H", "f"}:
            row = numbers[0] if numbers else 1
            column = numbers[1] if len(numbers) > 1 else 1
            self.y = min(self.height - 1, max(0, row - 1))
            self.x = min(self.width - 1, max(0, column - 1))
        elif command == "A":
            self.y = max(0, self.y - (numbers[0] if numbers else 1))
        elif command == "B":
            self.y = min(self.height - 1, self.y + (numbers[0] if numbers else 1))
        elif command == "C":
            self.x = min(self.width - 1, self.x + (numbers[0] if numbers else 1))
        elif command == "D":
            self.x = max(0, self.x - (numbers[0] if numbers else 1))
        elif command == "G":
            self.x = min(self.width - 1, max(0, (numbers[0] if numbers else 1) - 1))
        elif command == "d":
            self.y = min(self.height - 1, max(0, (numbers[0] if numbers else 1) - 1))
        elif command == "J" and not private:
            if not numbers or numbers[0] in {0, 2, 3}:
                self.cells = [[" "] * self.width for _ in range(self.height)]
        elif command == "K" and not private:
            self.cells[self.y][self.x :] = [" "] * (self.width - self.x)
        elif command == "s":
            self.saved_cursor = (self.x, self.y)
        elif command == "u":
            self.x, self.y = self.saved_cursor

    def _write(self, character: str) -> None:
        if self.x >= self.width:
            self.x = 0
            self.y = min(self.height - 1, self.y + 1)
        self.cells[self.y][self.x] = character
        self.x += 1

    def text(self) -> str:
        return "\n".join("".join(row).rstrip() for row in self.cells)


def rendered_text(output: bytearray) -> str:
    return ANSI_SEQUENCE.sub("", output.decode("utf-8", errors="replace"))


SCREEN_CACHE: dict[int, tuple[PtyScreen, int]] = {}


def visible_text(output: bytearray) -> str:
    key = id(output)
    screen, parsed = SCREEN_CACHE.get(key, (PtyScreen(), 0))
    if len(output) < parsed:
        screen, parsed = PtyScreen(), 0
    if len(output) > parsed:
        screen.feed(bytes(output[parsed:]))
        SCREEN_CACHE[key] = (screen, len(output))
    return screen.text()


def current_screen_contains(output: bytearray, needle: str) -> bool:
    return needle in visible_text(output)


def fully_reconstructed_visible_text(output: bytearray) -> str:
    screen = PtyScreen()
    screen.feed(bytes(output))
    return screen.text()


def set_size(fd: int, width: int, height: int) -> None:
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))


def establish_controlling_terminal() -> None:
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


def read_available(fd: int, output: bytearray, timeout: float) -> None:
    ready, _, _ = select.select([fd], [], [], timeout)
    if not ready:
        return
    try:
        while True:
            chunk = os.read(fd, 65536)
            if not chunk:
                return
            output.extend(chunk)
            if len(chunk) < 65536:
                return
    except (BlockingIOError, OSError):
        return


def drain_for(fd: int, output: bytearray, seconds: float) -> None:
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return
        read_available(fd, output, min(0.03, remaining))


def send(fd: int, output: bytearray, data: bytes, pause: float = 0.08) -> None:
    os.write(fd, data)
    drain_for(fd, output, pause)


def wait_for_text(fd: int, output: bytearray, needle: str, timeout: float, message: str) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if current_screen_contains(output, needle) or needle in rendered_text(output):
            return
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        read_available(fd, output, min(0.05, remaining))
    visible = fully_reconstructed_visible_text(output)
    if needle in visible:
        return
    raise RuntimeError(f"{message}; rendered screen tail: {visible[-1200:]!r}")


def wait_for_current_text(
    fd: int, output: bytearray, needle: str, timeout: float, message: str
) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if current_screen_contains(output, needle):
            return
        read_available(fd, output, min(0.05, deadline - time.monotonic()))
    visible = fully_reconstructed_visible_text(output)
    if needle in visible:
        return
    raise RuntimeError(f"{message}; rendered screen tail: {visible[-1200:]!r}")


def session_host_is_ready(text: str) -> bool:
    return SESSION_HOST_READY_MARKER in text


def wait_for_session_host(fd: int, output: bytearray, timeout: float, message: str) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if session_host_is_ready(visible_text(output)):
            return
        read_available(fd, output, min(0.05, deadline - time.monotonic()))
    visible = fully_reconstructed_visible_text(output)
    if session_host_is_ready(visible):
        return
    raise RuntimeError(f"{message}; rendered screen tail: {visible[-1200:]!r}")


def session_browser_is_ready(text: str) -> bool:
    return (
        "Interactive Sessions" in text
        and SESSION_BROWSER_MARKER in text
        and ACTIVE_SESSION_TITLE_MARKER not in text
    )


def wait_for_session_browser(fd: int, output: bytearray, timeout: float, message: str) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if session_browser_is_ready(visible_text(output)):
            return
        read_available(fd, output, min(0.05, deadline - time.monotonic()))
    visible = fully_reconstructed_visible_text(output)
    if session_browser_is_ready(visible):
        return
    raise RuntimeError(f"{message}; rendered screen tail: {visible[-1200:]!r}")


def property_has_value(line: str, label: str, value: str) -> bool:
    if label not in line:
        return False
    actual = line.split(label, 1)[1].lstrip()
    if not actual.startswith(value):
        return False
    suffix = actual[len(value) :]
    return not suffix or not suffix[0].isalnum()


def wait_for_property(
    fd: int, output: bytearray, label: str, value: str, timeout: float, message: str
) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        visible = visible_text(output)
        if any(property_has_value(line, label, value) for line in visible.splitlines()):
            return
        read_available(fd, output, min(0.05, deadline - time.monotonic()))
    visible = fully_reconstructed_visible_text(output)
    if any(property_has_value(line, label, value) for line in visible.splitlines()):
        return
    raise RuntimeError(f"{message}; rendered screen tail: {visible[-1200:]!r}")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def make_executable(path: Path) -> None:
    path.chmod(path.stat().st_mode | 0o111)


def write_mock_adapter(
    root: Path,
    mock_binary: Path,
    adapter_id: str,
    mode: str,
    extra_args: tuple[str, ...] = (),
) -> None:
    executable = root / adapter_id / "bin" / adapter_id
    executable.parent.mkdir(parents=True)
    arguments = ("--mode", mode, "--id", adapter_id, *extra_args)
    executable.write_text(
        "#!/bin/sh\n"
        f"exec {shlex.quote(str(mock_binary))} {' '.join(shlex.quote(value) for value in arguments)}\n",
        encoding="utf-8",
    )
    make_executable(executable)
    (root / adapter_id / "adapter.json").write_text(
        json.dumps(
            {
                "id": adapter_id,
                "name": f"PTY {adapter_id}",
                "version": "1.0.0",
                "protocol_version": 1,
                "executable": f"bin/{adapter_id}",
            }
        ),
        encoding="utf-8",
    )


def wait_for_controller_endpoint(root: Path, timeout: float) -> tuple[dict[str, str], Path]:
    endpoint_path = root / ".controller" / "endpoint.json"
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            endpoint = json.loads(endpoint_path.read_text(encoding="utf-8"))
        except (FileNotFoundError, json.JSONDecodeError):
            time.sleep(0.02)
            continue
        address = endpoint.get("address")
        token = endpoint.get("token")
        if isinstance(address, str) and isinstance(token, str):
            return {"address": address, "token": token}, endpoint_path
        time.sleep(0.02)
    raise RuntimeError("controller daemon did not publish an endpoint")


def _wait_for_marker_progress(
    master: int | None, output: bytearray | None, deadline: float
) -> None:
    timeout = min(0.01, max(0.0, deadline - time.monotonic()))
    if master is not None and output is not None:
        # A full PTY can stall the UI before it processes the request/cleanup
        # that publishes the marker. Preserve these bytes for screen assertions.
        read_available(master, output, timeout)
    else:
        time.sleep(timeout)


def wait_for_file(
    path: Path, timeout: float, message: str,
    *, master: int | None = None, output: bytearray | None = None,
) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.is_file():
            return
        _wait_for_marker_progress(master, output, deadline)
    raise RuntimeError(message)


def reset_hold_markers(control: Path) -> None:
    for name in ("ready", "release", "launches"):
        (control / name).unlink(missing_ok=True)


def assert_single_hold_launch(control: Path) -> None:
    launches = (control / "launches").read_text(encoding="utf-8").splitlines()
    require(launches == ["adapter-a"], f"held action launched {launches!r}, expected one adapter-a process")


def action_invocations(control: Path) -> list[str]:
    marker = control / "actions"
    if not marker.is_file():
        return []
    return marker.read_text(encoding="utf-8").splitlines()


def wait_for_action_invocations(
    control: Path, expected: list[str], timeout: float, message: str,
    *, master: int | None = None, output: bytearray | None = None,
) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if action_invocations(control) == expected:
            return
        _wait_for_marker_progress(master, output, deadline)
    raise RuntimeError(f"{message}; got {action_invocations(control)!r}, expected {expected!r}")


def session_marker(control: Path) -> Path:
    return control / "sessions"


def session_marker_entries(control: Path) -> list[str]:
    marker = session_marker(control)
    if not marker.is_file():
        return []
    return [entry for entry in marker.read_text(encoding="utf-8").splitlines() if entry]


def reset_session_markers(control: Path) -> None:
    marker = session_marker(control)
    for path in (marker, Path(f"{marker}.ready"), Path(f"{marker}.release")):
        path.unlink(missing_ok=True)


def wait_for_session_marker_entries(
    control: Path, expected: list[str], timeout: float, message: str,
    *, master: int | None = None, output: bytearray | None = None,
) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if session_marker_entries(control) == expected:
            return
        _wait_for_marker_progress(master, output, deadline)
    raise RuntimeError(
        f"{message}; got {session_marker_entries(control)!r}, expected {expected!r}"
    )


def release_delayed_session(control: Path) -> None:
    Path(f"{session_marker(control)}.release").write_text("release\n", encoding="utf-8")


def prove_pty_usable_after_exit(master: int, slave_name: str, output: bytearray) -> None:
    slave = os.open(slave_name, os.O_RDWR | os.O_NOCTTY)
    shell = subprocess.Popen(
        ["/bin/sh", "-c", "printf __PTY_READY__; IFS= read -r line; printf __PTY_ECHO__%s \"$line\""],
        stdin=slave,
        stdout=slave,
        stderr=slave,
        preexec_fn=establish_controlling_terminal,
        close_fds=True,
    )
    os.close(slave)
    try:
        wait_for_text(master, output, "__PTY_READY__", 1.0, "PTY did not accept a post-exit shell")
        send(master, output, b"restored-input\n")
        wait_for_text(master, output, "__PTY_ECHO__restored-input", 1.0, "PTY did not restore canonical input/output")
        shell.wait(timeout=1)
        require(shell.returncode == 0, f"post-exit PTY shell exited with {shell.returncode}")
    finally:
        if shell.poll() is None:
            shell.kill()
            shell.wait(timeout=1)


def shutdown_controller(endpoint: dict[str, str]) -> None:
    address = endpoint["address"]
    token = endpoint["token"]
    require(address.startswith("127.0.0.1:"), "controller endpoint was not loopback")
    _, port = address.rsplit(":", 1)
    with socket.create_connection(("127.0.0.1", int(port)), timeout=2) as stream:
        stream.sendall(json.dumps({"token": token, "command": "Shutdown"}).encode("utf-8") + b"\n")
        response = stream.recv(4096)
    require(b"Completed" in response, "controller daemon did not acknowledge shutdown")


def assert_no_fixture_processes(root: Path) -> None:
    processes = subprocess.check_output(["ps", "-axo", "pid=,command="], text=True)
    leaked = [line.strip() for line in processes.splitlines() if str(root) in line]
    require(not leaked, f"fixture processes leaked: {leaked!r}")


def setup_m42_fixture(controller_binary: Path, mock_binary: Path) -> tuple[tempfile.TemporaryDirectory[str], subprocess.Popen[bytes], dict[str, str], Path]:
    require(controller_binary.is_file() and os.access(controller_binary, os.X_OK), "controller binary is not executable")
    require(mock_binary.is_file() and os.access(mock_binary, os.X_OK), "mock binary is not executable")
    fixture = tempfile.TemporaryDirectory(prefix="dragonstui-m42-pty-")
    root = Path(fixture.name)
    control = root / ".m42-pty"
    control.mkdir()
    write_mock_adapter(
        root,
        mock_binary,
        "adapter-a",
        "hold",
        (
            "--hold-ready",
            str(control / "ready"),
            "--hold-release",
            str(control / "release"),
            "--launch-marker",
            str(control / "launches"),
        ),
    )
    write_mock_adapter(root, mock_binary, "adapter-b", "timeout")
    write_mock_adapter(root, mock_binary, "capability-a", "shared-capabilities")
    write_mock_adapter(root, mock_binary, "capability-b", "shared-capabilities")
    write_mock_adapter(root, mock_binary, "live-a", "observability-events",
                       ("--event-release", str(control / "observations.release")))
    write_mock_adapter(root, mock_binary, "stress-a", "stress-events")
    write_mock_adapter(root, mock_binary, "stress-b", "stress-events")
    write_mock_adapter(
        root,
        mock_binary,
        "z-actions",
        "actions",
        ("--action-marker", str(control / "actions")),
    )
    write_mock_adapter(
        root,
        mock_binary,
        "z-sessions",
        "delayed-sessions",
        ("--session-marker", str(control / "sessions")),
    )
    daemon = subprocess.Popen(
        [str(controller_binary), "--root", str(root), "controller-daemon"],
        env={**os.environ, "DRAGONSTUI_CONTROLLER_TOKEN": secrets.token_hex(32)},
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    try:
        endpoint, _endpoint_path = wait_for_controller_endpoint(root, 4.0)
        return fixture, daemon, endpoint, root
    except Exception:
        daemon.kill()
        daemon.wait(timeout=2)
        fixture.cleanup()
        raise


def cleanup_m42_fixture(
    fixture: tempfile.TemporaryDirectory[str] | None,
    daemon: subprocess.Popen[bytes] | None,
    endpoint: dict[str, str] | None,
) -> None:
    if daemon is not None and daemon.poll() is None and endpoint is not None:
        try:
            shutdown_controller(endpoint)
        except (OSError, RuntimeError):
            pass
    if daemon is not None and daemon.poll() is None:
        try:
            daemon.wait(timeout=2)
        except subprocess.TimeoutExpired:
            os.killpg(daemon.pid, signal.SIGKILL)
            daemon.wait(timeout=2)
    if fixture is not None:
        root = Path(fixture.name)
        fixture.cleanup()
        assert_no_fixture_processes(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--exit", choices=("q", "ctrl-c", "sigterm", "sighup"), default="q")
    parser.add_argument("--skip-splash", action="store_true")
    parser.add_argument("--adapter-id", help="expect this discovered adapter in the M41 inspector")
    parser.add_argument("--m42-controller", type=Path, help="real dragonstui-adapter binary for M42 PTY acceptance")
    parser.add_argument("--m42-mock", type=Path, help="real adapter-host mock binary for M42 PTY acceptance")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("provide a showcase binary after --")
    require((args.m42_controller is None) == (args.m42_mock is None), "M42 controller and mock binaries must be supplied together")

    fixture: tempfile.TemporaryDirectory[str] | None = None
    daemon: subprocess.Popen[bytes] | None = None
    endpoint: dict[str, str] | None = None
    m42_root: Path | None = None
    m42_control: Path | None = None
    if args.m42_controller is not None:
        fixture, daemon, endpoint, m42_root = setup_m42_fixture(args.m42_controller, args.m42_mock)
        command = [*command, "--adapter-root", str(m42_root)]
        args.adapter_id = "adapter-a"
    master, slave = pty.openpty()
    slave_name = os.ttyname(slave)
    original_mode = termios.tcgetattr(slave)
    set_size(slave, *RESIZE_SEQUENCE[0])
    process = subprocess.Popen(
        command,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        preexec_fn=establish_controlling_terminal,
        close_fds=True,
    )
    os.close(slave)
    os.set_blocking(master, False)
    output = bytearray()

    try:
        splash_start = len(output)
        drain_for(master, output, 0.70)  # Observe several slow Dragonfire gradient/loading ticks.
        if args.skip_splash:
            send(master, output, b"\r")
        else:
            drain_for(master, output, 6.25)  # Allow the 6s boundary plus the 50ms render tick.
            require(
                "Static benchmark context" in output[splash_start:].decode("utf-8", errors="replace"),
                "showcase did not transition after the six-second default splash",
            )
        send(master, output, b"2")  # Make the first Overview click a state transition.

        if m42_root is not None:
            control = m42_root / ".m42-pty"
            m42_control = control
            before = len(output)
            send(master, output, b"8")
            adapter_render = output[before:].decode("utf-8", errors="replace")
            require(
                "Adapter Inspector" in adapter_render and "adapter-a" in adapter_render,
                "keyboard navigation did not open the adapter inspector",
            )
            wait_for_text(master, output, "State:", 2.0, "initial diagnostics did not render stopped state")

            reset_hold_markers(control)
            send(master, output, b"s")
            wait_for_file(control / "ready", 1.0, "held adapter start did not publish readiness", master=master, output=output)
            send(master, output, b"s")
            wait_for_text(
                master,
                output,
                "another adapter management operation for adapter-a is already in progress",
                1.0,
                "same-adapter conflict was not presented by the TUI",
            )
            assert_single_hold_launch(control)
            (control / "release").write_text("release\n", encoding="utf-8")
            wait_for_text(master, output, "Completed: Started", 2.0, "held Start did not complete after release")
            wait_for_text(master, output, "running", 2.0, "authoritative running diagnostics did not render")
            wait_for_text(master, output, "test.echo", 2.0, "running diagnostics did not render capabilities")
            wait_for_text(master, output, "PID:", 2.0, "running diagnostics did not render a PID")

            send(master, output, b"t")
            wait_for_text(master, output, "Completed: Stopped", 2.0, "guard did not release adapter-a after held Start")
            wait_for_text(master, output, "stopped", 2.0, "authoritative stopped diagnostics did not render")

            reset_hold_markers(control)
            send(master, output, b"r")
            wait_for_file(control / "ready", 1.0, "held Restart did not publish readiness", master=master, output=output)
            (control / "release").write_text("release\n", encoding="utf-8")
            wait_for_text(master, output, "Completed: Restarted", 2.0, "Restart did not complete through the TUI")
            wait_for_text(master, output, "running", 2.0, "authoritative restarted diagnostics did not render")

            send(master, output, b"\t" * 4)
            send(master, output, b"\x1b[B")
            wait_for_property(master, output, "Adapter ID:", "adapter-b", 1.0, "keyboard table navigation did not select adapter-b")
            send(master, output, b"s")
            wait_for_text(master, output, "Failed:", 3.0, "controlled lifecycle failure was not presented by the TUI")
            wait_for_text(master, output, "failed", 6.0, "failure diagnostics did not render")
            wait_for_text(master, output, "adapter handshake timed out", 6.0, "failure diagnostics did not render the daemon error")
            send(master, output, b"\x1b[A")
            send(master, output, b"t")
            wait_for_text(master, output, "Completed: Stopped", 2.0, "TUI did not remain interactive after a management failure")

            # M43: start two independent adapters whose live handshake snapshots
            # advertise the same opaque contract, then browse it through real keys.
            send(master, output, b"\x1b[B\x1b[B")
            send(master, output, b"s")
            wait_for_text(master, output, "running", 2.0, "first capability provider did not start")
            send(master, output, b"\x1b[B")
            send(master, output, b"s")
            wait_for_text(master, output, "cap.before, cap.shared", 2.0, "second capability provider did not report live capabilities")

            send(master, output, b"c")
            wait_for_text(master, output, "Capabilities", 1.0, "capability browser did not open")
            wait_for_text(master, output, "cap.shared", 1.0, "capability browser did not render the shared contract")
            send(master, output, b"\x1b[B")
            wait_for_text(master, output, "Capability: cap.shared", 1.0, "capability selection did not update provider detail")
            visible = fully_reconstructed_visible_text(output)
            require("capability-a · PTY capability-a · running" in visible, "first shared capability provider was not visible")
            require("capability-b · PTY capability-b · running" in visible, "second shared capability provider was not visible")
            send(master, output, b"\x1b")
            visible = fully_reconstructed_visible_text(output)
            require("Adapter Inspector" in visible, "capability browser did not return to the adapter inspector")

            # Release batch two only after batch one reaches the UI. A fixed delay
            # can coalesce both batches when controller polling is delayed, losing
            # events through the intentionally bounded sync_channel(8).
            # The showcase worker drains authenticated controller data away from
            # the UI loop and the tick applies it to render-owned state.
            send(master, output, b"\x1b[B" * 4)
            send(master, output, b"s")
            wait_for_text(master, output, "running", 2.0, "live adapter did not start")
            wait_for_property(master, output, "Live events received:", "8", 5.0, "first observability batch did not reach showcase state")
            (control / "observations.release").write_text("release\n", encoding="utf-8")
            wait_for_property(master, output, "Live events received:", "16", 5.0, "observability fixture did not reach showcase state")
            wait_for_property(master, output, "Last live adapter:", "live-a", 1.0, "live adapter identity was not rendered")
            wait_for_property(master, output, "Last live stream:", "observations", 1.0, "live stream identity was not rendered")
            wait_for_property(master, output, "Last live kind:", "fixture / metric", 1.0, "semantic event class was not rendered")
            wait_for_property(master, output, "Last live payload:", "{\"sequence\":1}", 1.0, "opaque live payload was not rendered")
            send(master, output, b"1")
            wait_for_text(master, output, "Static benchmark context", 1.0, "input stalled while live worker was active")
            send(master, output, b"8")
            wait_for_text(master, output, "Adapter Inspector", 1.0, "adapter inspector did not remain responsive")

            # M52: inspect the real opaque payload retained through the live-data
            # path, expand its root, then return without disturbing Adapter mode.
            send(master, output, b"j")
            wait_for_text(master, output, "Structured Payload", 1.0, "structured payload browser did not open")
            send(master, output, b"\x1b[C")
            wait_for_text(master, output, "sequence: 1", 1.0, "structured payload root did not expand")
            send(master, output, b"\x1b")
            wait_for_text(master, output, "Adapter Inspector", 1.0, "structured payload browser did not return")

            # M53–M58: the fixture carries multiple logs/metrics/statuses/events
            # and two deterministic error groups through the real controller path.
            send(master, output, b"o")
            wait_for_text(master, output, "Observability · Logs", 1.0, "log viewer did not open")
            wait_for_text(master, output, "fixture log startup", 1.0, "first semantic Log was not visible")
            wait_for_text(master, output, "fixture log warning", 1.0, "second semantic Log was not visible")
            send(master, output, b"p")
            wait_for_text(master, output, "PAUSED", 1.0, "log viewer did not pause semantic follow")
            send(master, output, b"f")
            wait_for_text(master, output, "FOLLOW", 1.0, "log viewer did not resume semantic follow")
            send(master, output, b"2")
            wait_for_text(master, output, "Observability · Metrics", 1.0, "metric graph did not open")
            wait_for_text(master, output, "fixture.value", 1.0, "semantic Metric series was not visible")
            send(master, output, b"3")
            wait_for_text(master, output, "Observability · Heatmap", 1.0, "heatmap did not open")
            send(master, output, b"4")
            wait_for_text(master, output, "Observability · Status", 1.0, "status matrix did not open")
            wait_for_text(master, output, "fixture-api", 1.0, "first semantic Status row was not visible")
            wait_for_text(master, output, "fixture-worker", 1.0, "second semantic Status row was not visible")
            wait_for_text(master, output, "fixture-db", 1.0, "third semantic Status row was not visible")
            send(master, output, b"5")
            wait_for_text(master, output, "Observability · Timeline", 1.0, "timeline did not open")
            wait_for_text(master, output, "fixture deployment", 1.0, "first semantic Event was not visible")
            wait_for_text(master, output, "fixture follow-up", 1.0, "second semantic Event was not visible")
            send(master, output, b"\x1b[B")
            wait_for_text(master, output, "second timeline detail", 1.0, "timeline selection did not update detail")
            send(master, output, b"6")
            wait_for_text(master, output, "Observability · Errors", 1.0, "error viewer did not open")
            wait_for_text(master, output, "fixture.error", 1.0, "semantic Error group was not visible")
            wait_for_text(master, output, "fixture distinct error", 1.0, "second semantic Error group was not visible")
            send(master, output, b"\x1b[B")
            wait_for_text(master, output, "frame three", 1.0, "semantic Error stack was not visible")
            send(master, output, b"1")
            wait_for_text(master, output, "fixture log", 1.0, "log viewer did not survive cross-view navigation")
            send(master, output, b"o")
            wait_for_text(master, output, "Adapter Inspector", 1.0, "log viewer did not return to adapters")

            # M45–M47: the observability fixture already fills retained history.
            # Each stress stream adds one bounded handoff batch, proving eviction
            # while pause affects only the selection.
            send(master, output, b"\x1b[B")
            send(master, output, b"s")
            wait_for_property(master, output, "Live events received:", "24", 3.0, "first stress burst did not continue bounded ingestion")
            wait_for_property(master, output, "Retained live history:", "16/16", 1.0, "first stress burst did not preserve history capacity")
            send(master, output, b"p")
            wait_for_text(master, output, "PAUSED", 1.0, "P did not pause generic live follow state")
            wait_for_property(master, output, "Visible selection:", "stress-a", 1.0, "pause did not preserve the first stress selection")

            send(master, output, b"\x1b[B")
            send(master, output, b"s")
            wait_for_property(master, output, "Live events received:", "32", 3.0, "second stress burst did not continue ingestion while paused")
            wait_for_property(master, output, "Retained live history:", "16/16", 1.0, "retained history did not evict at capacity")
            wait_for_property(master, output, "Visible selection:", "stress-a", 1.0, "paused view jumped to the new live tail")

            send(master, output, b"/stress-b\r")
            wait_for_text(master, output, "stress-b", 1.0, "slash search did not apply the generic query")
            send(master, output, b"f")
            wait_for_text(master, output, "FOLLOW", 1.0, "F did not resume generic live follow state")

            # Phase 7: action behavior comes only from the adapter's declared
            # metadata. The marker is producer-side evidence: cancelling the
            # confirmation must leave it untouched; confirming adds exactly one
            # invocation. The delayed action exposes the running state before its
            # typed terminal result without blocking the input/render loop.
            send(master, output, b"\x1b[B")
            wait_for_property(master, output, "Adapter ID:", "z-actions", 1.0, "action adapter was not selected")
            send(master, output, b"s")
            wait_for_current_text(master, output, 'Completed: Started { id: AdapterId("z-actions") }', 2.0, "action adapter did not start")
            send(master, output, b"a")
            wait_for_text(master, output, "Adapter Actions", 1.0, "declared action browser did not open")
            wait_for_text(master, output, "fixture.action.alpha", 1.0, "declared action metadata was not visible")

            send(master, output, b"\r")
            wait_for_action_invocations(control, ["fixture.action.alpha"], 2.0, "direct action did not invoke once", master=master, output=output)
            wait_for_text(master, output, "Operation succeeded", 2.0, "successful operation notification was not visible")

            send(master, output, b"\x1b[B\r")
            wait_for_action_invocations(
                control,
                ["fixture.action.alpha", "fixture.destroy.everything"],
                2.0,
                "declared failure action did not invoke once",
                master=master, output=output,
            )
            wait_for_text(master, output, "Operation failed", 2.0, "failed operation notification was not visible")

            send(master, output, b"\x1b[B\r")
            wait_for_text(master, output, "Confirm Adapter Action", 1.0, "declared confirmation policy did not open a modal")
            send(master, output, b"\x1b")
            drain_for(master, output, 0.20)
            require(
                action_invocations(control) == ["fixture.action.alpha", "fixture.destroy.everything"],
                "cancelled confirmation invoked an adapter action",
            )
            send(master, output, b"\r\r")
            wait_for_action_invocations(
                control,
                ["fixture.action.alpha", "fixture.destroy.everything", "fixture.inspect"],
                2.0,
                "confirmed action did not invoke exactly once",
                master=master, output=output,
            )
            wait_for_text(master, output, "Operation succeeded", 2.0, "confirmed operation did not succeed")

            send(master, output, b"\x1b[B\r")
            wait_for_action_invocations(
                control,
                [
                    "fixture.action.alpha",
                    "fixture.destroy.everything",
                    "fixture.inspect",
                    "fixture.action.delta",
                ],
                2.0,
                "delayed action did not invoke once",
                master=master, output=output,
            )
            wait_for_text(master, output, "running", 1.0, "delayed operation did not expose running state")
            wait_for_text(master, output, "succeeded", 2.0, "delayed operation did not reach typed success")
            send(master, output, b"a")
            wait_for_text(master, output, "Adapter Inspector", 1.0, "action browser did not return to adapters")
            send(master, output, b"o")
            wait_for_text(master, output, "Observability · Logs", 1.0, "action surface did not preserve observability navigation")
            send(master, output, b"o")
            wait_for_text(master, output, "Adapter Inspector", 1.0, "observability did not return after action acceptance")

            # M65: discover a provider-declared interactive surface. The delayed
            # provider fixture writes its active-session registry before it emits
            # SessionOpened, then waits for an explicit release marker.
            send(master, output, b"\x1b[B" * 5)
            send(master, output, b"s")
            wait_for_current_text(
                master,
                output,
                'Completed: Started { id: AdapterId("z-sessions") }',
                2.0,
                "session adapter did not start",
            )
            send(master, output, b"h")
            wait_for_session_browser(master, output, 2.0, "declared session browser did not open")
            reset_session_markers(control)
            send(master, output, b"\r")
            wait_for_file(
                Path(f"{session_marker(control)}.ready"),
                1.0,
                "delayed session open did not publish provider readiness",
                master=master, output=output,
            )
            wait_for_session_marker_entries(
                control,
                ["fixture-session"],
                1.0,
                "provider did not retain the opened interactive session",
                master=master, output=output,
            )
            release_delayed_session(control)
            wait_for_session_host(master, output, 1.0, "interactive host did not open")
            send(master, output, b"alpha")
            wait_for_current_text(master, output, "echo:a", 2.0, "typed session input did not reach the provider")
            set_size(master, 100, 30)
            os.killpg(process.pid, signal.SIGWINCH)
            wait_for_current_text(master, output, "resized:22x97", 2.0, "rendered session dimensions did not reach the provider")
            send(master, output, b"\x05")
            wait_for_session_browser(master, output, 2.0, "non-zero terminal exit did not return to the session browser")
            wait_for_session_marker_entries(
                control,
                [],
                2.0,
                "provider retained the non-zero-exit session after output pressure",
                master=master, output=output,
            )

            reset_session_markers(control)
            send(master, output, b"\r")
            wait_for_file(
                Path(f"{session_marker(control)}.ready"),
                1.0,
                "explicit-close session did not publish provider readiness",
                master=master, output=output,
            )
            release_delayed_session(control)
            wait_for_session_host(master, output, 1.0, "explicit-close interactive host did not open")
            send(master, output, b"\x1bx")
            wait_for_session_browser(master, output, 2.0, "typed session close did not return to the session browser")
            wait_for_session_marker_entries(
                control,
                [],
                2.0,
                "provider retained the explicitly closed session",
                master=master, output=output,
            )
            send(master, output, b"h")
            # A terminal-operation notification may cover the right-side
            # inspector title, but cannot cover the adapter table itself.
            wait_for_current_text(master, output, "PTY adapter-a", 1.0, "session browser did not return to adapters")

            # A now has a provider-owned session but no UI claimant: Escape
            # abandons the pending open, release emits SessionOpened, then the
            # controller must close it. The marker is the mock's active-session
            # registry, not a request log, so [] proves resource release.
            # Leaving the session browser deliberately resets the shared table
            # selection to row zero. Select the sorted z-sessions fixture
            # explicitly; a generic empty browser has the same heading but
            # cannot prove an open reached the provider.
            send(master, output, b"\x1b[B" * 8)
            reset_session_markers(control)
            send(master, output, b"h")
            wait_for_session_browser(master, output, 2.0, "stale-open session browser did not open")
            send(master, output, b"\r")
            wait_for_file(
                Path(f"{session_marker(control)}.ready"),
                1.0,
                "stale delayed session did not publish provider readiness",
                master=master, output=output,
            )
            wait_for_session_marker_entries(
                control,
                ["fixture-session"],
                1.0,
                "stale delayed session was not provider-owned before release",
                master=master, output=output,
            )
            send(master, output, b"\x1b")
            wait_for_current_text(master, output, "PTY adapter-a", 1.0, "Escape did not abandon the pending session browser")
            release_delayed_session(control)
            wait_for_session_marker_entries(
                control,
                [],
                2.0,
                "stale SessionOpened did not close the provider-owned session",
                master=master, output=output,
            )
            drain_for(master, output, 0.10)
            require(
                not session_host_is_ready(visible_text(output)),
                "stale SessionOpened replaced the active session browser state",
            )
            set_size(master, *RESIZE_SEQUENCE[0])
            os.killpg(process.pid, signal.SIGWINCH)
            drain_for(master, output, 0.10)

            # The action assertions above already observed each terminal outcome.
            # Let the bounded five-second informational overlays expire before
            # asserting underlying adapter-inspector text through later header
            # navigation; an overlay may legitimately obscure that text without
            # changing the selected section or hit region.
            drain_for(master, output, 5.2)

        # Explicit visible header tabs: 1 Overview through 8 Adapters.
        for x, marker in (
            (14, "Primitive index"),
            (24, "Data contract"),
            (31, "Canvas · Braille waveform"),
            (42, "TextInput · grapheme aware"),
            (50, "Focus · KeyMap · Style"),
            (64, "Language"),
        ):
            send(master, output, f"\x1b[<0;{x};3M".encode("ascii"))
            require(marker in visible_text(output), f"header click missed {marker}")

        # Adapter management remains an optional application integration: the
        # showcase must expose its empty host root honestly via every entry path.
        if m42_root is None:
            before = len(output)
            send(master, output, b"8")
            adapter_render = output[before:].decode("utf-8", errors="replace")
            if args.adapter_id:
                require("Adapter Inspector" in adapter_render and args.adapter_id in adapter_render, "keyboard navigation did not open the adapter inspector")
            else:
                require("No installed adapters" in adapter_render, "keyboard navigation did not open the empty Adapters section")
        send(master, output, b"1")
        send(master, output, b"\x1b[<0;75;3M")
        adapter_render = visible_text(output)
        if args.adapter_id:
            require("Adapter Inspector" in adapter_render and args.adapter_id in adapter_render, "header click did not open the adapter inspector")
        else:
            require("No installed adapters" in adapter_render, "header click did not open the Adapters section")
        send(master, output, b"1")
        before = len(output)
        send(master, output, b"\x10")
        send(master, output, b"Open Adapters\r")
        adapter_render = output[before:].decode("utf-8", errors="replace")
        if args.adapter_id:
            require("Adapter Inspector" in adapter_render and args.adapter_id in adapter_render, "command palette did not open the adapter inspector")
        else:
            require("No installed adapters" in adapter_render, "command palette did not open the Adapters section")

        # Keyboard navigation remains available alongside the header hit regions.
        send(master, output, b"1")
        send(master, output, b"2")
        send(master, output, b"\t\x1b[Z\x1b[B\x1b[C")  # Tab, Shift+Tab, list/tree routes.
        send(master, output, b"\x1b[<0;12;9M")  # Mouse click.
        send(master, output, b"\x1b[<65;12;9M")  # Mouse wheel up.
        send(master, output, b"\x1b[<64;12;9M")  # Mouse wheel down.
        send(master, output, b"3\x1b[B")  # Data/table selection.
        send(master, output, b"4")  # Graphics: progress, gauge, sparkline, canvas.
        send(master, output, b"5")  # Input and grapheme-aware editor state.
        send(master, output, "!".encode("utf-8"))
        send(master, output, b"\t")
        send(master, output, "\N{ROCKET}".encode("utf-8"))
        send(master, output, b"6")  # Interaction section.
        send(master, output, b"m\r")  # Modal open/close and focus restoration.
        send(master, output, b"\x10")  # Ctrl+P.
        send(master, output, b"Show Overview\r")  # Filter and execute a palette command.

        # Settings: keyboard 7, mouse language selection, RGB modal, live theme and reset.
        send(master, output, b"7")
        send(master, output, b"\x1b[B\r")  # English -> Türkçe.
        require("Ayarlar" in output.decode("utf-8", errors="replace"), "Turkish Settings UI missing")
        if args.adapter_id:
            before = len(output)
            send(master, output, b"8")
            adapter_render = output[before:].decode("utf-8", errors="replace")
            require("Adaptör İnceleyici" in adapter_render and "Adaptör Kimliği" in adapter_render and args.adapter_id in adapter_render, "Turkish adapter inspector missing")
            send(master, output, b"7")
        send(master, output, b"\x1b[<0;4;7M")  # Click English directly.
        require("Settings" in output.decode("utf-8", errors="replace"), "English Settings UI missing")
        send(master, output, b"\x1b[B\x1b[B\x1b[B\r")  # Select Primary color.
        send(master, output, b"\x7f\x7f\x7f200")  # R = 200.
        send(master, output, b"\x1b[B\x7f\x7f40")  # G = 40.
        send(master, output, b"\x1b[B\x7f\x7f50\r")  # B = 50, apply.
        require(
            b"\x1b[48;2;200;40;50m" in output,
            "custom primary color did not reach the rendered selected state",
        )
        send(master, output, b"\x1b[B" * 7 + b"\r")  # Reset Dragonfire Theme.
        require(
            b"\x1b[48;2;120;20;10m" in output,
            "Dragonfire reset did not restore the primary color",
        )

        for width, height in RESIZE_SEQUENCE[1:]:
            set_size(master, width, height)
            os.killpg(process.pid, signal.SIGWINCH)
            drain_for(master, output, 0.10)

        if m42_root is not None:
            # Keep a second controller-owned host active only for the outer
            # exit/signal cleanup path. Resetting via the capability browser
            # gives the fixture's sorted adapter table a deterministic origin.
            send(master, output, b"8")
            wait_for_current_text(master, output, "Adapter Inspector", 1.0, "could not return to adapters for final session cleanup")
            send(master, output, b"c")
            wait_for_current_text(master, output, "Capabilities", 1.0, "capability browser did not reset final session selection")
            send(master, output, b"c")
            wait_for_current_text(master, output, "Adapter Inspector", 1.0, "capability browser did not return after resetting final session selection")
            send(master, output, b"\x1b[B" * 8)
            # Only z-sessions declares this fixture surface, so the typed
            # discovery below proves both the selected adapter and controller
            # runtime availability without depending on a clipped detail pane.
            send(master, output, b"h")
            wait_for_session_browser(master, output, 2.0, "final declared session browser did not open")
            reset_session_markers(control)
            send(master, output, b"\r")
            wait_for_file(
                Path(f"{session_marker(control)}.ready"),
                1.0,
                "final delayed session did not publish provider readiness",
                master=master, output=output,
            )
            release_delayed_session(control)
            wait_for_session_host(master, output, 1.0, "second interactive host did not open")

        if args.exit == "q":
            send(master, output, b"q")
        elif args.exit == "ctrl-c":
            send(master, output, b"\x03")
        elif args.exit == "sigterm":
            os.killpg(process.pid, signal.SIGTERM)
        else:
            os.killpg(process.pid, signal.SIGHUP)
        deadline = time.monotonic() + 8.0
        while process.poll() is None and time.monotonic() < deadline:
            read_available(master, output, 0.05)
        if process.poll() is None:
            process.kill()
            raise RuntimeError("showcase did not exit after the requested key")
        drain_for(master, output, 0.10)
        if m42_control is not None:
            wait_for_session_marker_entries(
                m42_control,
                [],
                2.0,
                "outer shutdown retained the active provider session",
                master=master, output=output,
            )

        restored_mode = termios.tcgetattr(master)
        rendered = output.decode("utf-8", errors="replace")
        require(process.returncode == 0, f"showcase exited with {process.returncode}")
        require(
            "we are the recall" in rendered and "𝐍𝐨𝐰 𝐥𝐨𝐚𝐝𝐢𝐧𝐠" in rendered and "⣀" in rendered,
            "supplied Dragonfire splash artwork, manifesto, or loading text missing",
        )
        require(
            output.count(b"\x1b[38;2;") >= 6,
            "Dragonfire splash did not emit multiple true-color style regions",
        )
        require(
            "Overview" in rendered and "Primitive index" in rendered and "Settings" in rendered,
            "section rendering missing overview, widgets, or settings",
        )
        require("Show Overview" in rendered and "DragonsTUI" in rendered, "command palette or modal content missing")
        require("İstanbul" in rendered and "🚀" in rendered, "Unicode input content missing")
        require(any("⠀" <= char <= "⣿" for char in rendered), "Braille canvas output missing")
        require(b"\x1b[?1049h" in output and b"\x1b[?1049l" in output, "alternate-screen lifecycle missing")
        require(b"\x1b[?25l" in output and b"\x1b[?25h" in output, "cursor lifecycle missing")
        for mode in (1000, 1002, 1003, 1006, 1015):
            require(
                f"\x1b[?{mode}h".encode("ascii") in output
                and f"\x1b[?{mode}l".encode("ascii") in output,
                f"mouse reporting mode {mode} was not restored",
            )
        require(
            (restored_mode[3] & (termios.ICANON | termios.ECHO))
            == (original_mode[3] & (termios.ICANON | termios.ECHO)),
            "canonical/echo terminal mode was not restored",
        )
        prove_pty_usable_after_exit(master, slave_name, output)
    except Exception:
        # Retain the current frame before teardown even for marker/readiness
        # failures; historical output tails can hide the actual selected row.
        screen = fully_reconstructed_visible_text(output)
        print(f"PTY failure current screen:\n{screen}", file=sys.stderr)
        raise
    finally:
        try:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
        finally:
            os.close(master)
            cleanup_m42_fixture(fixture, daemon, endpoint)

    print(
        "Showcase PTY passed: six-second splash/animation, pages, settings, focus, keyboard, mouse, palette, modal, "
        "Unicode, Braille, resize sequence, lifecycle restoration, and "
        f"{args.exit} exit"
        + (", typed adapter lifecycle/diagnostics, conflict, failure, capability browser, M65 input/resize/close, "
           "output-pressure exit, abandoned-open cleanup, active-session shutdown, and fixture process cleanup." if m42_root is not None else ".")
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
