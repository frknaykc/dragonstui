"""Dependency-free validation of current adapter-to-host protocol-v1 messages.

Mirrors crates/dragonstui-adapter-host/src/protocol.rs and runtime.rs::validate_info.
No profile, correlation, session-lifecycle, or fixture-event policy lives here.
In particular the current runtime requires a nonempty version, *not* SemVer,
and does not reject duplicate/undeclared actions or sessions or blank labels.
Manifest/runtime identity equality needs external context and belongs to callers.

Missing Option fields and explicit null are equivalent; default Vec and bool
fields may be omitted but cannot be null. Unknown additive fields are tolerated.
Input is an already-decoded JSON object; duplicate keys/lexical parsing, framing,
and line/depth resource limits belong to the transport, not this validator.
"""

import math
import re


class ProtocolError(Exception):
    """A stable generic failure code, never adapter-controlled raw content."""

    def __init__(self, code: str = "invalid_message") -> None:
        self.code = code
        super().__init__(code)


_SEGMENT = re.compile(r"[a-z0-9][a-z0-9_-]{0,63}")
_REQUEST = re.compile(r"[A-Za-z0-9._:-]{1,128}")
_TYPES = frozenset({"adapter_info", "response", "error", "event", "session_opened",
                    "session_output", "session_exit", "shutdown_ack"})
_SEVERITIES = frozenset({"trace", "debug", "info", "warning", "error", "critical"})
_STATUSES = frozenset({"ok", "warning", "error", "unknown"})
_U64_MAX = (1 << 64) - 1
_I32_MIN = -(1 << 31)
_I32_MAX = (1 << 31) - 1


def _require(condition, code="invalid_field"):
    if not condition:
        raise ProtocolError(code)


def _recognized_fields(obj, fields):
    # Serde rejects duplicate recognized struct fields, but ignores duplicate
    # additive fields and uses last-value semantics in opaque Value objects.
    _require(not (getattr(obj, "duplicate_keys", set()) & set(fields)), "duplicate_field")


def validate_identifier(value, kind: str) -> None:
    """Validate adapter/capability/action/request/session IDs without coercion.

    ASCII segments have a 64-byte maximum, requests 128 bytes. Dotted action
    and capability identifiers have *no aggregate length limit* in Rust.
    """
    _require(isinstance(value, str), "invalid_identifier")
    if kind == "request":
        valid = _REQUEST.fullmatch(value) is not None
    elif kind in ("adapter", "session"):
        valid = _SEGMENT.fullmatch(value) is not None
    elif kind in ("capability", "action"):
        valid = all(_SEGMENT.fullmatch(segment) is not None for segment in value.split("."))
    else:
        raise ProtocolError("invalid_identifier_kind")
    _require(valid, "invalid_identifier")


def _number(value):
    # serde_json without arbitrary_precision represents i64/u64 or finite f64.
    # An integer outside i64/u64 is not automatically invalid: the JSON parser
    # falls back to f64, provided that conversion remains finite.
    if type(value) not in (int, float):
        return False
    try:
        return math.isfinite(value)
    except OverflowError:
        return False


def _json_value(value):
    """Reject Python-only values and nonfinite numbers, even in opaque JSON."""
    pending = [(value, False)]
    # Iterative traversal avoids a Python recursion limit for deeply nested JSON.
    # Cycles cannot come from JSON; detect them without rejecting shared values.
    active = set()
    while pending:
        item, leaving = pending.pop()
        if leaving:
            active.remove(item)
            continue
        if item is None or type(item) is bool:
            continue
        if isinstance(item, str):
            try:
                item.encode("utf-8")
            except UnicodeEncodeError:
                raise ProtocolError("invalid_json_value") from None
        elif type(item) in (int, float):
            _require(_number(item), "invalid_json_value")
        elif isinstance(item, (dict, list)):
            identity = id(item)
            _require(identity not in active, "invalid_json_value")
            active.add(identity)
            pending.append((identity, True))
            if isinstance(item, dict):
                _require(all(isinstance(key, str) for key in item), "invalid_json_value")
                pending.extend((key, False) for key in item.keys())
                pending.extend((child, False) for child in item.values())
            else:
                pending.extend((child, False) for child in item)
        else:
            raise ProtocolError("invalid_json_value")


def _string(obj, field):
    _require(field in obj and isinstance(obj[field], str))


def _optional_string(obj, field):
    if obj.get(field) is not None:
        _string(obj, field)


def _identifier(obj, field, kind):
    validate_identifier(obj.get(field), kind)


def _vector(obj, field, default=False):
    value = obj.get(field, [] if default else None)
    _require(isinstance(value, list))
    return value


