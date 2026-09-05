#!/usr/bin/env python3
"""Bounded protocol-v1 test peer for explicitly supplied POSIX adapter executables."""

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import sys
import tempfile
import time

from adapter_conformance_protocol import ProtocolError, validate_identifier, validate_message
from adapter_conformance_transport import Peer, TransportError


class ScenarioError(Exception):
    def __init__(self, code):
        self.code = code
        super().__init__(code)


def require(condition, code):
    if not condition:
        raise ScenarioError(code)


def keys(value, allowed, required=()):
    require(isinstance(value, dict), "profile_object_required")
    require(set(value) <= set(allowed) and set(required) <= set(value), "profile_fields")


def bounded_int(value, low, high):
    return type(value) is int and low <= value <= high


def outgoing_frame(message):
    wire = json.dumps({"protocol": 1, **message}, ensure_ascii=True, allow_nan=False, separators=(",", ":"))
    require(len(wire.encode("ascii")) + 1 <= 65536, "profile_frame_limit")


def validate_profile(profile):
    keys(profile, {"schema_version", "requests", "session", "events"}, {"schema_version"})
    require(type(profile["schema_version"]) is int and profile["schema_version"] == 1,
            "profile_version")
    requests = profile.get("requests", [])
    require(isinstance(requests, list) and len(requests) <= 32, "profile_request_limit")
    names = set()
    for request in requests:
        keys(request, {"name", "operation", "payload", "action", "confirmed", "expect",
                       "response_payload", "error_code"}, {"name", "operation", "payload", "expect"})
        name = request["name"]
        require(isinstance(name, str) and 0 < len(name) <= 80 and name not in names,
                "profile_request_name")
        names.add(name)
        validate_identifier(request["operation"], "capability")
        if "action" in request:
            validate_identifier(request["action"], "action")
        if "confirmed" in request:
            require(type(request["confirmed"]) is bool and "action" in request,
                    "profile_confirmation")
        require(request["expect"] in ("response", "error", "either"), "profile_expectation")
        if "response_payload" in request:
            require(request["expect"] == "response", "profile_response_expectation")
        if "error_code" in request:
            require(request["expect"] == "error" and isinstance(request["error_code"], str),
                    "profile_error_expectation")
    if "session" in profile:
        session = profile["session"]
        keys(session, {"capability", "rows", "columns", "input", "expect_output_contains", "resize", "exit_code"},
             {"capability", "rows", "columns"})
        validate_identifier(session["capability"], "capability")
        require(all(bounded_int(session[k], 1, 65535) for k in ("rows", "columns")), "profile_dimensions")
        if "input" in session:
            require(isinstance(session["input"], str) and len(session["input"].encode("utf-8")) <= 8192,
                    "profile_input")
        if "expect_output_contains" in session:
            text = session["expect_output_contains"]
            require(isinstance(text, str) and 0 < len(text.encode("utf-8")) <= 8192,
                    "profile_output_expectation")
        if "resize" in session:
            keys(session["resize"], {"rows", "columns"}, {"rows", "columns"})
            require(all(bounded_int(session["resize"][k], 1, 65535) for k in ("rows", "columns")),
                    "profile_dimensions")
        if "exit_code" in session:
            require(session["exit_code"] is None or bounded_int(session["exit_code"], -(2**31), 2**31-1),
                    "profile_exit_code")
    if "events" in profile:
        events = profile["events"]
        keys(events, {"minimum", "observation_types", "require_generic"}, {"minimum"})
        require(bounded_int(events["minimum"], 1, 4096), "profile_event_minimum")
        kinds = events.get("observation_types", [])
        require(isinstance(kinds, list) and all(isinstance(k, str) for k in kinds), "profile_observation_types")
        require(len(kinds) == len(set(kinds)) and set(kinds) <= {"log", "metric", "status", "event", "error"},
                "profile_observation_types")
        if "require_generic" in events:
            require(type(events["require_generic"]) is bool, "profile_generic_expectation")
    # Budget outgoing complete frames before starting an executable.
    for request in requests:
        wire = {k: request[k] for k in ("operation", "action", "payload") if k in request}
        wire.update(type="request", id="conformance-request-31")
        outgoing_frame(wire)
    if "session" in profile:
        session = profile["session"]
        outgoing_frame({"type": "session_open", "id": "conformance-open-0",
                        **{k: session[k] for k in ("capability", "rows", "columns")}})
        # Provider IDs have at most 64 ASCII bytes. Budget the worst valid ID.
        if "input" in session:
            outgoing_frame({"type": "session_input", "session_id": "s" * 64, "data": session["input"]})
        if "resize" in session:
            outgoing_frame({"type": "session_resize", "session_id": "s" * 64, **session["resize"]})
    return profile


