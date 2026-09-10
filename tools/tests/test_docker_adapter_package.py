import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from tools.packaging.docker_adapter_package import build


class DockerPackageTests(unittest.TestCase):
    def test_single_file_registry_checksum_and_platforms_without_execution(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source = base / "source"
            content = b"#!/usr/bin/env python3\nraise RuntimeError('must not execute')\n"
            source.write_bytes(content)
            result = build(base / "package with spaces", source)
            artifact = Path(result["artifact"])
            self.assertEqual(artifact.read_bytes(), content)
            self.assertEqual(artifact.stat().st_mode & 0o777, 0o755)
            self.assertEqual(result["sha256"], hashlib.sha256(content).hexdigest())
            entry = json.loads(Path(result["registry"]).read_text())["adapters"][0]
            self.assertEqual(entry["id"], "docker")
            entries = entry["releases"][0]["artifacts"]
            self.assertEqual({(e["os"], e["architecture"]) for e in entries},
                             {(o, a) for o in ("macos", "linux") for a in ("aarch64", "x86_64")})
            for item in entries:
                self.assertEqual(Path(item["source"].removeprefix("file://")), artifact)
                self.assertEqual(item["size"], len(content))
                self.assertEqual(item["sha256"], result["sha256"])

    def test_refuses_existing_output_and_symlink(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source = base / "source"
            source.write_text("#!/usr/bin/env python3\npass\n")
            destination = base / "owned"
            destination.mkdir()
            marker = destination / "preserve"
            marker.write_text("unchanged")
            link = base / "link"
            link.symlink_to(destination, target_is_directory=True)
            for output in (destination, link):
                with self.assertRaises(FileExistsError):
                    build(output, source)
                self.assertEqual(marker.read_text(), "unchanged")

    def test_invalid_source_leaves_no_output(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source = base / "source"
            source.write_text("not an adapter")
            with self.assertRaises(ValueError):
                build(base / "output", source)
            self.assertFalse((base / "output").exists())


if __name__ == "__main__":
    unittest.main()
