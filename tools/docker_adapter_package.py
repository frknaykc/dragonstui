#!/usr/bin/env python3
"""Build a local, single-executable Docker adapter registry (no execution/download).

The script artifact requires Python 3.10+ and the Docker CLI on the user's PATH.
It is separate from the four native DragonsTUI application binaries.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import stat

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.1.0"


def build(output: Path, source: Path = ROOT / "adapters/docker/dragonstui-docker") -> dict:
    source = source.resolve(strict=True)
    if not source.is_file() or not 0 < source.stat().st_size <= 1024 * 1024:
        raise ValueError("adapter source must be a nonempty regular file under 1 MiB")
    data = source.read_bytes()
    if not data.startswith(b"#!/usr/bin/env python3\n"):
        raise ValueError("expected the standalone Python adapter executable")
    # Refuse existing outputs, including symlinks. Never overwrite another package.
    output = output.absolute()
    output.mkdir(mode=0o700, parents=False, exist_ok=False)
    output = output.resolve()
    try:
        artifact = output / "dragonstui-docker"
        artifact.write_bytes(data)
        artifact.chmod(stat.S_IRUSR | stat.S_IWUSR | stat.S_IXUSR |
                       stat.S_IRGRP | stat.S_IXGRP | stat.S_IROTH | stat.S_IXOTH)
        digest = hashlib.sha256(data).hexdigest()
        artifacts = [
            {"os": os_name, "architecture": architecture,
             # Host v1 file:// transport treats the suffix as a literal path.
             "source": "file://" + str(artifact.resolve()), "sha256": digest,
             "size": len(data), "executable": "dragonstui-docker"}
            for os_name in ("linux", "macos")
            for architecture in ("aarch64", "x86_64")
        ]
        registry = {"adapters": [{"id": "docker", "name": "Docker",
            "description": "Container operations and explicitly bound exec sessions; requires Python and Docker CLI",
            "releases": [{"version": VERSION, "protocol_version": 1,
                          "artifacts": artifacts}]}]}
        (output / "registry.json").write_text(json.dumps(registry, indent=2) + "\n", encoding="utf-8")
        (output / "dragonstui-docker.sha256").write_text(
            f"{digest}  dragonstui-docker\n", encoding="ascii")
        return {"registry": str(output / "registry.json"), "artifact": str(artifact),
                "sha256": digest, "version": VERSION,
                "note": "Local registry only; platform entries require Python 3.10+ and Docker CLI. No provider started."}
    except BaseException:
        shutil.rmtree(output)
        raise


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="new directory; parent must exist")
    args = parser.parse_args()
    print(json.dumps(build(args.output), indent=2))


if __name__ == "__main__":
    main()
