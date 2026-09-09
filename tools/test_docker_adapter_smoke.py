import unittest
from unittest.mock import patch

import docker_adapter_smoke as smoke


class DockerFixtureTests(unittest.TestCase):
    def test_uncertain_create_recovers_only_exact_owned_identity_before_cleanup(self):
        fixture = smoke.DockerFixture("already-local:test")
        calls = []

        def fake_docker(*args, **kwargs):
            calls.append(args)
            if args[0] == "create":
                raise smoke.subprocess.TimeoutExpired("docker create", 20)
            if args[0] == "ps":
                if "--quiet" in args:
                    return ""
                self.assertIn("label=" + smoke.LABEL + "=" + fixture.name, args)
                self.assertIn("name=^/" + fixture.name + "$", args)
                return "c" * 64
            if args[0] == "inspect":
                return fixture.name
            return ""

        with patch.object(smoke, "docker", side_effect=fake_docker):
            with self.assertRaises(smoke.subprocess.TimeoutExpired):
                fixture.__enter__()
        self.assertIn(("rm", "--force", "--volumes", "c" * 64), calls)

    def test_creation_has_no_pull_host_mount_network_or_privilege(self):
        fixture = smoke.DockerFixture("already-local:test")
        container_id = "a" * 64
        calls = []

        def fake_docker(*args, **kwargs):
            calls.append(args)
            if args[0] == "create":
                return container_id
            if args[0] == "inspect":
                return fixture.name
            return ""

        with patch.object(smoke, "docker", side_effect=fake_docker):
            with fixture:
                self.assertEqual(fixture.id, container_id)
        create = next(c for c in calls if c[0] == "create")
        self.assertIn("--pull=never", create)
        self.assertIn("--read-only", create)
        self.assertEqual(create[create.index("--network") + 1], "none")
        self.assertEqual(create[create.index("--cap-drop") + 1], "ALL")
        self.assertNotIn("--mount", create)
        self.assertNotIn("--volume", create)
        self.assertNotIn("--privileged", create)
        self.assertIn(("rm", "--force", "--volumes", container_id), calls)
        self.assertTrue(all("pull" != c[0] for c in calls))

    def test_cleanup_refuses_unowned_container(self):
        fixture = smoke.DockerFixture("already-local:test")
        fixture.id = "a" * 64
        with patch.object(smoke, "docker", return_value="wrong-owner") as docker:
            with self.assertRaisesRegex(RuntimeError, "ownership changed"):
                fixture.__exit__(None, None, None)
            self.assertEqual(docker.call_count, 1)
            self.assertEqual(docker.call_args.args[0], "inspect")

    def test_missing_local_image_never_creates_or_downloads(self):
        fixture = smoke.DockerFixture("missing:test")
        with patch.object(smoke, "docker", side_effect=RuntimeError("missing image")) as docker:
            with self.assertRaisesRegex(RuntimeError, "missing image"):
                fixture.__enter__()
            self.assertIsNone(fixture.id)
            self.assertEqual(docker.call_count, 1)
            self.assertEqual(docker.call_args.args[:2], ("image", "inspect"))

    def test_failed_start_cleans_only_created_fixture(self):
        fixture = smoke.DockerFixture("already-local:test")
        calls = []

        def fake_docker(*args, **kwargs):
            calls.append(args)
            if args[0] == "create":
                return "b" * 64
            if args[0] == "start":
                raise RuntimeError("start failed")
            if args[0] == "inspect":
                return fixture.name
            return ""

        with patch.object(smoke, "docker", side_effect=fake_docker):
            with self.assertRaisesRegex(RuntimeError, "start failed"):
                fixture.__enter__()
        self.assertIn(("rm", "--force", "--volumes", "b" * 64), calls)


if __name__ == "__main__":
    unittest.main()
