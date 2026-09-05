import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from reference_mock_fixture import create_fixture


@unittest.skipUnless(os.name == "posix", "POSIX launcher")
class ReferenceFixtureTests(unittest.TestCase):
    def test_existing_root_is_never_modified(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            keep = root / "keep"
            keep.write_text("user work")
            with self.assertRaises(FileExistsError):
                create_fixture(root, Path("/bin/sh"))
            self.assertEqual(keep.read_text(), "user work")
            self.assertEqual(list(root.iterdir()), [keep])

    def test_invalid_binary_does_not_create_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary = Path(tmp) / "not-executable"
            binary.write_text("not executable")
            root = Path(tmp) / "new"
            with self.assertRaises(ValueError):
                create_fixture(root, binary)
            self.assertFalse(root.exists())

    def test_symlink_root_is_not_followed_or_removed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "link"
            target = Path(tmp) / "target"
            target.mkdir()
            root.symlink_to(target, target_is_directory=True)
            with self.assertRaises(FileExistsError):
                create_fixture(root, Path("/bin/sh"))
            self.assertTrue(root.is_symlink())
            self.assertTrue(target.is_dir())
            self.assertEqual(list(target.iterdir()), [])

    def test_copy_failure_rolls_back_only_new_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "new"
            keep = Path(tmp) / "keep"
            keep.write_text("preserved")
            with patch("reference_mock_fixture.shutil.copy2", side_effect=OSError("copy failed")):
                with self.assertRaisesRegex(OSError, "copy failed"):
                    create_fixture(root, Path("/bin/sh"))
            self.assertFalse(root.exists())
            self.assertEqual(keep.read_text(), "preserved")

    def test_quoted_paths_and_gates_reach_executable_as_exact_arguments(self):
        for gated in (False, True):
            with self.subTest(gated=gated), tempfile.TemporaryDirectory() as tmp:
                # The fixture probe prints actual argv; no provider response is simulated here.
                source = Path(tmp) / "argv probe's binary"
                source.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\"\n")
                source.chmod(0o700)
                root = Path(tmp) / "root's space"
                result = create_fixture(root, source, gated=gated)
                source.unlink()  # Launcher must use its own copy, not the build/source path.
                args = subprocess.check_output([str(root / "reference/bin/launch")], text=True).splitlines()
                control = root / ".reference-control"
                expected = ["--mode", "reference", "--id", "reference",
                            "--action-marker", str(control / "actions"),
                            "--session-marker", str(control / "sessions")]
                if gated:
                    expected += ["--event-release", str(control / "observations.release"),
                                 "--action-release", str(control / "actions.release")]
                self.assertEqual(args, expected)
                self.assertEqual(result["root"], str(root))
                self.assertEqual(root.stat().st_mode & 0o777, 0o700)
                manifest = json.loads((root / "reference/adapter.json").read_text())
                self.assertEqual(manifest["executable"], "bin/launch")
                self.assertEqual(manifest["id"], "reference")
                self.assertEqual(list(control.iterdir()), [])  # Setup starts nothing.


if __name__ == "__main__":
    unittest.main()
