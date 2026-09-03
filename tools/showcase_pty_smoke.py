#!/usr/bin/env python3
"""Exercise the release DragonsTUI showcase in a real POSIX pseudo-terminal."""

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

RESIZE_SEQUENCE = ((160, 55), (120, 40), (80, 24), (40, 15), (20, 8), (5, 3), (1, 1), (80, 24))


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


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--exit", choices=("q", "ctrl-c"), default="q")
    parser.add_argument("--skip-splash", action="store_true")
    parser.add_argument("--adapter-id", help="expect this discovered adapter in the M41 inspector")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("provide a showcase binary after --")

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

        # Explicit visible header tabs: 1 Overview through 8 Adapters.
        for x, marker in (
            (3, "Static benchmark context"),
            (14, "Primitive index"),
            (24, "Data contract"),
            (31, "Canvas · Braille waveform"),
            (42, "TextInput · grapheme aware"),
            (50, "Focus · KeyMap · Style"),
            (64, "Language"),
        ):
            before = len(output)
            send(master, output, f"\x1b[<0;{x};3M".encode("ascii"))
            require(marker in output[before:].decode("utf-8", errors="replace"), f"header click missed {marker}")

        # Adapter management remains an optional application integration: the
        # showcase must expose its empty host root honestly via every entry path.
        before = len(output)
        send(master, output, b"8")
        adapter_render = output[before:].decode("utf-8", errors="replace")
        if args.adapter_id:
            require("Adapter Inspector" in adapter_render and args.adapter_id in adapter_render, "keyboard navigation did not open the adapter inspector")
        else:
            require("No installed adapters" in adapter_render, "keyboard navigation did not open the empty Adapters section")
        send(master, output, b"1")
        before = len(output)
        send(master, output, b"\x1b[<0;75;3M")
        adapter_render = output[before:].decode("utf-8", errors="replace")
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

        send(master, output, b"q" if args.exit == "q" else b"\x03")
        deadline = time.monotonic() + 8.0
        while process.poll() is None and time.monotonic() < deadline:
            read_available(master, output, 0.05)
        if process.poll() is None:
            process.kill()
            raise RuntimeError("showcase did not exit after the requested key")
        drain_for(master, output, 0.10)

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
        require(b"\x1b[?1003h" in output and b"\x1b[?1003l" in output, "mouse-capture lifecycle missing")
        require(
            (restored_mode[3] & (termios.ICANON | termios.ECHO))
            == (original_mode[3] & (termios.ICANON | termios.ECHO)),
            "canonical/echo terminal mode was not restored",
        )
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=2)
        os.close(master)

    print(
        "Showcase PTY passed: six-second splash/animation, pages, settings, focus, keyboard, mouse, palette, modal, "
        "Unicode, Braille, resize sequence, lifecycle restoration, and "
        f"{args.exit} exit."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
