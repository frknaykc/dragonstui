import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

from adapter_conformance import Suite

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "tools/adapter_conformance.py"
FIXTURE = ROOT / "tools/fixtures/conformance_adapter.py"


def requests_profile():
    return {"schema_version": 1, "requests": [
        {"name": "first", "operation": "sample.echo", "payload": {"v": 1}, "expect": "response", "response_payload": {"v": 1}},
        {"name": "second", "operation": "sample.echo", "payload": {"v": True}, "expect": "response", "response_payload": {"v": True}},
    ]}


def session_profile():
    return {"schema_version": 1, "session": {"capability": "sample.session", "rows": 24, "columns": 80,
            "input": "hello", "expect_output_contains": "echo:hello", "resize": {"rows": 30, "columns": 100},
            "exit_code": None}}


@unittest.skipUnless(os.name == "posix", "POSIX conformance peer")
class ConformanceCliTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="m67-cli-test-")
        self.root = Path(self.temp.name)
        self.counter = 0
        self.markers = []

    def tearDown(self):
        # Rescue only PIDs recorded by this task's fixture if a regression timed out.
        for marker in self.markers:
            if marker.exists():
                for value in marker.read_text().splitlines():
                    try:
                        os.killpg(int(value), signal.SIGKILL)
                    except ProcessLookupError:
                        pass
        self.temp.cleanup()

    def run_case(self, mode="minimal", profile=None, raw_profile=None, options=()):
        self.counter += 1
        marker = self.root / f"launch-{self.counter}"
        self.markers.append(marker)
        args = [sys.executable, str(CLI), "--expect-id", "sample", "--timeout-ms", "500", *options]
        if profile is not None or raw_profile is not None:
            path = self.root / f"profile-{self.counter}.json"
            path.write_text(raw_profile if raw_profile is not None else json.dumps(profile))
            args.extend(["--profile", str(path)])
        args.extend(["--", sys.executable, str(FIXTURE), "--mode", mode, "--launch-marker", str(marker)])
        completed = subprocess.run(args, capture_output=True, text=True, timeout=8)
        self.assertEqual(completed.stderr, "", completed.stderr)
        report = json.loads(completed.stdout)
        if marker.exists():
            for value in marker.read_text().splitlines():
                with self.assertRaises(ProcessLookupError, msg="conformance adapter was not reaped"):
                    os.kill(int(value), 0)
        return completed, report, marker

    def test_minimal_provider_needs_no_fixture_capabilities_and_restarts(self):
        completed, report, marker = self.run_case()
        self.assertEqual(completed.returncode, 0)
        self.assertEqual(report["scope"], "requested_scenarios_only")
        self.assertEqual(report["declared_capabilities"], ["sample.echo"])
        self.assertEqual(len(marker.read_text().splitlines()), 2)
        skipped = {c["name"] for c in report["checks"] if c["status"] == "skipped"}
        self.assertEqual(skipped, {"requests", "session", "events"})
        self.assertTrue(all(c["status"] == "passed" for c in report["checks"] if c["name"].endswith("shutdown")))

    def test_additive_fields_and_global_uncorrelated_error_are_valid(self):
        for mode in ("additive", "global-error"):
            with self.subTest(mode=mode):
                completed, report, _ = self.run_case(mode)
                self.assertEqual(completed.returncode, 0, report)

    def test_request_payloads_and_out_of_order_correlation(self):
        for mode in ("minimal", "out-of-order"):
            with self.subTest(mode=mode):
                completed, report, _ = self.run_case(mode, requests_profile())
                self.assertEqual(completed.returncode, 0, report)
                self.assertEqual(len([c for c in report["checks"] if c["name"].startswith("request.")]), 2)

    def test_boolean_payload_is_not_numeric_payload(self):
        profile = requests_profile()
        profile["requests"][0]["response_payload"] = {"v": True}
        completed, report, _ = self.run_case(profile=profile)
        self.assertEqual(completed.returncode, 1)
        self.assertIn("request_payload_mismatch", [c.get("code") for c in report["checks"]])

    def test_opted_in_session_checks_chunked_output_resize_and_exit(self):
        completed, report, _ = self.run_case("session", session_profile())
        self.assertEqual(completed.returncode, 0, report)
        self.assertIn({"name": "session", "status": "passed"}, report["checks"])

    def test_invalid_wire_and_handshake_metadata_fail(self):
        for mode in ("malformed", "wrong-protocol", "wrong-id", "duplicate-capability", "undeclared-action", "bad-observation"):
            with self.subTest(mode=mode):
                completed, report, marker = self.run_case(mode)
                self.assertEqual(completed.returncode, 1, report)
                self.assertEqual(report["status"], "failed")
                self.assertEqual(len(marker.read_text().splitlines()), 1)

    def test_unknown_and_duplicate_reply_ids_do_not_pass(self):
        for mode in ("wrong-response", "duplicate-response"):
            with self.subTest(mode=mode):
                completed, report, _ = self.run_case(mode, requests_profile())
                self.assertEqual(completed.returncode, 1)
                self.assertIn("uncorrelated_reply", [c.get("code") for c in report["checks"]])

    def test_session_wrong_identity_and_missing_release_fail(self):
        for mode in ("session-wrong-id", "session-no-exit"):
            with self.subTest(mode=mode):
                completed, report, _ = self.run_case(mode, session_profile())
                self.assertEqual(completed.returncode, 1, report)

    def test_timeouts_premature_exit_and_invalid_shutdown_fail(self):
        for mode in ("no-read", "no-shutdown", "premature-exit", "trailing-message", "nonzero-shutdown"):
            with self.subTest(mode=mode):
                completed, report, _ = self.run_case(mode)
                self.assertEqual(completed.returncode, 1, report)
                cleanup = [c for c in report["checks"] if c["name"].endswith("cleanup")]
                self.assertTrue(cleanup and cleanup[0]["reaped"])

    def test_provider_content_is_not_copied_into_report_or_stderr(self):
        completed, _, _ = self.run_case("secret-diagnostic")
        self.assertEqual(completed.returncode, 1)
        self.assertNotIn("PRIVATE_TOKEN_EXAMPLE_DONT_REPORT", completed.stdout + completed.stderr)

    def test_config_errors_never_launch_an_adapter(self):
        invalid = ["{", '{"schema_version":1,"schema_version":1}', '{"schema_version":2}',
                   '{"schema_version":1,"request":[]}', '{"schema_version":true}',
                   '{"schema_version":1,"requests":[{"name":"a","operation":"sample.echo","payload":NaN,"expect":"response"}]}',
                   '{"schema_version":1,"requests":[{"name":"a","operation":"sample.echo","payload":1e999,"expect":"response"}]}']
        for raw in invalid:
            with self.subTest(raw=raw):
                completed, report, marker = self.run_case(raw_profile=raw)
                self.assertEqual(completed.returncode, 2, report)
                self.assertFalse(marker.exists())

    def test_oversized_session_open_is_rejected_before_launch(self):
        profile = session_profile()
        profile["session"]["capability"] = "a." * 33000 + "a"
        completed, report, marker = self.run_case(profile=profile)
        self.assertEqual(completed.returncode, 2)
        self.assertEqual(report["code"], "profile_frame_limit")
        self.assertFalse(marker.exists())

    def test_cleanup_error_cannot_be_reported_as_pass(self):
        # Fault injection at the cleanup/report seam; no executable is launched.
        peer = Mock(stderr_bytes=0)
        peer.close.return_value = {"reaped": True, "forced": False, "cleanup_error": True}
        with patch("adapter_conformance.Peer", return_value=peer), patch.object(Suite, "handshake"), \
             patch.object(Suite, "declarations"), patch.object(Suite, "shutdown"):
            report = Suite(["unused"], {"schema_version": 1}, "sample", None, 1).run()
        self.assertEqual(report["status"], "failed")
        cleanup = next(c for c in report["checks"] if c["name"] == "cleanup")
        self.assertEqual(cleanup["status"], "failed")
        self.assertTrue(cleanup["cleanup_error"])

    def test_profile_cannot_invoke_undeclared_or_unconfirmed_actions(self):
        profile = {"schema_version": 1, "requests": [{"name": "action", "operation": "sample.echo",
                   "action": "sample.action", "payload": {}, "expect": "response"}]}
        for mode in ("minimal", "confirmation"):
            with self.subTest(mode=mode):
                completed, report, _ = self.run_case(mode, profile)
                self.assertEqual(completed.returncode, 1, report)
        profile["requests"][0]["confirmed"] = True
        completed, report, _ = self.run_case("confirmation", profile)
        self.assertEqual(completed.returncode, 0, report)

    def test_unsatisfied_event_expectation_fails_not_skips(self):
        completed, report, _ = self.run_case(profile={"schema_version": 1, "events": {"minimum": 1}})
        self.assertEqual(completed.returncode, 1)
        self.assertTrue(any(c["name"] == "events" and c["status"] == "failed" for c in report["checks"]))

    def test_catchable_signals_report_interruption_and_reap_the_peer(self):
        for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            with self.subTest(signal=signum):
                self.counter += 1
                marker = self.root / f"signal-{self.counter}"
                self.markers.append(marker)
                args = [sys.executable, str(CLI), "--expect-id", "sample", "--timeout-ms", "30000",
                        "--", sys.executable, str(FIXTURE), "--mode", "no-shutdown", "--launch-marker", str(marker)]
                with subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as process:
                    deadline = time.monotonic() + 3
                    while not marker.exists() and time.monotonic() < deadline:
                        time.sleep(.005)
                    self.assertTrue(marker.exists(), "fixture did not start")
                    process.send_signal(signum)
                    stdout, stderr = process.communicate(timeout=5)
                    report = json.loads(stdout)
                    self.assertEqual(stderr, "")
                    self.assertEqual(process.returncode, 128 + signum)
                    self.assertEqual(report["status"], "interrupted")
                    self.assertEqual(report["signal"], signum)
                    for value in marker.read_text().splitlines():
                        with self.assertRaises(ProcessLookupError):
                            os.kill(int(value), 0)

    def test_reference_profile_against_real_rust_provider(self):
        mock = Path(os.environ.get("DRAGONSTUI_CONFORMANCE_MOCK", ROOT / "target/debug/dragonstui-adapter-host-mock"))
        if not mock.is_file():
            self.skipTest("Build dragonstui-adapter-host-mock to exercise the real reference profile")
        args = [sys.executable, str(CLI), "--expect-id", "reference", "--expect-version", "1.0.0",
                "--profile", str(ROOT / "tools/fixtures/reference_conformance.json"), "--", str(mock),
                "--mode", "reference", "--id", "reference"]
        completed = subprocess.run(args, capture_output=True, text=True, timeout=15)
        self.assertEqual(completed.stderr, "")
        self.assertEqual(completed.returncode, 0, completed.stdout)
        report = json.loads(completed.stdout)
        self.assertTrue(all(c["status"] == "passed" for c in report["checks"]))
        self.assertEqual(set(report["observed"]["observation_types"]), {"log", "metric", "status", "event", "error"})
        self.assertGreater(report["observed"]["generic_events"], 0)


if __name__ == "__main__":
    unittest.main()
