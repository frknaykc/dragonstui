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


if __name__ == "__main__":
    unittest.main()
