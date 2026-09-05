import ast
import contextlib
import importlib.util
import io
import os
import pty
import select
import tempfile
import threading
import unittest
from pathlib import Path


HARNESS_PATH = Path(__file__).with_name("showcase_pty_smoke.py")
SPEC = importlib.util.spec_from_file_location(
    "showcase_pty_smoke",
    HARNESS_PATH,
)
assert SPEC is not None and SPEC.loader is not None
HARNESS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HARNESS)


class PropertyAssertionTests(unittest.TestCase):
    def test_property_match_does_not_accept_a_digit_from_another_value(self) -> None:
        line = "Live events received:   0 · Retained live history: 0/16"

        self.assertFalse(
            HARNESS.property_has_value(line, "Live events received:", "1")
        )

    def test_property_match_accepts_the_value_cell_prefix(self) -> None:
        line = "Last live adapter:      live-a"

        self.assertTrue(HARNESS.property_has_value(line, "Last live adapter:", "live-a"))


class SessionHostReadinessTests(unittest.TestCase):
    def test_session_browser_title_does_not_satisfy_host_readiness(self) -> None:
        self.assertFalse(HARNESS.session_host_is_ready("Interactive Sessions\nInteractive fixture"))

    def test_empty_session_host_satisfies_host_readiness(self) -> None:
        self.assertTrue(HARNESS.session_host_is_ready("Interactive Session\n(session output pending)"))


class SessionBrowserReadinessTests(unittest.TestCase):
    def test_browser_requires_provider_declared_session_metadata(self) -> None:
        self.assertFalse(HARNESS.session_browser_is_ready("Interactive Sessions"))

    def test_browser_rejects_a_screen_that_still_contains_the_active_host_title(self) -> None:
        self.assertFalse(
            HARNESS.session_browser_is_ready(
                "Interactive Sessions\nInteractive fixture\nInteractive Session · Close requested"
            )
        )

    def test_browser_accepts_provider_declared_session_without_an_active_host(self) -> None:
        self.assertTrue(HARNESS.session_browser_is_ready("Interactive Sessions\nInteractive fixture"))


class SessionFixtureMarkerTests(unittest.TestCase):
    def test_marker_represents_only_current_provider_sessions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            control = Path(directory)

            self.assertEqual(HARNESS.session_marker_entries(control), [])
            HARNESS.session_marker(control).write_text("fixture-session\n", encoding="utf-8")
            self.assertEqual(HARNESS.session_marker_entries(control), ["fixture-session"])
            HARNESS.reset_session_markers(control)
            self.assertEqual(HARNESS.session_marker_entries(control), [])


class CurrentScreenAssertionTests(unittest.TestCase):
    def test_prior_render_does_not_satisfy_a_current_screen_assertion(self) -> None:
        output = bytearray(
            b"\x1b[2J\x1b[HInteractive Sessions\x1b[2J\x1b[HInteractive Session"
        )

        self.assertIn("Interactive Sessions", HARNESS.rendered_text(output))
        self.assertFalse(HARNESS.current_screen_contains(output, "Interactive Sessions"))