def load_profile(path):
    if path is None:
        return {"schema_version": 1}
    def pairs(items):
        value = {}
        for key, item in items:
            require(key not in value, "profile_duplicate_key")
            value[key] = item
        return value
    def constant(_):
        raise ScenarioError("profile_nonfinite_number")
    with Path(path).open("rb") as source:
        data = source.read(131073)
    require(len(data) <= 131072, "profile_file_limit")
    profile = json.loads(data, object_pairs_hook=pairs, parse_constant=constant)
    json.dumps(profile, allow_nan=False, ensure_ascii=False).encode("utf-8")
    return validate_profile(profile)


class Suite:
    def __init__(self, command, profile, expected_id, expected_version, timeout, cancelled=None):
        self.command = command
        self.profile = profile
        self.expected_id = expected_id
        self.expected_version = expected_version
        self.timeout = timeout
        self.cancelled = cancelled
        self.checks = []
        self.observed = {"events": 0, "generic_events": 0, "observation_types": {}}
        self.info = None
        self.initial_info = None
        self.peer = None
        self.active_session = None
        self.session_exit = None
        self.session_output = ""
        self.session_output_matched = False
        self.pending = {}
        self.replies = {}
        self.open_request = None
        self.stage = "launch"

    def deadline(self):
        return time.monotonic() + self.timeout

    def check(self, name, callback):
        self.stage = name
        callback()
        self.checks.append({"name": name, "status": "passed"})

    def skip(self, name, reason="not_requested"):
        self.checks.append({"name": name, "status": "skipped", "reason": reason})

    def send(self, message, deadline=None):
        assert self.peer is not None
        self.peer.send({"protocol": 1, **message}, deadline if deadline is not None else self.deadline())

    def receive(self, deadline):
        assert self.peer is not None
        message = self.peer.receive(deadline)
        validate_message(message)
        return message

    def route(self, message):
        kind = message["type"]
        if kind == "event":
            self.observed["events"] += 1
            observation = message.get("observation")
            if observation is None:
                self.observed["generic_events"] += 1
            else:
                kinds = self.observed["observation_types"]
                label = observation["type"]
                kinds[label] = kinds.get(label, 0) + 1
        elif kind in ("response", "error"):
            correlation = message.get("id")
            if kind == "error" and correlation is None:
                self.observed["uncorrelated_errors"] = self.observed.get("uncorrelated_errors", 0) + 1
                return
            if kind == "error" and correlation == self.open_request and self.open_request is not None:
                raise ScenarioError("session_open_rejected")
            require(correlation in self.pending, "uncorrelated_reply")
            self.replies[correlation] = message
            del self.pending[correlation]
        elif kind == "session_opened":
            require(self.open_request is not None and message["id"] == self.open_request,
                    "uncorrelated_session_open")
            require(self.active_session is None, "duplicate_session_open")
            self.active_session = message["session_id"]
            self.open_request = None
        elif kind == "session_output":
            require(self.active_session is not None and message["session_id"] == self.active_session,
                    "unknown_session_output")
            # A bounded sliding window permits a profile assertion across chunk boundaries.
            text = self.profile.get("session", {}).get("expect_output_contains")
            if text is not None:
                combined = self.session_output + message["data"]
                self.session_output_matched |= text in combined
                self.session_output = combined[-len(text):]
        elif kind == "session_exit":
            require(self.active_session is not None and message["session_id"] == self.active_session,
                    "unknown_session_exit")
            self.session_exit = message
            self.active_session = None
        else:
            raise ScenarioError("unexpected_message_order")

    def until(self, predicate, deadline):
        while not predicate():
            self.route(self.receive(deadline))

    def handshake(self):
        deadline = self.deadline()
        self.send({"type": "hello", "host_version": "dragonstui-conformance-v1"}, deadline)
        message = self.receive(deadline)
        require(message["type"] == "adapter_info", "adapter_info_required")
        require(message["id"] == self.expected_id, "adapter_identity_mismatch")
        if self.expected_version is not None:
            require(message["version"] == self.expected_version, "adapter_version_mismatch")
        self.info = message
        if self.initial_info is None:
            self.initial_info = message

    def declarations(self):
        assert self.info is not None
        capabilities = self.info["capabilities"]
        # The v1 document requires action/session references to existing capabilities.
        # This cross-reference check is separate from permissive wire decoding.
        require(all(a["operation"] in capabilities for a in self.info.get("actions", [])),
                "action_capability_not_declared")
        require(all(s["capability"] in capabilities for s in self.info.get("sessions", [])),
                "session_capability_not_declared")
        actions = {a["id"]: a for a in self.info.get("actions", [])}
        for request in self.profile.get("requests", []):
            require(request["operation"] in capabilities, "profile_operation_not_declared")
            if "action" in request:
                require(sum(a["id"] == request["action"] for a in self.info.get("actions", [])) == 1,
                        "profile_action_identity_ambiguous")
                action = actions.get(request["action"])
                require(action is not None and action["operation"] == request["operation"],
                        "profile_action_not_declared")
                assert action is not None
                require(not action.get("confirmation_required", False) or request.get("confirmed") is True,
                        "profile_confirmation_required")
        if "session" in self.profile:
            declared = {s["capability"] for s in self.info.get("sessions", [])}
            require(self.profile["session"]["capability"] in declared, "profile_session_not_declared")

    def requests(self):
        deadline = self.deadline()
        cases = self.profile["requests"]
        # Unique IDs and pipelining permit valid out-of-order provider replies.
        for index, case in enumerate(cases):
            correlation = f"conformance-request-{index}"
            self.pending[correlation] = case
            message = {"type": "request", "id": correlation, "operation": case["operation"], "payload": case["payload"]}
            if "action" in case:
                message["action"] = case["action"]
            self.send(message, deadline)
        self.until(lambda: not self.pending, deadline)
        for index, case in enumerate(cases):
            reply = self.replies[f"conformance-request-{index}"]
            require(case["expect"] == "either" or case["expect"] == reply["type"], "request_outcome_mismatch")
            if "response_payload" in case:
                # JSON numbers and booleans are distinct; Python's True == 1 is not.
                actual = json.dumps(reply["payload"], sort_keys=True, separators=(",", ":"), allow_nan=False)
                expected = json.dumps(case["response_payload"], sort_keys=True, separators=(",", ":"), allow_nan=False)
                require(actual == expected, "request_payload_mismatch")
            if "error_code" in case:
                require(reply["code"] == case["error_code"], "request_error_code_mismatch")
            self.checks.append({"name": "request." + case["name"], "status": "passed"})
        self.replies.clear()

    def session(self):
        case = self.profile["session"]
        self.open_request = "conformance-open-0"
        self.send({"type": "session_open", "id": self.open_request, "capability": case["capability"],
                   "rows": case["rows"], "columns": case["columns"]})
        self.until(lambda: self.active_session is not None, self.deadline())
        if "input" in case:
            self.send({"type": "session_input", "session_id": self.active_session, "data": case["input"]})
        if "expect_output_contains" in case:
            self.until(lambda: self.session_output_matched, self.deadline())
        require(self.active_session is not None, "session_exited_before_close")
        if "resize" in case:
            self.send({"type": "session_resize", "session_id": self.active_session, **case["resize"]})
        self.send({"type": "session_close", "session_id": self.active_session})
        self.until(lambda: self.session_exit is not None, self.deadline())
        assert self.session_exit is not None
        if "exit_code" in case:
            require(self.session_exit.get("exit_code") == case["exit_code"], "session_exit_code_mismatch")

    def events(self):
        case = self.profile["events"]
        def observed():
            return (self.observed["events"] >= case["minimum"]
                    and set(case.get("observation_types", [])) <= set(self.observed["observation_types"])
                    and (not case.get("require_generic", False) or self.observed["generic_events"] > 0))
        self.until(observed, self.deadline())

    def shutdown(self):
        assert self.peer is not None
        deadline = self.deadline()
        self.send({"type": "shutdown"}, deadline)
        while True:
            message = self.receive(deadline)
            if message["type"] == "shutdown_ack":
                break
            self.route(message)
        require(self.peer.finish(deadline) == 0, "shutdown_exit_failure")

    def run(self):
        # A test-owned working directory, not a production adapter root/store.
        with tempfile.TemporaryDirectory(prefix="dragonstui-conformance-") as directory:
            for cycle in range(2):
                prefix = "restart." if cycle else ""
                self.stage = prefix + "launch"
                try:
                    self.peer = Peer(self.command, directory, cancelled=self.cancelled)
                    self.check(prefix + "handshake", self.handshake)
                    if cycle == 0:
                        self.check("declarations", self.declarations)
                        if self.profile.get("requests"):
                            self.check("requests", self.requests)
                        else:
                            self.skip("requests")
                        if "session" in self.profile:
                            self.check("session", self.session)
                        else:
                            self.skip("session")
                        if "events" in self.profile:
                            self.check("events", self.events)
                        else:
                            self.skip("events")
                    self.check(prefix + "shutdown", self.shutdown)
                except (ScenarioError, ProtocolError, TransportError) as error:
                    self.checks.append({"name": self.stage, "status": "failed", "code": error.code})
                except OSError:
                    self.checks.append({"name": self.stage, "status": "failed", "code": "process_io_error"})
                finally:
                    if self.peer is not None:
                        try:
                            cleanup = self.peer.close()
                        except (OSError, TransportError):
                            cleanup = {"reaped": False, "forced": True}
                        self.checks.append({"name": prefix + "cleanup", "status": "passed" if cleanup["reaped"] and not cleanup["forced"] and not cleanup.get("cleanup_error", False) else "failed",
                                            "reaped": cleanup["reaped"], "forced": cleanup["forced"],
                                            "cleanup_error": cleanup.get("cleanup_error", False),
                                            "stderr_bytes": self.peer.stderr_bytes})
                        self.peer = None
                if any(check["status"] == "failed" for check in self.checks):
                    if cycle == 0:
                        self.skip("restart", "prior_failure")
                    break
        failed = any(check["status"] == "failed" for check in self.checks)
        return {"schema_version": 1, "protocol": 1, "status": "failed" if failed else "passed",
                "scope": "requested_scenarios_only", "adapter_id": self.expected_id,
                "declared_capabilities": self.initial_info["capabilities"] if self.initial_info else [],
                "checks": self.checks, "observed": self.observed}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, epilog="Runs the supplied executable twice with your user permissions. Not a sandbox or universal certificate.")
    parser.add_argument("--expect-id", required=True)
    parser.add_argument("--expect-version")
    parser.add_argument("--profile", type=Path, help="Opt-in JSON scenarios; omitted means handshake/shutdown/restart only")
    parser.add_argument("--timeout-ms", type=int, default=2000, help="Per-phase budget (100..30000; default 2000)")
    parser.add_argument("command", nargs=argparse.REMAINDER, help="-- EXECUTABLE [ARG ...]; arguments are not interpreted by a shell")
    args = parser.parse_args(argv)
    try:
        require(os.name == "posix", "posix_required")
        validate_identifier(args.expect_id, "adapter")
        if args.expect_version is not None:
            validate_message({"type": "adapter_info", "protocol": 1, "id": args.expect_id,
                              "version": args.expect_version, "capabilities": ["conformance.probe"]})
        require(bounded_int(args.timeout_ms, 100, 30000), "timeout_range")
        profile = load_profile(args.profile)
        command = args.command[1:] if args.command[:1] == ["--"] else args.command
        require(bool(command), "executable_required")
        executable = shutil.which(command[0])
        require(executable is not None, "executable_not_found")
        command = [str(Path(executable).resolve()), *command[1:]]
    except (ScenarioError, ProtocolError) as error:
        report = {"schema_version": 1, "status": "configuration_error", "code": error.code}
    except (ValueError, OSError, UnicodeError, RecursionError):
        report = {"schema_version": 1, "status": "configuration_error", "code": "invalid_profile"}
    else:
        cancellation = {"signal": None}
        def interrupt(signum, _frame):
            # Flag only: never interrupt Popen halfway through acquiring ownership.
            # The nonblocking transport observes this flag at each bounded poll.
            if cancellation["signal"] is None:
                cancellation["signal"] = signum
        signals = (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)
        previous = {s: signal.getsignal(s) for s in signals}
        try:
            for s in signals:
                signal.signal(s, interrupt)
            report = Suite(command, profile, args.expect_id, args.expect_version, args.timeout_ms / 1000,
                           cancelled=lambda: cancellation["signal"] is not None).run()
        finally:
            for s, handler in previous.items():
                signal.signal(s, handler)
        if cancellation["signal"] is not None:
            report["status"] = "interrupted"
            report["signal"] = cancellation["signal"]
    print(json.dumps(report, ensure_ascii=True, allow_nan=False, sort_keys=True))
    if report["status"] == "interrupted":
        return 128 + report["signal"]
    return {"passed": 0, "failed": 1, "configuration_error": 2}[report["status"]]


if __name__ == "__main__":
    raise SystemExit(main())
