"""Real, isolated unit-fixture peers; these are not adapter conformance results."""
import os
from pathlib import Path
import signal
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

from adapter_conformance_transport import Peer, TransportError


@unittest.skipUnless(os.name == "posix", "POSIX process isolation")
class TransportTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def peer(self, source, **limits):
        peer = Peer([sys.executable, "-u", "-c", source], Path(self.tmp.name), **limits)
        self.addCleanup(peer.close)
        return peer

    def deadline(self):
        return time.monotonic() + 3

    def error(self, code, action):
        with self.assertRaises(TransportError) as caught:
            action()
        self.assertEqual(caught.exception.code, code)
        self.assertEqual(str(caught.exception), code)

    def test_round_trip_and_finish(self):
        p = self.peer("import sys,json\nfor line in sys.stdin:\n m=json.loads(line); print(json.dumps({'id':m['id']})); break")
        p.send({'id': 'request-1'}, self.deadline())
        self.assertEqual(p.receive(self.deadline()), {'id': 'request-1'})
        self.assertEqual(p.finish(self.deadline()), 0)
        result = p.close()
        self.assertTrue(result['reaped'])
        self.assertFalse(result['forced'])
        self.assertEqual(p.close(), result)

    def test_invalid_json_and_non_objects(self):
        for raw in [b'not-json\n', b'{"a":NaN}\n',
                    b'{"a":Infinity}\n', b'{"a":1e999}\n', b'true\n', b'[]\n', b'\xff\n']:
            with self.subTest(raw=raw):
                p = self.peer(f'import os; os.write(1,{raw!r})')
                self.error('malformed', lambda: p.receive(self.deadline()))
                p.close()

    def test_duplicate_keys_retain_typed_evidence_and_opaque_last_value(self):
        p = self.peer('import os; os.write(1,b\'{"id":"a","id":"b","payload":{"x":1,"x":2}}\\n\')')
        message = p.receive(self.deadline())
        self.assertEqual(getattr(message, "duplicate_keys"), {"id"})
        self.assertEqual(message["payload"], {"x": 2})
        self.assertEqual(message["payload"].duplicate_keys, {"x"})

    def test_frame_limit_with_and_without_newline(self):
        for raw in [b'{' + b' ' * 64, b'{"x":"' + b'x' * 64 + b'"}\n']:
            with self.subTest(raw=raw):
                p = self.peer(f'import os; os.write(1,{raw!r})', max_frame_bytes=32)
                self.error('frame_limit', lambda: p.receive(self.deadline()))
                p.close()

    def test_partial_at_eof_and_no_newline_timeout(self):
        p = self.peer('import os; os.write(1,b"{}")')
        self.error('malformed', lambda: p.receive(self.deadline()))
        p = self.peer('import os,time; os.write(1,b"{}"); time.sleep(30)')
        self.error('timeout', lambda: p.receive(time.monotonic() + .2))

    def test_empty_eof(self):
        p = self.peer('pass')
        self.error('eof', lambda: p.receive(self.deadline()))

    def test_stderr_pressure_does_not_deadlock(self):
        p = self.peer('import os; os.write(2,b"x"*200000); os.write(1,b"{}\\n")')
        self.assertEqual(p.receive(self.deadline()), {})
        self.assertEqual(p.finish(self.deadline()), 0)
        self.assertEqual(p.stderr_bytes, 200000)

    def test_pending_stdout_and_stderr_during_blocked_write(self):
        p = self.peer('import sys,os,json\nfor i in range(80):\n os.write(2,b"s"*4096); print(json.dumps({"i":i}))\nm=json.loads(sys.stdin.readline()); print(json.dumps({"size":len(m["body"])}))',
                      max_frame_bytes=600000)
        p.send({'body': 'x' * 500000}, self.deadline())
        for i in range(80):
            self.assertEqual(p.receive(self.deadline()), {'i': i})
        self.assertEqual(p.receive(self.deadline()), {'size': 500000})
        self.assertEqual(p.finish(self.deadline()), 0)
        self.assertEqual(p.stderr_bytes, 80 * 4096)

    def test_global_frame_limit_not_reset_by_receive(self):
        p = self.peer('import sys\nfor line in sys.stdin: print("{}")', max_frames=2)
        for _ in range(2):
            p.send({}, self.deadline())
            self.assertEqual(p.receive(self.deadline()), {})
        p.send({}, self.deadline())
        self.error('traffic_limit', lambda: p.receive(self.deadline()))

    def test_pending_frames_are_bounded_during_send(self):
        p = self.peer('import os,time; os.write(1,b"{}\\n"*100); time.sleep(30)',
                      max_frames=4, max_frame_bytes=2000000)
        self.error('traffic_limit', lambda: p.send({'x': 'x' * 1000000}, self.deadline()))
        # Once a partial write fails, no retry/receive can bypass the failure.
        self.error('traffic_limit', lambda: p.receive(self.deadline()))

    def test_global_byte_limit_not_reset_between_reads(self):
        p = self.peer('import sys\nfor line in sys.stdin: print("{}")', max_bytes=5)
        p.send({}, self.deadline())
        self.assertEqual(p.receive(self.deadline()), {})
        p.send({}, self.deadline())
        self.error('traffic_limit', lambda: p.receive(self.deadline()))

    def test_finish_rejects_output_arriving_after_ack(self):
        p = self.peer('import sys; print("{}"); sys.stdin.read(); print("{}")')
        self.assertEqual(p.receive(self.deadline()), {})
        self.error('unexpected_output', lambda: p.finish(self.deadline()))

    def test_exit_status_and_sibling_group_isolation(self):
        sibling = self.peer('import time; time.sleep(30)')
        p = self.peer('import sys; sys.exit(7)')
        self.assertEqual(p.finish(self.deadline()), 7)
        self.assertTrue(p.close()['reaped'])
        os.kill(sibling.pid, 0)
        self.assertEqual(os.getpgid(sibling.pid), sibling.pid)

    def test_byte_limit_includes_stderr(self):
        p = self.peer('import os; os.write(2,b"x"*2000)', max_bytes=1000)
        self.error('traffic_limit', lambda: p.receive(self.deadline()))
        self.assertGreater(p.stderr_bytes, 1000)

    def test_never_reading_child_write_timeout_and_cleanup(self):
        p = self.peer('import time; time.sleep(30)', max_frame_bytes=2000000)
        self.error('timeout', lambda: p.send({'x': 'x' * 1000000}, time.monotonic() + .2))
        start = time.monotonic()
        result = p.close()
        self.assertTrue(result['reaped'])
        self.assertTrue(result['forced'])
        self.assertLess(time.monotonic() - start, 2)
        with self.assertRaises(ProcessLookupError):
            os.kill(p.pid, 0)

    def test_finish_rejects_extra_queued_or_partial_output(self):
        for extra in [b'{}\n', b'partial']:
            with self.subTest(extra=extra):
                p = self.peer(f'import os; os.write(1,b"{{}}\\n"+{extra!r})')
                self.assertEqual(p.receive(self.deadline()), {})
                self.error('unexpected_output', lambda: p.finish(self.deadline()))
                p.close()

    def test_finish_requires_eof_and_child_exit(self):
        p = self.peer('import os,time; os.write(1,b"{}\\n"); os.close(1); os.close(2); time.sleep(30)')
        self.assertEqual(p.receive(self.deadline()), {})
        self.error('timeout', lambda: p.finish(time.monotonic() + .2))
        self.assertTrue(p.close()['reaped'])

    @unittest.skipUnless(hasattr(os, 'fork'), 'requires fork')
    def test_descendant_inherited_pipes_cleanup_after_leader_exit(self):
        p = self.peer('import os,time,json\npid=os.fork()\nif pid==0:\n time.sleep(30); os._exit(0)\nprint(json.dumps({"descendant":pid})); os._exit(0)')
        child = p.receive(self.deadline())['descendant']
        self.assertEqual(os.getpgid(child), p.pid)
        self.error('timeout', lambda: p.finish(time.monotonic() + .2))
        start = time.monotonic()
        result = p.close()
        self.assertTrue(result['reaped'])
        self.assertTrue(result['forced'])
        self.assertLess(time.monotonic() - start, 2)
        # ESRCH or a transient reparented zombie is valid; a live sleeper is not.
        import subprocess
        state = subprocess.run(['ps', '-o', 'stat=', '-p', str(child)], capture_output=True, text=True)
        self.assertTrue(not state.stdout.strip() or state.stdout.strip().startswith('Z'), state.stdout)

    def test_configuration_and_outgoing_validation(self):
        for limits in [{'max_frames': True}, {'max_bytes': 0}, {'max_frame_bytes': -1}]:
            with self.subTest(limits=limits):
                self.error('invalid_config', lambda: self.peer('pass', **limits))
        p = self.peer('import time; time.sleep(30)')
        self.error('malformed', lambda: p.send({'a': float('nan')}, self.deadline()))
        self.error('malformed', lambda: p.send(True, self.deadline()))
        self.error('invalid_deadline', lambda: p.receive(float('nan')))

    def test_unsupported_platform_checked_before_spawn(self):
        with patch('adapter_conformance_transport.os.name', 'nt'), patch('adapter_conformance_transport.subprocess.Popen') as spawn:
            self.error('unsupported_platform', lambda: Peer(['unused'], self.tmp.name))
            spawn.assert_not_called()


if __name__ == '__main__':
    unittest.main()
