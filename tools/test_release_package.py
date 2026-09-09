#!/usr/bin/env python3
"""Deterministic release-helper tests; no Rust builds, services, or network."""
import io
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

import release_package as release


class ReleasePackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="release-test-", dir="/tmp")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / "repo"
        self.root.mkdir()
        for relative in ("Cargo.toml", "crates/dragonstui-adapter-host/Cargo.toml"):
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('[package]\nversion = "0.1.0"\n')
        (self.root / "LICENSE").write_text("Fixture MIT license\n")
        (self.root / "docs").mkdir()
        (self.root / "docs/installation.md").write_bytes(
            (release.ROOT / "docs/installation.md").read_bytes())
        (self.root / "tools").mkdir()
        (self.root / "tools/pty_smoke.py").write_bytes((release.ROOT / "tools/pty_smoke.py").read_bytes())
        self.bins = self.base / "bin"
        self.bins.mkdir()
        for name in release.BINARIES:
            binary = self.bins / name
            binary.write_text(f'''#!{sys.executable}
import os, sys, termios, tty
if sys.argv[1:] == ['--version']:
    print('{name} 0.1.0')
elif sys.argv[1:] == ['--help']:
    print('Usage: {name} [--help] [--version]')
elif sys.argv[1:2] == ['--root'] and sys.argv[-1:] == ['list']:
    print('ID\\tVERSION\\tSTATE\\tPROTOCOL')
else:
    old = termios.tcgetattr(0)
    try:
        tty.setraw(0)
        os.write(1, b'\\x1b[?1049h\\x1b[?25l\\x1b[?1003h')
        while b'q' not in os.read(0, 1024):
            pass
    finally:
        termios.tcsetattr(0, termios.TCSANOW, old)
        os.write(1, b'\\x1b[?1003l\\x1b[?25h\\x1b[?1049l')
''')
            binary.chmod(0o755)
        self.output = self.base / "out"

    def build(self, platform="macos-arm64"):
        return release.build("0.1.0", platform, self.bins, self.output, self.root)

    def test_package_includes_offline_installation_and_first_run_guide(self):
        archive, _ = self.build()
        with tarfile.open(archive, "r:gz") as bundle:
            source = bundle.extractfile("README.md")
            assert source is not None
            with source:
                text = source.read().decode("utf-8")
        for expected in ("$HOME/.local/bin", "--adapter-root", "--root", "uninstall", "update"):
            self.assertIn(expected, text)
        self.assertNotRegex(text, r"\]\([-a-z]+\.md(?:#[^)]+)?\)")

    def verify(self, archive, checksum):
        # Runtime composition uses executable Python fixtures, not native binaries.
        with mock.patch.object(release, "audit_dependencies"):
            release.verify("0.1.0", "macos-arm64", archive, checksum, self.root)

    def modified_archive(self, change):
        archive, checksum = self.build()
        with tarfile.open(archive, "r:gz") as bundle:
            entries = []
            for member in bundle:
                source = bundle.extractfile(member)
                assert source is not None
                with source:
                    entries.append((member, source.read()))
        change(entries)
        with tarfile.open(archive, "w:gz", format=tarfile.USTAR_FORMAT) as bundle:
            for member, data in entries:
                bundle.addfile(member, io.BytesIO(data) if member.isfile() else None)
        checksum.write_text(f"{release.sha256(archive)}  {archive.name}\n")
        return archive, checksum

    def test_build_inventory_modes_checksum_and_reproducibility(self):
        for platform in release.PLATFORMS:
            with self.subTest(platform=platform):
                archive, checksum = self.build(platform)
                first = archive.read_bytes()
                with tarfile.open(archive, "r:gz") as bundle:
                    members = release.validate_archive(bundle)
                    self.assertEqual({m.name: m.mode for m in members}, release.INVENTORY)
                    self.assertTrue(all(m.uid == m.gid == m.mtime == 0 for m in members))
                self.assertEqual(checksum.read_text(), f"{release.sha256(archive)}  {archive.name}\n")
                self.build(platform)
                self.assertEqual(first, archive.read_bytes())

    def test_both_manifest_versions_required(self):
        for relative in ("Cargo.toml", "crates/dragonstui-adapter-host/Cargo.toml"):
            path = self.root / relative
            original = path.read_text()
            path.write_text('[package]\nversion = "0.2.0"\n')
            with self.assertRaisesRegex(ValueError, "does not match"):
                self.build()
            path.write_text(original)

    def test_invalid_version_and_platform(self):
        for version in ("../0.1.0", "v0.1.0", "0.1.0/extra"):
            with self.assertRaises(ValueError):
                release.validate_version(version, self.root)
        with self.assertRaises(ValueError):
            self.build("unknown")

    def test_missing_nonexecutable_and_symlink_binary(self):
        binary = self.bins / release.BINARIES[0]
        binary.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "not executable"):
            self.build()
        binary.unlink()
        with self.assertRaises(FileNotFoundError):
            self.build()
        binary.symlink_to(self.bins / release.BINARIES[1])
        with self.assertRaisesRegex(ValueError, "regular"):
            self.build()

    def test_checksum_rejection_before_execution(self):
        archive, checksum = self.build()
        checksum.write_text("0" * 64 + f"  {archive.name}\n")
        with mock.patch.object(release, "run_checked") as runner:
            with self.assertRaisesRegex(ValueError, "checksum"):
                self.verify(archive, checksum)
            runner.assert_not_called()

    def test_checksum_wrong_filename_and_extra_line(self):
        archive, checksum = self.build()
        for text in (f"{release.sha256(archive)}  other.tar.gz\n",
                     f"{release.sha256(archive)}  {archive.name}\nextra\n"):
            checksum.write_text(text)
            with self.assertRaisesRegex(ValueError, "checksum"):
                self.verify(archive, checksum)

    def test_archive_filename_version_platform_rejection(self):
        archive, checksum = self.build("linux-x86_64")
        with self.assertRaisesRegex(ValueError, "filename"):
            self.verify(archive, checksum)

    def test_missing_duplicate_extra_and_unsafe_paths(self):
        changes = [lambda e: e.pop(), lambda e: e.append(e[0])]
        for path in ("../escape", "/absolute", "./dragons_tui", "sub/dragons_tui", "extra", "a\\b"):
            changes.append(lambda e, path=path: setattr(e[0][0], "name", path))
        for change in changes:
            with self.subTest(change=change):
                archive, checksum = self.modified_archive(change)
                with mock.patch.object(release, "run_checked") as runner:
                    with self.assertRaises(ValueError):
                        self.verify(archive, checksum)
                    runner.assert_not_called()
        self.assertFalse((self.base / "escape").exists())

    def test_links_directories_special_files_rejected(self):
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.DIRTYPE, tarfile.FIFOTYPE, tarfile.CHRTYPE):
            def change(entries):
                entries[0][0].type = kind
                entries[0][0].size = 0
            with self.subTest(kind=kind):
                archive, checksum = self.modified_archive(change)
                with self.assertRaises(ValueError):
                    self.verify(archive, checksum)

    def test_wrong_modes_rejected(self):
        for mode in (0o644, 0o777, 0o4755):
            archive, checksum = self.modified_archive(lambda e: setattr(e[0][0], "mode", mode))
            with self.assertRaisesRegex(ValueError, "mode"):
                self.verify(archive, checksum)

    def test_document_contents_checked(self):
        for name in ("README.md", "LICENSE"):
            def change(entries):
                index = next(i for i, (m, _) in enumerate(entries) if m.name == name)
                member, data = entries[index]
                entries[index] = (member, b"x" * len(data))
            archive, checksum = self.modified_archive(change)
            with self.assertRaisesRegex(ValueError, name.split(".")[0]):
                self.verify(archive, checksum)

    def test_environment_and_runtime_composition_cleanup(self):
        archive, checksum = self.build()
        calls = []
        real_runner = release.run_checked
        def runner(command, cwd, env, timeout=10):
            calls.append((command, cwd, env, timeout))
            self.assertFalse(cwd.is_relative_to(self.root))
            self.assertNotIn("AWS_SECRET_ACCESS_KEY", env)
            installed = command[0] in release.BINARIES
            expected_path = f"{env['HOME']}/.local/bin:/usr/bin:/bin" if installed else "/usr/bin:/bin"
            self.assertEqual(env["PATH"], expected_path)
            self.assertTrue(Path(env["HOME"]).is_dir())
            if installed or command[0] == "/usr/bin/install":
                return real_runner(command, cwd, env, timeout)
            if command[1] == "--version":
                return f"{Path(command[0]).name} 0.1.0\n"
            return "help or PTY success\n"
        with mock.patch.dict(os.environ, {"AWS_SECRET_ACCESS_KEY": "fixture-never-forward"}), \
             mock.patch.object(release, "run_checked", side_effect=runner):
            self.verify(archive, checksum)
        self.assertEqual(len(calls), 20)
        self.assertEqual(calls[8][0][1:3], ["-I", "-c"])
        self.assertIn("--exit', 'q'", calls[8][0][3])
        self.assertFalse(calls[0][1].exists())

    def test_version_and_empty_help_rejection(self):
        archive, checksum = self.build()
        for responses, message in ((["wrong 0.1.0"], "--version"),
                                   (["dragons_tui 0.1.0", ""], "--help")):
            with mock.patch.object(release, "run_checked", side_effect=responses):
                with self.assertRaisesRegex(ValueError, message):
                    self.verify(archive, checksum)

    def test_local_fixture_complete_verify_real_pty(self):
        self.verify(*self.build())

    def test_command_failure_and_deadline(self):
        env = release.isolated_environment(self.base)
        for script, timeout, message in (("raise SystemExit(4)", 5, "command failed"),
                                         ("import time; time.sleep(60)", 0.05, "deadline")):
            with self.assertRaisesRegex(ValueError, message):
                release.run_checked([sys.executable, "-I", "-c", script], self.base, env, timeout)

    def test_native_architecture_and_system_dependencies(self):
        cases = [
            ("macos-arm64", "Mach-O 64-bit executable arm64", "binary:\n\t/usr/lib/libSystem.B.dylib (compatibility version 1.0)", True),
            ("macos-arm64", "Mach-O 64-bit executable arm64", "binary:\n\t/opt/homebrew/lib/custom.dylib (compatibility version 1.0)", False),
            ("linux-x86_64", "ELF 64-bit LSB executable, x86-64", "linux-vdso.so.1 (0x1)\nlibc.so.6 => /lib/libc.so.6 (0x2)\n/lib64/ld-linux-x86-64.so.2 (0x3)", True),
            ("linux-x86_64", "ELF 64-bit LSB executable, x86-64", "libc.so.6 => not found", False),
            ("linux-x86_64", "ELF 64-bit LSB executable, x86-64", "libcustom.so => /opt/libcustom.so (0x1)", False),
            ("macos-arm64", "ELF 64-bit LSB executable, x86-64", "", False),
        ]
        for platform, description, dependencies, valid in cases:
            with self.subTest(platform=platform, dependencies=dependencies), mock.patch.object(
                release, "run_checked", side_effect=[description, dependencies]
            ):
                if valid:
                    release.audit_dependencies("binary", platform, self.base, {})
                else:
                    with self.assertRaises(ValueError):
                        release.audit_dependencies("binary", platform, self.base, {})


if __name__ == "__main__":
    unittest.main()
