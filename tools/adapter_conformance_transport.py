"""Bounded POSIX stdio transport for isolated developer protocol-test peers.

This is not a controller, production adapter manager, or sandbox. The caller
owns the private cwd, executable selection, and absolute monotonic deadlines.
Limits cover incoming stdout + stderr bytes and lifetime stdout frame count;
max_frame_bytes includes the terminating newline (and bounds outgoing frames).
JSON booleans remain valid payload values, not integer configuration values.
"""
from collections import deque
import json
import math
import os
from pathlib import Path
import selectors
import signal
import subprocess
import time


class TransportError(Exception):
    """Content-free error; code is suitable for reports without peer output."""

    def __init__(self, code: str):
        self.code = code
        super().__init__(code)


class JsonObject(dict):
    """Retain duplicate-field evidence for typed schema checks, not payload policy."""

    def __init__(self):
        super().__init__()
        self.duplicate_keys = set()


def _pairs(items):
    result = JsonObject()
    for key, value in items:
        key.encode('utf-8')
        # Validate strings even when a later duplicate overwrites their value.
        # Nested objects have already passed this hook; walk only arrays here.
        pending = [value]
        while pending:
            item = pending.pop()
            if isinstance(item, str):
                item.encode('utf-8')
            elif isinstance(item, list):
                pending.extend(item)
        if key in result:
            result.duplicate_keys.add(key)
        result[key] = value
    return result


def _constant(_value):
    raise ValueError('nonfinite number')


def _float(value):
    # Match serde_json's default (non-float_roundtrip) u64-significand path.
    # The Rust/Python parity corpus guards this dependency-sensitive boundary.
    negative = value.startswith('-')
    mantissa, _, explicit = value.lstrip('-').lower().partition('e')
    whole, _, fraction = mantissa.partition('.')
    significand = 0
    exponent = 0
    overflow = False
    for digit in whole:
        candidate = significand * 10 + int(digit)
        if overflow or candidate > (1 << 64) - 1:
            overflow = True
            exponent += 1
        else:
            significand = candidate
    for digit in fraction:
        candidate = significand * 10 + int(digit)
        if candidate > (1 << 64) - 1:
            break
        significand = candidate
        exponent -= 1
    if explicit:
        exp_digits = explicit.lstrip('+-').lstrip('0') or '0'
        exp_value = int(exp_digits) if len(exp_digits) <= 10 else (1 << 31)
        if exp_value > (1 << 31) - 1:
            if significand and not explicit.startswith('-'):
                raise ValueError('nonfinite number')
            return -0.0 if negative else 0.0
        exponent += -exp_value if explicit.startswith('-') else exp_value
    result = float(significand)
    while result and exponent < -308:
        result /= 1e308
        exponent += 308
    if result:
        if exponent > 308:
            raise ValueError('nonfinite number')
        power = float('1e' + str(abs(exponent)))
        result = result * power if exponent >= 0 else result / power
    if not math.isfinite(result):
        raise ValueError('nonfinite number')
    return -result if negative else result


def _int(value):
    if value == '-0':
        return -0.0
    if len(value.lstrip('-')) <= 20:
        parsed = int(value)
        if -(1 << 63) <= parsed <= (1 << 64) - 1:
            return parsed
    return _float(value)


