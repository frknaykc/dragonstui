"""Focused M73 fixture boundaries; real installation is exercised by the PTY run."""
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import MagicMock, patch

from ecosystem_fixture import install_fixture, query


class EcosystemFixtureTests(unittest.TestCase):
    def test_existing_root_is_preserved_before_any_process(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sentinel = root / "keep"
            sentinel.write_text("unchanged")
            with patch("ecosystem_fixture.subprocess.run") as run:
                with self.assertRaises(FileExistsError):
                    install_fixture(root, Path("unused"), Path("unused"))
                run.assert_not_called()
            self.assertEqual(sentinel.read_text(), "unchanged")

    def test_non_loopback_is_rejected_before_connection(self):
        with patch("ecosystem_fixture.socket.create_connection") as connect:
            with self.assertRaises(ValueError):
                query({"address": "192.0.2.1:1", "token": "test-only"}, {})
            connect.assert_not_called()

    def response(self, body):
        stream = MagicMock()
        stream.__enter__.return_value = stream
        stream.makefile.return_value = io.BytesIO(body)
        return stream

    def test_typed_status_and_authenticated_envelope(self):
        stream = self.response(b'{"status":{"State":"running"},"error":null}\n')
        command = {"command": "diagnostics", "id": "reference"}
        with patch("ecosystem_fixture.socket.create_connection", return_value=stream):
            self.assertEqual(query({"address": "127.0.0.1:1234", "token": "test-only"}, command), {"State": "running"})
        sent = json.loads(stream.sendall.call_args.args[0])
        self.assertEqual(sent, {"token": "test-only", "command": command})

    def test_rejected_truncated_and_oversized_responses(self):
        for body in (b'{"status":null,"error":"private-detail"}\n',
                     b'{"status":"Completed"}', b'x' * (1024 * 1024 + 1)):
            with self.subTest(length=len(body)):
                stream = self.response(body)
                with patch("ecosystem_fixture.socket.create_connection", return_value=stream):
                    with self.assertRaises(RuntimeError) as error:
                        query({"address": "127.0.0.1:1234", "token": "test-only"}, {})
                self.assertNotIn("test-only", str(error.exception))
                self.assertNotIn("private-detail", str(error.exception))


if __name__ == "__main__":
    unittest.main()
