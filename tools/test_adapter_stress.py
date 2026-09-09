import os
from pathlib import Path
import tempfile
import time
import unittest

from adapter_stress import cpu_seconds, parse_ps, run_case, summarize_samples


class ResourceParsingTests(unittest.TestCase):
    def test_cpu_clocks_cover_macos_linux_and_days(self):
        self.assertAlmostEqual(cpu_seconds("01:02.34"), 62.34)
        self.assertEqual(cpu_seconds("02:03:04"), 7384)
        self.assertEqual(cpu_seconds("1-02:03:04"), 93784)
        with self.assertRaises(ValueError):
            cpu_seconds("42")

    def test_rss_is_kib_and_cpu_is_cumulative(self):
        self.assertEqual(parse_ps("  17 2048 00:01.25 S\n"),
                         {17: {"rss_bytes": 2097152, "cpu_seconds": 1.25}})
        self.assertEqual(parse_ps(""), {})

    def test_zombie_cpu_reset_is_not_a_live_resource_sample(self):
        self.assertEqual(parse_ps("17 0 0:00.00 Z\n18 2048 0:01.25 R+\n"),
                         {18: {"rss_bytes": 2097152, "cpu_seconds": 1.25}})

    def test_complete_samples_keep_host_and_provider_cost_separate(self):
        rows = lambda a, b: {1: {"rss_bytes": a, "cpu_seconds": a / 100},
                             2: {"rss_bytes": b, "cpu_seconds": b / 100}}
        summary = summarize_samples([(0, rows(100, 200)), (1, {1: {}}),
                                     (2, rows(300, 400))], [1, 2])
        self.assertEqual(summary["complete_samples"], 2)
        self.assertEqual(summary["host"]["cpu_percent_one_core"], 100)
        self.assertEqual(summary["adapters"]["sampled_peak_rss_bytes"], 400)
        self.assertEqual(summary["host"]["first_rss_bytes"], 100)

    def test_missing_samples_fail_instead_of_fabricating_zero(self):
        with self.assertRaisesRegex(RuntimeError, "fewer than two"):
            summarize_samples([], [1, 2])


@unittest.skipUnless(os.name == "posix", "isolated POSIX process groups")
class RunnerFailureTests(unittest.TestCase):
    def test_deadline_terminates_owned_process_and_preserves_error(self):
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / "pid"
            fixture = Path(directory) / "fixture"
            fixture.write_text("#!/usr/bin/env python3\nimport os,time\n"
                               f"open({str(pid_file)!r}, 'w').write(str(os.getpid()))\n"
                               "print('held fixture', flush=True)\ntime.sleep(60)\n")
            fixture.chmod(0o755)
            started = time.monotonic()
            with self.assertRaisesRegex(RuntimeError, "exceeded.*held fixture") as caught:
                # Match a multiline retained diagnostic below instead of discarding it.
                try:
                    run_case(fixture, 1, 1, 1)
                except RuntimeError as error:
                    raise RuntimeError(str(error).replace("\n", " ")) from error
            self.assertIn("held fixture", str(caught.exception))
            self.assertLess(time.monotonic() - started, 6)
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid_file.read_text()), 0)

    def test_nonzero_exit_and_missing_scenarios_are_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory) / "fixture"
            for code, expected in [(3, "Rust stress test failed"), (0, "missing fast/held")]:
                with self.subTest(code=code):
                    fixture.write_text(f"#!/bin/sh\nexit {code}\n")
                    fixture.chmod(0o755)
                    with self.assertRaisesRegex(RuntimeError, expected):
                        run_case(fixture, 1, 1, 2)


if __name__ == "__main__":
    unittest.main()
