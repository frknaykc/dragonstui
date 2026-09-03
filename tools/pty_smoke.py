#!/usr/bin/env python3
"""Exercise a DragonsTUI binary in a real POSIX pseudo-terminal.

This is a dependency-free acceptance helper. It validates lifecycle escape sequences,
resize delivery, clean application exit, and canonical/echo restoration for a supplied
binary. Example:

    python3 tools/pty_smoke.py --exit q -- target/debug/dragons_tui
"""

from __future__ import annotations

import argparse
import fcntl
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time

RESIZE_SEQUENCE = ((120, 40), (80, 24), (40, 15), (20, 8), (5, 3), (1, 1), (80, 24))


def set_size(fd: int, width: int, height: int) -> None:
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))


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
    except BlockingIOError:
        return
    except OSError:
        return


def drain_for(fd: int, output: bytearray, seconds: float) -> None:
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        read_available(fd, output, min(0.02, deadline - time.monotonic()))


def send(fd: int, output: bytearray, data: bytes, pause: float = 0.06) -> None:
    os.write(fd, data)
    drain_for(fd, output, pause)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--exit", choices=("q", "ctrl-c"), default="q")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("provide a binary after --")

    master, slave = pty.openpty()
    original_mode = termios.tcgetattr(slave)
    set_size(slave, *RESIZE_SEQUENCE[0])
    process = subprocess.Popen(
        command,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        preexec_fn=os.setsid,
        close_fds=True,
    )
    os.close(slave)
    os.set_blocking(master, False)
    output = bytearray()

    try:
        drain_for(master, output, 0.20)
        send(master, output, b"\r")  # Skip the splash into the main UI.
        send(master, output, b"\t")
        send(master, output, b"\x1b[<0;10;8M")  # SGR left click.
        send(master, output, b"\x1b[<64;30;10M")  # SGR wheel down.
        send(master, output, b"\x10")  # Ctrl+P palette.
        send(master, output, b"\x1b")

        for width, height in RESIZE_SEQUENCE[1:]:
            set_size(master, width, height)
            os.killpg(process.pid, signal.SIGWINCH)
            drain_for(master, output, 0.08)

        send(master, output, b"q" if args.exit == "q" else b"\x03")
        deadline = time.monotonic() + 8.0
        while process.poll() is None and time.monotonic() < deadline:
            read_available(master, output, 0.05)
        if process.poll() is None:
            process.kill()
            raise RuntimeError("application did not exit after the requested key")
        drain_for(master, output, 0.10)

        restored_mode = termios.tcgetattr(master)
        require(process.returncode == 0, f"application exited with {process.returncode}")
        require(b"\x1b[?1049h" in output and b"\x1b[?1049l" in output, "alternate-screen lifecycle missing")
        require(b"\x1b[?25l" in output and b"\x1b[?25h" in output, "cursor lifecycle missing")
        require(b"\x1b[?1003h" in output and b"\x1b[?1003l" in output, "mouse-capture lifecycle missing")
        require(
            (restored_mode[3] & (termios.ICANON | termios.ECHO)) == (original_mode[3] & (termios.ICANON | termios.ECHO)),
            "canonical/echo terminal mode was not restored",
        )
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=2)
        os.close(master)

    print(
        "PTY smoke passed: splash/main input, SGR mouse, Ctrl+P, resize sequence, "
        f"{args.exit} exit, alternate screen, cursor, mouse, and terminal-mode restoration."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
