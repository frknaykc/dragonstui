"""Isolated M73 registry fixture and bounded authenticated controller queries."""
import hashlib
import json
import platform
import socket
import subprocess
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.fixtures.reference_mock_fixture import create_fixture


def install_fixture(root: Path, mock: Path, controller: Path) -> Path:
    """Install a local launcher through the real CLI, never into an existing root.

    The launcher references a task-owned copy outside the installed directory.
    This deliberately tests single-artifact installation, not archive packaging.
    """
    root.mkdir(mode=0o700)
    source = root / ".reference-source"
    create_fixture(source, mock, gated=True)
    launcher = source / "reference/bin/launch"
    data = launcher.read_bytes()
    os_name = {"Darwin": "macos", "Linux": "linux"}[platform.system()]
    architecture = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64"}[platform.machine()]
    registry = root / ".reference-registry.json"
    registry.write_text(json.dumps({"adapters": [{"id": "reference", "name": "Reference Mock",
        "releases": [{"version": "1.0.0", "protocol_version": 1, "artifacts": [{
            "os": os_name, "architecture": architecture, "source": launcher.as_uri(),
            "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
            "executable": "bin/launch"}]}]}]}), encoding="utf-8")
    subprocess.run([str(controller), "--root", str(root), "install", "reference",
                    "--registry", str(registry)], check=True, timeout=10, capture_output=True)
    installed = root / "reference"
    if (installed / "bin/launch").read_bytes() != data:
        raise RuntimeError("installed launcher differs from registry artifact")
    if not (installed / "adapter-install.json").is_file():
        raise RuntimeError("install provenance absent")
    control = source / ".reference-control"
    if (control / "sessions").exists() or (control / "actions").exists():
        raise RuntimeError("installation unexpectedly executed provider")
    return control


def query(endpoint: dict, command: dict) -> dict:
    """Never include endpoint credentials or wire bytes in diagnostics."""
    address = endpoint["address"]
    if not address.startswith("127.0.0.1:"):
        raise ValueError("fixture endpoint must be loopback")
    port = int(address.rsplit(":", 1)[1])
    with socket.create_connection(("127.0.0.1", port), timeout=2) as stream:
        stream.sendall(json.dumps({"token": endpoint["token"], "command": command}).encode() + b"\n")
        with stream.makefile("rb") as reader:
            line = reader.readline(1024 * 1024 + 1)
    if len(line) > 1024 * 1024 or not line.endswith(b"\n"):
        raise RuntimeError("invalid fixture controller response framing")
    result = json.loads(line)
    if result.get("error") is not None:
        raise RuntimeError("fixture controller rejected command")
    return result["status"]
