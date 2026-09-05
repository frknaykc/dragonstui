#!/usr/bin/env python3
"""Test-only batch bridge to the actual M67 parser and validator (no peers).

Input is one JSON array of raw wire strings; output is one boolean per string.
The Rust integration test supplies the repository tools directory explicitly.
Do not replace the imports with a second schema implementation.
"""

import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(sys.argv[1]).resolve()))

from adapter_conformance_protocol import ProtocolError, validate_message
from adapter_conformance_transport import _constant, _float, _int, _pairs


def accepts(raw):
    try:
        message = json.loads(
            raw, object_pairs_hook=_pairs, parse_float=_float, parse_int=_int, parse_constant=_constant
        )
        validate_message(message)
    except (ProtocolError, ValueError, OverflowError, RecursionError):
        return False
    return True


def main():
    # Finite, repository-owned corpus only. Fail loudly on bridge/harness errors.
    cases = json.load(sys.stdin)
    if not isinstance(cases, list) or len(cases) > 4096:
        raise ValueError("expected a bounded array of wire strings")
    if any(not isinstance(raw, str) or len(raw) > 16384 for raw in cases):
        raise ValueError("expected bounded wire strings")
    json.dump([accepts(raw) for raw in cases], sys.stdout)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
