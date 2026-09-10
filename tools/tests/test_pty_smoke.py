"""Deterministic deadline-boundary checks for the baseline PTY helper."""

import unittest
from unittest import mock

from tools.acceptance import pty_smoke


class ReadDeadlineTests(unittest.TestCase):
    def test_drain_deadline_crossing_uses_nonnegative_select_timeout(self):
        with (
            mock.patch.object(pty_smoke.time, "monotonic", side_effect=[0, 0.999, 1.001, 1.002]),
            mock.patch.object(pty_smoke.select, "select", return_value=([], [], [])) as select,
        ):
            pty_smoke.drain_for(7, bytearray(), 1)
        select.assert_called_once_with([7], [], [], 0.0)

    def test_positive_timeout_is_preserved(self):
        with mock.patch.object(pty_smoke.select, "select", return_value=([], [], [])) as select:
            pty_smoke.read_available(7, bytearray(), 0.02)
        select.assert_called_once_with([7], [], [], 0.02)


if __name__ == "__main__":
    unittest.main()
