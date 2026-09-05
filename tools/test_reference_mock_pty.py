import os
from pathlib import Path
import pty
import signal
import subprocess
import sys
import tempfile
import time
import unittest

from reference_mock_pty_smoke import assert_ansi_restored, session_terminal_evidence, stop_showcase


class SessionTerminalEvidenceTests(unittest.TestCase):
    def test_explicit_expected_exit_is_typed_and_wrong_codes_fail(self):
        self.assertEqual(session_terminal_evidence("Interactive session exited with code 2"),
                         {"kind": "exit_code", "exit_code": 2})
        for code in ("0", "20", "21", "-2"):
            with self.subTest(code=code), self.assertRaises(RuntimeError):
                session_terminal_evidence(f"Interactive session exited with code {code}")

    def test_authoritative_inactivity_never_invents_an_exit_code(self):
        self.assertEqual(session_terminal_evidence("Interactive session is no longer active"),
                         {"kind": "authoritative_inactivity", "exit_code": None})

    def test_browser_or_disconnect_alone_is_not_release_evidence(self):
        for text in ("Interactive Sessions", "Interactive fixture",
                     "Interactive session disconnected", "Interactive session exited"):
            self.assertIsNone(session_terminal_evidence(text))


class AnsiRestorationTests(unittest.TestCase):
    @staticmethod
    def restored():
        return bytearray(b"\x1b[?1049;1000;1002;1003;1006;1015h\x1b[?25l"
                         b"\x1b[?1049;1000;1002;1003;1006;1015l\x1b[?25h")

    def test_final_restored_state_accepts_combined_private_modes(self):
        assert_ansi_restored(self.restored())

    def test_later_regression_of_each_mode_is_rejected(self):
        for mode in (1049, 25, 1000, 1002, 1003, 1006, 1015):
            with self.subTest(mode=mode):
                bad = self.restored() + f"\x1b[?{mode}{'l' if mode == 25 else 'h'}".encode()
                with self.assertRaisesRegex(RuntimeError, f"terminal mode {mode}"):
                    assert_ansi_restored(bad)

    def test_missing_lifecycle_is_rejected(self):
        with self.assertRaises(RuntimeError):
            assert_ansi_restored(bytearray())
        with self.assertRaises(RuntimeError):
            assert_ansi_restored(bytearray(b"\x1b[?1049l"))


@unittest.skipUnless(os.name == "posix", "POSIX process/PTY cleanup")
class ReferenceCleanupTests(unittest.TestCase):
    def test_no_process_and_already_exited_process_are_safe(self):
        stop_showcase(None, None, bytearray())
        child = subprocess.Popen([sys.executable, "-c", "pass"], start_new_session=True)
        child.wait(timeout=3)
        stop_showcase(child, None, bytearray())
        self.assertEqual(child.returncode, 0)

    def test_live_process_is_terminated_and_reaped(self):
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"], start_new_session=True)
        try:
            stop_showcase(child, None, bytearray())
            self.assertEqual(child.returncode, -signal.SIGTERM)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=3)

    def test_full_pty_and_ignored_sigterm_are_drained_and_force_reaped(self):
        with tempfile.TemporaryDirectory() as tmp:
            marker = Path(tmp) / "full"
            master, slave = pty.openpty()
            os.set_blocking(master, False)
            child = subprocess.Popen([
                sys.executable, "-c",
                "import os,signal,sys,time\n"
                "from pathlib import Path\n"
                "signal.signal(signal.SIGTERM, signal.SIG_IGN)\n"
                "os.set_blocking(1, False)\n"
                "while True:\n"
                "    try: os.write(1, b'x' * 4096)\n"
                "    except BlockingIOError: break\n"
                "Path(sys.argv[1]).write_text('full')\n"
                "time.sleep(30)\n",
                str(marker),
            ], stdin=subprocess.DEVNULL, stdout=slave, stderr=slave, start_new_session=True)
            os.close(slave)
            output = bytearray()
            try:
                deadline = time.monotonic() + 3
                while not marker.exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue(marker.exists(), "producer must hit real PTY backpressure")
                stop_showcase(child, master, output)
                self.assertEqual(child.returncode, -signal.SIGKILL)
                self.assertGreater(len(output), 0)
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=3)
                os.close(master)


if __name__ == "__main__":
    unittest.main()
