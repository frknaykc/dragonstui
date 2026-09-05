import importlib.util
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "showcase_pty_smoke",
    Path(__file__).with_name("showcase_pty_smoke.py"),
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


class CurrentScreenAssertionTests(unittest.TestCase):
    def test_prior_render_does_not_satisfy_a_current_screen_assertion(self) -> None:
        output = bytearray(
            b"\x1b[2J\x1b[HInteractive Sessions\x1b[2J\x1b[HInteractive Session"
        )

        self.assertIn("Interactive Sessions", HARNESS.rendered_text(output))
        self.assertFalse(HARNESS.current_screen_contains(output, "Interactive Sessions"))


if __name__ == "__main__":
    unittest.main()
