"""Offline contract regressions for the protocol-v1 conformance validator."""

import copy
import unittest

from adapter_conformance_protocol import ProtocolError, validate_identifier, validate_message


def envelope(message_type, **fields):
    return {"type": message_type, "protocol": 1, **fields}


def info(**fields):
    return envelope("adapter_info", **{
        "id": "fixture", "version": "1.0.0", "capabilities": ["fixture.echo"],
        **fields,
    })


def event(observation=None, **fields):
    return envelope("event", stream="", kind="", payload=None,
                    observation=observation, **fields)


class ProtocolTests(unittest.TestCase):
    def rejected(self, message, code=None):
        with self.assertRaises(ProtocolError) as caught:
            validate_message(message)
        self.assertIsInstance(caught.exception.code, str)
        self.assertEqual(str(caught.exception), caught.exception.code)
        if code is not None:
            self.assertEqual(caught.exception.code, code)

    def test_all_adapter_to_host_envelopes(self):
        messages = [info(), envelope("response", id="REQ:1", payload=None),
                    envelope("error", code="", message=""), event(),
                    envelope("session_opened", id="REQ:1", session_id="s1"),
                    envelope("session_output", session_id="s1", data=""),
                    envelope("session_exit", session_id="s1"), envelope("shutdown_ack")]
        for message in messages:
            with self.subTest(kind=message["type"]):
                self.assertIsNone(validate_message(message))

    def test_protocol_is_required_strict_integer_one(self):
        for value in [None, True, False, 1.0, "1", 0, 2, -1, 2**32]:
            with self.subTest(value=value):
                self.rejected({"type": "shutdown_ack", "protocol": value})
        self.rejected({"type": "shutdown_ack"})

    def test_unknown_and_host_direction_types_rejected(self):
        for kind in [None, [], {}, "unknown", "hello", "request", "shutdown",
                     "session_open", "session_input", "session_resize", "session_close"]:
            with self.subTest(kind=kind):
                self.rejected(envelope(kind))
        for value in [None, [], "object", 1]:
            self.rejected(value)

    def test_required_fields_cannot_be_omitted_or_wrong_type(self):
        examples = [info(), envelope("response", id="r", payload={}),
                    envelope("error", code="", message=""),
                    envelope("event", stream="", kind="", payload={}),
                    envelope("session_opened", id="r", session_id="s"),
                    envelope("session_output", session_id="s", data="")]
        for message in examples:
            for field in message.keys() - {"type", "protocol"}:
                with self.subTest(kind=message["type"], field=field):
                    missing = dict(message)
                    del missing[field]
                    self.rejected(missing)
                    if field != "payload":
                        self.rejected({**message, field: None})
                        self.rejected({**message, field: True})

    def test_identifier_exact_boundaries_and_alphabets(self):
        for kind in ["adapter", "session", "capability", "action"]:
            for value in ["a", "0", "a_-0", "a" * 64]:
                validate_identifier(value, kind)
            for value in ["", "a" * 65, "A", "_a", "-a", "é", "a\n", True, None]:
                with self.subTest(kind=kind, value=value), self.assertRaises(ProtocolError):
                    validate_identifier(value, kind)
        for kind in ["capability", "action"]:
            validate_identifier(".".join(["a" * 64] * 8), kind)
            for value in ["a..b", ".a", "a.", "a.B"]:
                with self.assertRaises(ProtocolError):
                    validate_identifier(value, kind)
        for kind in ["adapter", "session"]:
            with self.assertRaises(ProtocolError):
                validate_identifier("a.b", kind)
        for value in ["A:._-0", ":", "X" * 128]:
            validate_identifier(value, "request")
        for value in ["X" * 129, "", "a/b", "é", "a\n", False]:
            with self.assertRaises(ProtocolError):
                validate_identifier(value, "request")
        with self.assertRaises(ProtocolError):
            validate_identifier("a", "unknown")

    def test_handshake_capabilities_nonempty_unique_typed(self):
        for capabilities in [[], None, "fixture.echo", ["x", "x"], ["X"], [None]]:
            self.rejected(info(capabilities=capabilities))
        validate_message(info(capabilities=["arbitrary", "other.surface"]))
        self.rejected(info(version=""))

    def test_runtime_does_not_require_semver_or_validate_declaration_semantics(self):
        # runtime.rs::validate_info only checks version nonempty, identity match,
        # protocol, and unique/nonempty capabilities. Do not invent M68 rules.
        action = {"id": "arbitrary", "label": " ", "operation": "undeclared"}
        session = {"capability": "undeclared", "label": ""}
        validate_message(info(version="non-semver", actions=[action, action],
                              sessions=[session, session]))

    def test_metadata_defaults_and_nullable_options(self):
        validate_message(info(actions=[{"id": "a", "label": "", "operation": "x",
                                        "description": None}],
                              sessions=[{"capability": "x", "label": "", "description": None}]))
        for field in ["actions", "sessions"]:
            for value in [None, {}, "", [None]]:
                self.rejected(info(**{field: value}))
        for value in [None, 0, 1, "false"]:
            self.rejected(info(actions=[{"id": "a", "label": "", "operation": "x",
                                         "confirmation_required": value}]))
        for value in [True, False]:
            validate_message(info(actions=[{"id": "a", "label": "", "operation": "x",
                                            "confirmation_required": value}]))
        for field in ["id", "label", "operation"]:
            action = {"id": "a", "label": "", "operation": "x"}
            del action[field]
            self.rejected(info(actions=[action]))
        for field in ["capability", "label"]:
            session = {"capability": "x", "label": ""}
            del session[field]
            self.rejected(info(sessions=[session]))
        self.rejected(info(actions=[{"id": "a", "label": "", "operation": "x", "description": 1}]))
        self.rejected(info(sessions=[{"capability": "x", "label": "", "description": False}]))

    def test_optional_error_id_and_exit_code(self):
        for value in [None, "REQ:1"]:
            validate_message(envelope("error", id=value, code="", message=""))
        for value in [None, -(2**31), 0, 2**31 - 1]:
            validate_message(envelope("session_exit", session_id="s", exit_code=value))
        for value in [True, False, 0.0, "0", -(2**31) - 1, 2**31]:
            self.rejected(envelope("session_exit", session_id="s", exit_code=value))
        self.rejected(envelope("error", id="bad/request", code="", message=""))

    def test_all_observation_shapes_and_nullable_fields(self):
        samples = [
            {"type": "log", "text": "", "severity": None},
            {"type": "metric", "name": "", "value": 1, "unit": None},
            {"type": "status", "entity": "", "check": "", "status": "unknown"},
            {"type": "event", "title": "", "detail": None},
            {"type": "error", "message": "", "signature": None},
        ]
        for sample in samples:
            with self.subTest(kind=sample["type"]):
                validate_message(event(sample))
                validate_message(event({**sample, "timestamp_millis": None}))
                for field, value in sample.items():
                    if value is not None:
                        missing = dict(sample)
                        del missing[field]
                        self.rejected(event(missing))
                        self.rejected(event({**sample, field: None}))
        for value in [[], "log", {}, {"type": "timeline"}, {"type": []}]:
            self.rejected(event(value))

    def test_observation_enum_values(self):
        for severity in ["trace", "debug", "info", "warning", "error", "critical"]:
            validate_message(event({"type": "log", "text": "", "severity": severity}))
        for status in ["ok", "warning", "error", "unknown"]:
            validate_message(event({"type": "status", "entity": "", "check": "", "status": status}))
        for value in ["warn", "INFO", "", 0, True, [], {}]:
            self.rejected(event({"type": "log", "text": "", "severity": value}))
            self.rejected(event({"type": "status", "entity": "", "check": "", "status": value}))

    def test_serde_unit_enums_accept_single_key_null_objects(self):
        # serde's externally-tagged unit enums accept both string and map form.
        validate_message(event({"type": "log", "text": "", "severity": {"info": None}}))
        validate_message(event({"type": "status", "entity": "", "check": "",
                                "status": {"ok": None}}))
        for value in [{"info": False}, {"info": []}, {"info": None, "warning": None}]:
            self.rejected(event({"type": "log", "text": "", "severity": value}))
        self.rejected(event({"type": {"log": None}, "text": ""}))

    def test_json_cycles_and_shared_values(self):
        cyclic = []
        cyclic.append(cyclic)
        self.rejected(envelope("response", id="r", payload=cyclic))
        shared = [1, 2]
        validate_message(envelope("response", id="r", payload=[shared, shared]))
        self.rejected(envelope("response", id="r", payload=(1,)))
        self.rejected(envelope("response", id="r", payload={"\ud800": "value"}))
        self.rejected(envelope("shutdown_ack", future="\udfff"))

    def test_timestamp_is_nullable_u64_not_float_or_bool(self):
        for value in [None, 0, 2**64 - 1]:
            validate_message(event({"type": "event", "title": "", "timestamp_millis": value}))
        for value in [-1, 2**64, True, 1.0, "1"]:
            self.rejected(event({"type": "event", "title": "", "timestamp_millis": value}))

    def test_metric_numbers_support_finite_large_integer_fallback(self):
        for value in [0, -1, 1.25, 2**64, -(2**64), 10**300, 1e308]:
            validate_message(event({"type": "metric", "name": "", "value": value}))
        for value in [None, True, False, "1", [], {}, float("nan"), float("inf"),
                      -float("inf"), 10**400]:
            self.rejected(event({"type": "metric", "name": "", "value": value}))

    def test_stack_default_is_not_nullable(self):
        validate_message(event({"type": "error", "message": "", "stack": ["", "line"]}))
        for value in [None, "line", [1], [None], [True]]:
            self.rejected(event({"type": "error", "message": "", "stack": value}))

    def test_additive_fields_and_opaque_payload_preserved(self):
        message = event({"type": "metric", "name": "", "value": 1,
                         "future": {"opaque": [None, True]}}, future={"anything": 7})
        message["payload"] = {"domain": [None, True, 1, 1.25, "", {"x": "y"}]}
        original = copy.deepcopy(message)
        validate_message(message)
        self.assertEqual(message, original)
        validate_message(info(actions=[{"id": "a", "label": "", "operation": "x", "future": []}],
                              sessions=[{"capability": "x", "label": "", "future": None}], future=1))

    def test_invalid_non_json_values_and_sanitized_errors(self):
        for value in [float("nan"), float("inf"), 10**400, {1: "value"}, {"x"}, b"bytes", "\ud800"]:
            self.rejected(envelope("response", id="r", payload=value))
        secret = "secret-invalid-request/value"
        with self.assertRaises(ProtocolError) as caught:
            validate_message(envelope("response", id=secret, payload={"secret": secret}))
        self.assertNotIn(secret, str(caught.exception))
        self.assertNotIn(secret, repr(caught.exception))


if __name__ == "__main__":
    unittest.main()
