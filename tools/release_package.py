#!/usr/bin/env python3
"""Build and verify local native release bundles (Python 3.11+, stdlib only).

This executes trusted local release binaries, not arbitrary downloaded archives.
Checksum verification establishes integrity, not publisher authenticity.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
BINARIES = ("dragons_tui", "dragonstui-showcase", "dragonstui-adapter",
            "dragonstui-adapter-host-mock")
PLATFORMS = ("macos-arm64", "linux-x86_64")
INVENTORY = {**dict.fromkeys(BINARIES, 0o755), "LICENSE": 0o644, "README.md": 0o644}
MAX_MEMBER_SIZE = 512 * 1024 * 1024


def validate_version(version: str, root: Path = ROOT) -> str:
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", version):
        raise ValueError("invalid release version")
    for relative in ("Cargo.toml", "crates/dragonstui-adapter-host/Cargo.toml"):
        with (root / relative).open("rb") as source:
            actual = tomllib.load(source)["package"]["version"]
        if actual != version:
            raise ValueError(f"version {version} does not match {relative}: {actual}")
    return version


def archive_name(version: str, platform: str) -> str:
    if platform not in PLATFORMS:
        raise ValueError("unsupported release platform")
    return f"dragonstui-v{version}-{platform}.tar.gz"


def readme(version: str, platform: str, root: Path = ROOT) -> bytes:
    guide = (root / "docs/installation.md").read_text(encoding="utf-8")
    # The full guide ships offline without expanding the six-file inventory.
    guide = re.sub(r"\]\(([-a-z]+\.md)(#[^)]+)?\)",
                   r"](https://github.com/frknaykc/dragonstui/blob/master/docs/\1\2)", guide)
    return (f"# DragonsTUI {version} ({platform})\n\n"
            "Use this bundle only on its named operating system and architecture.\n"
            "See LICENSE (MIT).\n\n" + guide).encode("utf-8")


def sha256(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def build(version: str, platform: str, bin_dir: Path, output: Path,
          root: Path = ROOT) -> tuple[Path, Path]:
    validate_version(version, root)
    name = archive_name(version, platform)
    sources = {name: bin_dir / name for name in BINARIES}
    sources["LICENSE"] = root / "LICENSE"
    for name, path in sources.items():
        mode = path.lstat().st_mode
        if not stat.S_ISREG(mode) or not 0 < path.stat().st_size <= MAX_MEMBER_SIZE:
            raise ValueError(f"not a nonempty regular input file: {path}")
        if name in BINARIES and not mode & 0o111:
            raise ValueError(f"binary is not executable: {path}")
    output.mkdir(parents=True, exist_ok=True)
    archive = output / archive_name(version, platform)
    # Fixed ordering, owner, permissions, timestamps and gzip header make repeated
    # packaging of identical inputs byte-for-byte reproducible.
    with tempfile.TemporaryDirectory(prefix=".package-", dir=output) as staging:
        staged = Path(staging) / archive.name
        with staged.open("wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", filename="", mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as bundle:
                for name, mode in INVENTORY.items():
                    data = readme(version, platform, root) if name == "README.md" else sources[name].read_bytes()
                    info = tarfile.TarInfo(name)
                    info.mode, info.size, info.mtime = mode, len(data), 0
                    bundle.addfile(info, io.BytesIO(data))
        checksum = archive.with_name(archive.name + ".sha256")
        staged_checksum = Path(staging) / checksum.name
        staged_checksum.write_text(f"{sha256(staged)}  {archive.name}\n", encoding="ascii")
        staged.replace(archive)
        staged_checksum.replace(checksum)
    return archive, checksum


def validate_archive(bundle: tarfile.TarFile) -> list[tarfile.TarInfo]:
    members = []
    seen = set()
    for member in bundle:
        # Flat layout: no directory entries, aliases, links, special files, or
        # extension metadata. Check everything before writing any payload.
        if member.name not in INVENTORY or member.name in seen:
            raise ValueError(f"unexpected or duplicate archive path: {member.name!r}")
        if not member.isfile() or member.type != tarfile.REGTYPE or member.linkname or member.pax_headers:
            raise ValueError(f"archive member is not a plain regular file: {member.name}")
        if member.mode != INVENTORY[member.name]:
            raise ValueError(f"unexpected mode for {member.name}")
        if not 0 < member.size <= MAX_MEMBER_SIZE:
            raise ValueError(f"invalid size for {member.name}")
        seen.add(member.name)
        members.append(member)
    if seen != set(INVENTORY):
        raise ValueError(f"missing archive members: {sorted(set(INVENTORY) - seen)}")
    return members


def isolated_environment(base: Path) -> dict[str, str]:
    env = {"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C", "TERM": "xterm-256color"}
    for key, directory in {"HOME": "home", "XDG_CONFIG_HOME": "config",
                           "XDG_DATA_HOME": "data", "XDG_CACHE_HOME": "cache",
                           "XDG_STATE_HOME": "state", "XDG_RUNTIME_DIR": "runtime",
                           "TMPDIR": "tmp"}.items():
        path = base / directory
        path.mkdir(mode=0o700)
        env[key] = str(path)
    return env


def run_checked(command: list[str], cwd: Path, env: dict[str, str], timeout: float = 10) -> str:
    with subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          text=True, start_new_session=True) as process:
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            # The PTY wrapper handles TERM by unwinding pty_smoke's finally block,
            # including its dashboard child (which has its own process group).
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.communicate(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.communicate()
            raise ValueError(f"command deadline exceeded: {command[0]}") from None
        if process.returncode:
            raise ValueError(f"command failed ({process.returncode}): {command[0]}: {stderr.strip()}")
        return stdout


PTY_WRAPPER = """
import importlib.util, signal, sys

