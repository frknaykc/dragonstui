#!/usr/bin/env python3
"""Small independent protocol peer used only by the M67 runner regression tests."""
import argparse
import json
from pathlib import Path
import sys
import time
import os

parser = argparse.ArgumentParser()
parser.add_argument("--mode", default="minimal")
parser.add_argument("--launch-marker", type=Path)
args = parser.parse_args()
if args.launch_marker:
    with args.launch_marker.open("a") as marker:
        marker.write(str(os.getpid()) + "\n")


def emit(message_type, **fields):
    print(json.dumps({"type": message_type, "protocol": 1, **fields}), flush=True)


if args.mode == "no-read":
    while True:
        time.sleep(1)

hello = json.loads(sys.stdin.readline())
if args.mode == "malformed":
    print("not-json", flush=True)
    raise SystemExit(0)
if args.mode == "secret-diagnostic":
    print("PRIVATE_TOKEN_EXAMPLE_DONT_REPORT", file=sys.stderr, flush=True)
    print("PRIVATE_TOKEN_EXAMPLE_DONT_REPORT", flush=True)
    raise SystemExit(0)

capabilities = ["sample.echo"]
metadata = {}
if args.mode.startswith("session"):
    capabilities.append("sample.session")
    metadata["sessions"] = [{"capability": "sample.session", "label": "Sample session"}]
if args.mode == "duplicate-capability":
    capabilities *= 2
if args.mode == "undeclared-action":
    metadata["actions"] = [{"id": "sample.action", "label": "Example", "operation": "missing.operation"}]
if args.mode == "confirmation":
    metadata["actions"] = [{"id": "sample.action", "label": "Example", "operation": "sample.echo", "confirmation_required": True}]
if args.mode == "additive":
    metadata["future_additive_field"] = {"anything": True}

emit("adapter_info", id="other" if args.mode == "wrong-id" else "sample", version="1.0.0",
     capabilities=capabilities, **metadata,
     **({"protocol": 2} if args.mode == "wrong-protocol" else {}))
if args.mode == "premature-exit":
    raise SystemExit(0)
if args.mode == "bad-observation":
    emit("event", stream="sample", kind="opaque", payload={}, observation={"type": "unknown"})
if args.mode == "global-error":
    emit("error", code="notice", message="Unscoped producer diagnostic")

pending = []
for line in sys.stdin:
    message = json.loads(line)
    kind = message["type"]
    if kind == "request":
        if args.mode == "out-of-order":
            pending.append(message)
            if len(pending) < 2:
                continue
            for request in reversed(pending):
                emit("response", id=request["id"], payload=request["payload"])
            pending.clear()
        else:
            identity = "unknown-request" if args.mode == "wrong-response" else message["id"]
            emit("response", id=identity, payload=message["payload"])
            if args.mode == "duplicate-response":
                emit("response", id=identity, payload=message["payload"])
    elif kind == "session_open":
        emit("session_opened", id=message["id"], session_id="sample-session")
        emit("session_output", session_id="wrong-session" if args.mode == "session-wrong-id" else "sample-session", data="ready")
    elif kind == "session_input":
        data = message["data"]
        emit("session_output", session_id="sample-session", data="echo:")
        emit("session_output", session_id="sample-session", data=data)
    elif kind == "session_close" and args.mode != "session-no-exit":
        emit("session_exit", session_id="sample-session", exit_code=None)
    elif kind == "shutdown":
        if args.mode == "no-shutdown":
            continue
        emit("shutdown_ack")
        if args.mode == "trailing-message":
            emit("event", stream="sample", kind="late", payload={})
        raise SystemExit(7 if args.mode == "nonzero-shutdown" else 0)
