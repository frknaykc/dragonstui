#!/usr/bin/env python3
"""Create a private POSIX reference-adapter root; never start a process."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil


def create_fixture(root: Path, mock_binary: Path, *, gated: bool = False) -> dict[str, str]:
    if os.name != "posix":
        raise ValueError("reference fixture launcher requires POSIX /bin/sh")
    binary = mock_binary.resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError("mock binary must be an executable file")
    root = root.absolute()
    # Exclusive creation is intentional: never replace a user's adapter store.
    root.mkdir(mode=0o700)
    try:
        control = root / ".reference-control"
        control.mkdir(mode=0o700)
        adapter = root / "reference"
        binaries = adapter / "bin"
        binaries.mkdir(parents=True)
        copied = binaries / "reference-mock"
        shutil.copy2(binary, copied)
        arguments = [str(copied), "--mode", "reference", "--id", "reference",
                     "--action-marker", str(control / "actions"),
                     "--session-marker", str(control / "sessions")]
        if gated:
            arguments += ["--event-release", str(control / "observations.release"),
                          "--action-release", str(control / "actions.release")]
        launcher = binaries / "launch"
        launcher.write_text("#!/bin/sh\nexec " + shlex.join(arguments) + "\n", encoding="utf-8")
        launcher.chmod(0o700)
        (adapter / "adapter.json").write_text(json.dumps({
            "id": "reference", "name": "Reference Mock", "version": "1.0.0",
            "protocol_version": 1, "executable": "bin/launch",
        }, indent=2) + "\n", encoding="utf-8")
        return {"root": str(root), "control": str(control), "adapter_id": "reference",
                "binary_sha256": hashlib.sha256(copied.read_bytes()).hexdigest()}
    except BaseException:
        # This invocation exclusively created the root; it owns this rollback.
        shutil.rmtree(root)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path, help="NEW root (parent must exist)")
    parser.add_argument("--mock", required=True, type=Path, help="built adapter-host mock executable")
    parser.add_argument("--gated", action="store_true", help="hold second event batch and delayed action until marker release")
    args = parser.parse_args()
    try:
        result = create_fixture(args.root, args.mock, gated=args.gated)
    except (OSError, ValueError) as error:
        parser.exit(2, f"reference fixture: {error}\n")
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