def stop(signum, frame):
    raise RuntimeError('release PTY deadline exceeded')

signal.signal(signal.SIGTERM, stop)
spec = importlib.util.spec_from_file_location('release_pty_smoke', sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
sys.argv = [sys.argv[1], '--exit', 'q', '--', sys.argv[2]]
raise SystemExit(module.main())
"""


def audit_dependencies(executable: str, platform: str, cwd: Path, env: dict[str, str]) -> None:
    description = run_checked(["/usr/bin/file", executable], cwd, env)
    required = ("Mach-O 64-bit", "arm64") if platform == "macos-arm64" else ("ELF 64-bit", "x86-64")
    if not all(value in description for value in required):
        raise ValueError(f"binary architecture/format mismatch: {description.strip()}")
    if platform == "macos-arm64":
        linked = run_checked(["/usr/bin/otool", "-L", executable], cwd, env)
        dependencies = [line.strip().split(" (", 1)[0] for line in linked.splitlines()[1:] if line.strip()]
        if not dependencies or any(not path.startswith(("/usr/lib/", "/System/Library/")) for path in dependencies):
            raise ValueError(f"non-system macOS dependency: {linked}")
    else:
        linked = run_checked(["/usr/bin/ldd", executable], cwd, env)
        allowed = {"libc.so.6", "libm.so.6", "libgcc_s.so.1", "libpthread.so.0",
                   "libdl.so.2", "librt.so.1", "ld-linux-x86-64.so.2", "linux-vdso.so.1"}
        names = [Path(line.split()[0]).name for line in linked.splitlines() if line.strip()]
        if not names or "not found" in linked or any(name not in allowed for name in names):
            raise ValueError(f"missing or non-baseline Linux dependency: {linked}")


def verify(version: str, platform: str, archive: Path, checksum: Path,
           root: Path = ROOT) -> None:
    validate_version(version, root)
    if archive.name != archive_name(version, platform):
        raise ValueError("archive filename does not match requested version/platform")
    expected = f"{sha256(archive)}  {archive.name}\n"
    if checksum.read_text(encoding="ascii") != expected:
        raise ValueError("checksum mismatch or invalid checksum manifest")
    # Explicit system temp root, never caller TMPDIR or repository-controlled cwd.
    with tempfile.TemporaryDirectory(prefix="dragonstui-release-", dir="/tmp") as directory:
        base = Path(directory).resolve()
        if base.is_relative_to(root.resolve()):
            raise ValueError("verification directory must be outside repository")
        extracted = base / "package"
        extracted.mkdir(mode=0o700)
        with tarfile.open(archive, "r:gz") as bundle:
            members = validate_archive(bundle)
            for member in members:
                source = bundle.extractfile(member)
                if source is None:
                    raise ValueError(f"missing archive payload: {member.name}")
                with source, (extracted / member.name).open("xb") as destination:
                    shutil.copyfileobj(source, destination)
                (extracted / member.name).chmod(member.mode)
        if (extracted / "README.md").read_bytes() != readme(version, platform, root):
            raise ValueError("README does not match release version/platform")
        if (extracted / "LICENSE").read_bytes() != (root / "LICENSE").read_bytes():
            raise ValueError("LICENSE does not match repository")
        env = isolated_environment(base)
        for name in BINARIES:
            executable = str(extracted / name)
            audit_dependencies(executable, platform, extracted, env)
            if run_checked([executable, "--version"], extracted, env).strip() != f"{name} {version}":
                raise ValueError(f"unexpected --version output: {name}")
            help_output = run_checked([executable, "--help"], extracted, env)
            if not help_output.strip() or "\x1b" in help_output:
                raise ValueError(f"empty or terminal-control --help output: {name}")
        run_checked([sys.executable, "-I", "-c", PTY_WRAPPER,
                     str(root / "tools/pty_smoke.py"), str(extracted / "dragons_tui")],
                    extracted, env, timeout=20)
        verify_user_install(extracted, env, version)


def verify_user_install(extracted: Path, env: dict[str, str], version: str) -> None:
    """Exercise documented install/replacement/uninstall only in verifier-owned HOME."""
    bin_dir = Path(env["HOME"]) / ".local/bin"
    bin_dir.mkdir(parents=True)
    keep_binary = bin_dir / "unrelated-tool"
    keep_binary.write_text("unrelated fixture\n")
    keep_data = Path(env["XDG_DATA_HOME"]) / "retain.txt"
    keep_data.write_text("retained fixture\n")
    installed_env = {**env, "PATH": f"{bin_dir}:{env['PATH']}"}
    install = ["/usr/bin/install", "-m", "755",
               *(str(extracted / name) for name in BINARIES), str(bin_dir)]
    # Same-version replacement checks filesystem mechanics, not migration between versions.
    for _ in range(2):
        run_checked(install, extracted, env)
        for name in BINARIES:
            if run_checked([name, "--version"], extracted, installed_env).strip() != f"{name} {version}":
                raise ValueError(f"installed PATH version mismatch: {name}")
    missing_root = Path(env["XDG_DATA_HOME"]) / "dragonstui/adapters"
    listing = run_checked(["dragonstui-adapter", "--root", str(missing_root), "list"],
                          extracted, installed_env)
    if listing != "ID\tVERSION\tSTATE\tPROTOCOL\n" or missing_root.exists():
        raise ValueError("first-run adapter list must be empty and read-only")
    for name in BINARIES:
        (bin_dir / name).unlink()
    if set(bin_dir.iterdir()) != {keep_binary} or keep_binary.read_text() != "unrelated fixture\n":
        raise ValueError("uninstall changed an unrelated executable")
    if keep_data.read_text() != "retained fixture\n":
        raise ValueError("install/update/uninstall changed retained data")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("build", "verify"):
        command = commands.add_parser(name)
        command.add_argument("--version", required=True)
        command.add_argument("--platform", required=True, choices=PLATFORMS)
        if name == "build":
            command.add_argument("--bin-dir", required=True, type=Path)
            command.add_argument("--output", required=True, type=Path, help="output directory")
        else:
            command.add_argument("--archive", required=True, type=Path)
            command.add_argument("--checksum", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "build":
            for path in build(args.version, args.platform, args.bin_dir, args.output):
                print(path)
        else:
            verify(args.version, args.platform, args.archive, args.checksum)
            print(f"Verified {args.archive.name}: inventory, SHA-256, native architecture/system dependencies, version/help, dashboard PTY q lifecycle, user install/PATH/empty root/replacement/uninstall")
    except (OSError, ValueError, tarfile.TarError, EOFError) as error:
        parser.exit(1, f"release package: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