def _integer(value, minimum, maximum):
    _require(type(value) is int and minimum <= value <= maximum)


def _enum(value, choices, unit=False):
    # serde externally-tagged unit variants also deserialize {"variant": null}.
    # Internally-tagged discriminator fields must still be plain strings.
    if unit and isinstance(value, dict) and len(value) == 1:
        _require(not getattr(value, "duplicate_keys", set()))
        value, payload = next(iter(value.items()))
        _require(payload is None)
    _require(isinstance(value, str) and value in choices)


def _adapter_info(message):
    _identifier(message, "id", "adapter")
    _string(message, "version")
    _require(bool(message["version"]), "invalid_handshake")
    capabilities = _vector(message, "capabilities")
    for capability in capabilities:
        validate_identifier(capability, "capability")
    _require(bool(capabilities) and len(set(capabilities)) == len(capabilities),
             "invalid_handshake")
    for action in _vector(message, "actions", default=True):
        _require(isinstance(action, dict))
        _recognized_fields(action, {"id", "operation", "label", "description", "confirmation_required"})
        _identifier(action, "id", "action")
        _identifier(action, "operation", "capability")
        _string(action, "label")
        _optional_string(action, "description")
        _require(type(action.get("confirmation_required", False)) is bool)
    for session in _vector(message, "sessions", default=True):
        _require(isinstance(session, dict))
        _recognized_fields(session, {"capability", "label", "description"})
        _identifier(session, "capability", "capability")
        _string(session, "label")
        _optional_string(session, "description")


def _observation(observation):
    _require(isinstance(observation, dict))
    kind = observation.get("type")
    _enum(kind, {"log", "metric", "status", "event", "error"})
    fields = {"log": {"text", "severity"}, "metric": {"name", "value", "unit"},
              "status": {"entity", "check", "status"}, "event": {"title", "detail"},
              "error": {"message", "signature", "stack"}}[kind]
    _recognized_fields(observation, fields | {"type", "timestamp_millis"})
    timestamp = observation.get("timestamp_millis")
    if timestamp is not None:
        _integer(timestamp, 0, _U64_MAX)
    if kind == "log":
        _string(observation, "text")
        if observation.get("severity") is not None:
            _enum(observation["severity"], _SEVERITIES, unit=True)
    elif kind == "metric":
        _string(observation, "name")
        _require(_number(observation.get("value")))
        _optional_string(observation, "unit")
    elif kind == "status":
        _string(observation, "entity")
        _string(observation, "check")
        _enum(observation.get("status"), _STATUSES, unit=True)
    elif kind == "event":
        _string(observation, "title")
        _optional_string(observation, "detail")
    elif kind == "error":
        _string(observation, "message")
        _optional_string(observation, "signature")
        _require(all(isinstance(line, str) for line in _vector(observation, "stack", default=True)))


def validate_message(message: dict) -> None:
    """Accept a current v1 adapter-originated envelope or raise ProtocolError.

    Does not mutate input or fill defaults. Callers may use .get('actions', [])
    and .get('sessions', []) after successful validation; explicit null fails.
    """
    _require(isinstance(message, dict), "invalid_message")
    _json_value(message)
    _require(type(message.get("protocol")) is int and message["protocol"] == 1,
             "unsupported_protocol")
    kind = message.get("type")
    _require(isinstance(kind, str) and kind in _TYPES, "invalid_message_type")
    assert isinstance(kind, str)
    fields = {"adapter_info": {"id", "version", "capabilities", "actions", "sessions"},
              "response": {"id", "payload"}, "error": {"id", "code", "message"},
              "event": {"stream", "kind", "payload", "observation"},
              "session_opened": {"id", "session_id"}, "session_output": {"session_id", "data"},
              "session_exit": {"session_id", "exit_code"}, "shutdown_ack": set()}[kind]
    _recognized_fields(message, fields | {"type", "protocol"})
    if kind == "adapter_info":
        _adapter_info(message)
    elif kind == "response":
        _identifier(message, "id", "request")
        _require("payload" in message)
    elif kind == "error":
        if message.get("id") is not None:
            _identifier(message, "id", "request")
        _string(message, "code")
        _string(message, "message")
    elif kind == "event":
        _string(message, "stream")
        _string(message, "kind")
        _require("payload" in message)
        if message.get("observation") is not None:
            _observation(message["observation"])
    elif kind == "session_opened":
        _identifier(message, "id", "request")
        _identifier(message, "session_id", "session")
    elif kind == "session_output":
        _identifier(message, "session_id", "session")
        _string(message, "data")
    elif kind == "session_exit":
        _identifier(message, "session_id", "session")
        if message.get("exit_code") is not None:
            _integer(message["exit_code"], _I32_MIN, _I32_MAX)