class Peer:
    def __init__(self, command: list[str], cwd: str | Path, *,
                 max_frame_bytes=65536, max_frames=4096, max_bytes=4194304, cancelled=None):
        if os.name != 'posix':
            raise TransportError('unsupported_platform')
        if (not isinstance(command, list) or not command
                or any(not isinstance(arg, str) or '\x00' in arg for arg in command)
                or any(type(n) is not int or n <= 0
                       for n in (max_frame_bytes, max_frames, max_bytes))):
            raise TransportError('invalid_config')
        self._max_frame = max_frame_bytes
        self._max_frames = max_frames
        self._max_bytes = max_bytes
        self._cancelled = cancelled
        if cancelled is not None and cancelled():
            raise TransportError('interrupted')
        self._bytes = 0
        self._frames = 0
        self._stderr_bytes = 0
        self._pending = deque()
        self._partial = bytearray()
        self._failure = None
        self._closed = None
        self._selector = selectors.DefaultSelector()
        try:
            self._process = subprocess.Popen(command, cwd=cwd, stdin=subprocess.PIPE,
                                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                             bufsize=0, start_new_session=True, shell=False)
        except (OSError, ValueError, TypeError):
            self._selector.close()
            raise TransportError('spawn_failed') from None
        # Popen guarantees these streams for PIPE; narrow Optional typing too.
        assert self._process.stdin is not None
        assert self._process.stdout is not None
        assert self._process.stderr is not None
        self._stdin = self._process.stdin
        self._stdout = self._process.stdout
        self._stderr = self._process.stderr
        try:
            for stream in (self._stdin, self._stdout, self._stderr):
                os.set_blocking(stream.fileno(), False)
            self._selector.register(self._stdout, selectors.EVENT_READ, 'stdout')
            self._selector.register(self._stderr, selectors.EVENT_READ, 'stderr')
        except (OSError, ValueError):
            self.close()
            raise TransportError('io_error') from None

    @property
    def pid(self) -> int:
        return self._process.pid

    @property
    def stderr_bytes(self) -> int:
        return self._stderr_bytes

    def _check(self, deadline):
        if self._cancelled is not None and self._cancelled():
            raise TransportError('interrupted')
        if self._closed is not None:
            raise TransportError('closed')
        if self._failure is not None:
            raise TransportError(self._failure)
        if (type(deadline) not in (int, float) or not math.isfinite(deadline)):
            raise TransportError('invalid_deadline')
        if time.monotonic() >= deadline:
            raise TransportError('timeout')

    def _ingest(self, data, channel):
        self._bytes += len(data)
        if channel == 'stderr':
            self._stderr_bytes += len(data)
        if self._bytes > self._max_bytes:
            raise TransportError('traffic_limit')
        if channel == 'stderr':
            return  # Never retain or print child diagnostics.
        self._partial.extend(data)
        while True:
            end = self._partial.find(b'\n')
            if end < 0:
                if len(self._partial) >= self._max_frame:
                    raise TransportError('frame_limit')
                return
            if end + 1 > self._max_frame:
                raise TransportError('frame_limit')
            self._frames += 1
            if self._frames > self._max_frames:
                raise TransportError('traffic_limit')
            self._pending.append(bytes(self._partial[:end]))
            del self._partial[:end + 1]

    def _pump(self, deadline, outgoing=None):
        """One fair, bounded read/write turn; no worker threads or blocking IO."""
        self._check(deadline)
        sent = 0
        try:
            events = self._selector.select(max(0, min(.05, deadline - time.monotonic())))
            # Drain BOTH output channels before trying another bounded write.
            for key, _mask in events:
                if key.data == 'stdin':
                    continue
                try:
                    data = os.read(key.fd, 16384)
                except BlockingIOError:
                    continue
                if data:
                    self._ingest(data, key.data)
                else:
                    self._selector.unregister(key.fileobj)
                    (self._stdout if key.data == 'stdout' else self._stderr).close()
            for key, _mask in events:
                if key.data == 'stdin' and outgoing is not None:
                    try:
                        sent = os.write(key.fd, outgoing[:16384])
                    except BlockingIOError:
                        pass
            return sent
        except TransportError as exc:
            self._failure = exc.code
            raise
        except (BrokenPipeError, ConnectionResetError):
            self._failure = 'eof'
            raise TransportError('eof') from None
        except (OSError, ValueError):
            self._failure = 'io_error'
            raise TransportError('io_error') from None

    def send(self, message: dict, deadline: float) -> None:
        self._check(deadline)
        if not isinstance(message, dict):
            raise TransportError('malformed')
        try:
            data = bytearray()
            encoder = json.JSONEncoder(ensure_ascii=True, allow_nan=False, separators=(',', ':'))
            for piece in encoder.iterencode(message):
                self._check(deadline)
                data.extend(piece.encode('ascii'))
                if len(data) + 1 > self._max_frame:
                    raise TransportError('frame_limit')
            data.extend(b'\n')
        except (TypeError, ValueError, OverflowError, RecursionError):
            raise TransportError('malformed') from None
        if self._stdin.closed:
            raise TransportError('eof')
        self._selector.register(self._stdin, selectors.EVENT_WRITE, 'stdin')
        view = memoryview(data)
        try:
            while view:
                view = view[self._pump(deadline, view):]
        except TransportError as exc:
            # A partially sent frame cannot be retried safely.
            self._failure = exc.code
            raise
        finally:
            self._selector.unregister(self._stdin)

    def receive(self, deadline: float) -> dict:
        self._check(deadline)
        while not self._pending:
            if self._stdout.closed:
                code = 'malformed' if self._partial else 'eof'
                self._failure = code
                raise TransportError(code)
            self._pump(deadline)
        raw = self._pending.popleft()
        try:
            result = json.loads(raw.decode('utf-8'), object_pairs_hook=_pairs,
                                parse_constant=_constant, parse_float=_float, parse_int=_int)
            if not isinstance(result, dict):
                raise ValueError('object required')
            return result
        except (ValueError, UnicodeError, RecursionError):
            self._failure = 'malformed'
            raise TransportError('malformed') from None

    def finish(self, deadline: float) -> int:
        """After the last expected acknowledgement, require silent exit + EOF.

        Closes stdin. Any queued/new stdout, including an unfinished frame,
        fails. Descendants holding pipes are subject to the same deadline.
        """
        self._check(deadline)
        self._stdin.close()
        while True:
            if self._pending or self._partial:
                self._failure = 'unexpected_output'
                raise TransportError('unexpected_output')
            status = self._process.poll()
            if self._stdout.closed and self._stderr.closed and status is not None:
                return status
            self._pump(deadline)

    def close(self) -> dict:
        """Idempotent bounded cleanup of only our session's original group.

        Escaped/re-sessioned descendants are outside this transport's scope.
        Kill the group even if its leader has already exited; do not await an
        orphan's inherited pipe EOF without a deadline. No sandbox is implied.
        """
        if self._closed is not None:
            return dict(self._closed)
        forced = False
        cleanup_error = False
        try:
            os.killpg(self.pid, signal.SIGKILL)
            forced = True
        except ProcessLookupError:
            pass
        except OSError:
            cleanup_error = True
        self._stdin.close()
        end = time.monotonic() + .75
        # Bypass protocol parsing/limits, but retain byte accounting on cleanup.
        while time.monotonic() < end and self._selector.get_map():
            try:
                events = self._selector.select(min(.05, max(0, end - time.monotonic())))
                for key, _mask in events:
                    if key.data == 'stdin':
                        self._selector.unregister(key.fileobj)
                        continue
                    try:
                        data = os.read(key.fd, 16384)
                    except BlockingIOError:
                        continue
                    if data:
                        self._bytes += len(data)
                        if key.data == 'stderr':
                            self._stderr_bytes += len(data)
                    else:
                        self._selector.unregister(key.fileobj)
                        (self._stdout if key.data == 'stdout' else self._stderr).close()
            except (OSError, ValueError):
                cleanup_error = True
                break
        for stream in (self._stdin, self._stdout, self._stderr):
            stream.close()
        self._selector.close()
        try:
            self._process.wait(timeout=max(.01, end - time.monotonic()))
        except subprocess.TimeoutExpired:
            cleanup_error = True
        self._pending.clear()
        self._partial.clear()
        self._closed = {'reaped': self._process.returncode is not None,
                        'forced': forced}
        if cleanup_error:
            self._closed['cleanup_error'] = True
        return dict(self._closed)