class MarkerWaitDrainTests(unittest.TestCase):
    def test_full_pty_does_not_block_marker_publication(self) -> None:
        cases = (
            ("file", "ready", "ready\n", None),
            ("action", "actions", "fixture.action.alpha\n", ["fixture.action.alpha"]),
            ("session-open", "sessions", "fixture-session\n", ["fixture-session"]),
            ("session-close", "sessions", "", []),
        )
        for name, marker_name, contents, expected in cases:
            with self.subTest(wait=name), tempfile.TemporaryDirectory() as directory:
                control = Path(directory)
                marker = control / marker_name
                if name == "session-close":
                    marker.write_text("fixture-session\n", encoding="utf-8")
                master, slave = pty.openpty()
                os.set_blocking(master, False)
                os.set_blocking(slave, False)
                output = bytearray()
                full = threading.Event()
                stop = threading.Event()
                errors = []
                filled = []

                def produce() -> None:
                    try:
                        # Observe EAGAIN, rather than guessing the PTY capacity
                        # or sleeping until a producer is presumed blocked.
                        count = 0
                        while not stop.is_set():
                            try:
                                count += os.write(slave, b"x" * 4096)
                            except BlockingIOError:
                                break
                            if count >= 8 * 1024 * 1024:
                                raise AssertionError("PTY did not apply bounded backpressure")
                        filled.append(count)
                        full.set()
                        # Publication requires additional terminal writes AFTER
                        # the observed full buffer, just like a blocked UI.
                        remaining = memoryview(b"y" * (256 * 1024))
                        while remaining and not stop.is_set():
                            _, writable, _ = select.select([], [slave], [], 0.05)
                            if writable:
                                try:
                                    remaining = remaining[os.write(slave, remaining):]
                                except BlockingIOError:
                                    pass
                        if not stop.is_set():
                            marker.write_text(contents, encoding="utf-8")
                    except BaseException as error:
                        errors.append(error)
                        full.set()

                producer = threading.Thread(target=produce, daemon=True)
                producer.start()
                try:
                    self.assertTrue(full.wait(2.0), "producer did not reach PTY backpressure")
                    self.assertEqual(errors, [])
                    self.assertGreater(filled[0], 0)
                    if name == "file":
                        HARNESS.wait_for_file(
                            marker, 1.0, "blocked ready marker",
                            master=master, output=output,
                        )
                    else:
                        wait = (
                            HARNESS.wait_for_action_invocations
                            if name == "action"
                            else HARNESS.wait_for_session_marker_entries
                        )
                        wait(
                            control, expected, 1.0, "blocked fixture marker",
                            master=master, output=output,
                        )
                    producer.join(2.0)
                    self.assertFalse(producer.is_alive(), "producer did not finish")
                    self.assertEqual(errors, [])
                    self.assertEqual(marker.read_text(encoding="utf-8"), contents)
                    self.assertGreater(len(output), filled[0])
                    self.assertEqual(output[:filled[0]], b"x" * filled[0])
                    self.assertIn(b"y", output)
                finally:
                    stop.set()
                    producer.join(2.0)
                    os.close(slave)
                    os.close(master)
                    self.assertFalse(producer.is_alive(), "producer cleanup timed out")

    def test_marker_waits_preserve_non_pty_callers_and_timeout_diagnostics(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            control = Path(directory)
            (control / "ready").write_text("ready\n", encoding="utf-8")
            (control / "actions").write_text("alpha\n", encoding="utf-8")
            HARNESS.session_marker(control).write_text("session\n", encoding="utf-8")
            HARNESS.wait_for_file(control / "ready", 1.0, "ready missing")
            HARNESS.wait_for_action_invocations(control, ["alpha"], 1.0, "action missing")
            HARNESS.wait_for_session_marker_entries(control, ["session"], 1.0, "session missing")
            with self.assertRaisesRegex(RuntimeError, "ready missing"):
                HARNESS.wait_for_file(control / "missing", 0.01, "ready missing")
            for wait in (HARNESS.wait_for_action_invocations, HARNESS.wait_for_session_marker_entries):
                with self.subTest(wait=wait.__name__), self.assertRaisesRegex(
                    RuntimeError, "marker mismatch; got .*expected \\['other'\\]"
                ):
                    wait(control, ["other"], 0.01, "marker mismatch")

    def test_all_main_marker_waits_supply_the_live_terminal(self) -> None:
        tree = ast.parse(HARNESS_PATH.read_text(encoding="utf-8"))
        main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "main")
        names = {"wait_for_file", "wait_for_action_invocations", "wait_for_session_marker_entries"}
        calls = [
            (node.func.id, node) for node in ast.walk(main)
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id in names
        ]
        self.assertEqual({name for name, _ in calls}, names)
        for wait_name, call in calls:
            with self.subTest(wait=wait_name, line=call.lineno):
                keywords = {item.arg: item.value for item in call.keywords}
                for name in ("master", "output"):
                    self.assertIn(name, keywords)
                    value = keywords[name]
                    assert isinstance(value, ast.Name)
                    self.assertEqual(value.id, name)


class FailureDiagnosticTests(unittest.TestCase):
    def test_main_failure_handler_prints_current_screen_and_preserves_error(self) -> None:
        tree = ast.parse(HARNESS_PATH.read_text(encoding="utf-8"))
        main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "main")
        protected = next(node for node in main.body if isinstance(node, ast.Try))
        # Execute the actual handler without launching the acceptance harness.
        probe = ast.Module(body=[ast.Try(
            body=ast.parse('raise RuntimeError("original marker failure")').body,
            handlers=protected.handlers, orelse=[], finalbody=[],
        )], type_ignores=[])
        stderr = io.StringIO()
        namespace = dict(vars(HARNESS), output=bytearray(b"\x1b[2J\x1b[Hold\x1b[2J\x1b[Hcurrent"))
        with contextlib.redirect_stderr(stderr), self.assertRaisesRegex(RuntimeError, "original marker failure"):
            exec(compile(ast.fix_missing_locations(probe), str(HARNESS_PATH), "exec"), namespace)
        self.assertIn("PTY failure current screen:\ncurrent", stderr.getvalue())
        self.assertNotIn("old", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
