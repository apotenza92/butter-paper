"""Target-budget regressions; no Cargo, macOS tools, or real subprocesses."""
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/run-native-bounded.py"
spec = importlib.util.spec_from_file_location("native_bounded", SCRIPT)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class NativeBoundedTargetTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = (Path(self.scratch.name) / "migration/crate").resolve()
        self.root.mkdir(parents=True)
        self.policy = dict(targetRelativeToMigration=".build-targets/gpui-migration",
                           preflightFreeKiB=100, runtimeStopFreeKiB=50,
                           maxTargetKiB=20, minMemoryKiB=10, wallSeconds=30,
                           terminationGraceSeconds=1, rustMinStackBytes=16777216)
        (self.root / "build-guard-policy.json").write_text(json.dumps(self.policy))

    def resolve(self, arguments, environment=None):
        return runner.resolve_target(arguments, self.root, environment or {}, self.policy)

    def test_explicit_target_forms_override_environment_and_pin_actual_command(self):
        expected = (self.root / ".prepared/window-harness/target").resolve()
        for flags in [["--target-dir", ".prepared/window-harness/target"],
                      ["--target-dir=.prepared/window-harness/target"]]:
            with self.subTest(flags=flags):
                command, env, target = self.resolve(
                    ["cargo", "test"] + flags + ["--", "--test-threads=1"],
                    {"CARGO_TARGET_DIR": "wrong-target"})
                self.assertEqual(target, expected)
                self.assertEqual(env["CARGO_TARGET_DIR"], str(expected))
                self.assertEqual(command, ["cargo", "test", "--target-dir", str(expected),
                                           "--", "--test-threads=1"])

    def test_env_prefix_and_inherited_target_use_actual_subprocess_cwd(self):
        command, env, target = self.resolve(
            ["env", "RUST_MIN_STACK=16777216", "CARGO_TARGET_DIR=../isolated", "cargo", "test"],
            {"CARGO_TARGET_DIR": "overridden"})
        self.assertEqual(target, (self.root.parent / "isolated").resolve())
        self.assertEqual(command[0], "cargo")
        self.assertEqual(env["RUST_MIN_STACK"], "16777216")
        self.assertEqual(self.resolve(["cargo", "build"], {"CARGO_TARGET_DIR": str(target)})[2], target)

    def test_default_target_and_swift_command_preserved(self):
        command, _, target = self.resolve(["xcrun", "swiftc", "-O", "file.swift"])
        self.assertEqual(command, ["xcrun", "swiftc", "-O", "file.swift"])
        self.assertEqual(target, (self.root.parent / self.policy["targetRelativeToMigration"]).resolve())

    def test_forwarded_test_arguments_are_not_cargo_target_overrides(self):
        command, _, target = self.resolve(["cargo", "test", "--", "--target-dir=fixture"])
        self.assertEqual(command[-2:], ["--", "--target-dir=fixture"])
        self.assertEqual(command[2:4], ["--target-dir", str(target)])

    def test_invalid_or_ambiguous_overrides_rejected(self):
        cases = [["cargo", "test", "--target-dir"],
                 ["cargo", "test", "--target-dir="],
                 ["cargo", "test", "--target-dir", "--release"],
                 ["cargo", "test", "--target-dir=a", "--target-dir=b"],
                 ["env", "-C", "/tmp", "cargo", "test"],
                 ["cargo", "test", "--target-dir=/"],
                 ["cargo", "test", "--target-dir=."]]
        for command in cases:
            with self.subTest(command=command), self.assertRaises(ValueError):
                self.resolve(command)
        with self.assertRaises(ValueError):
            self.resolve(["cargo", "test"], {"CARGO_TARGET_DIR": ""})

    def test_target_symlink_resolves_to_monitored_directory(self):
        actual = self.root / "actual"
        actual.mkdir()
        (self.root / "alias").symlink_to(actual, target_is_directory=True)
        command, _, target = self.resolve(["cargo", "test", "--target-dir=alias"])
        self.assertEqual(target, actual)
        self.assertEqual(command[-1], str(actual))

    def test_oversized_explicit_target_blocks_spawn(self):
        actual = self.root / ".prepared/window-harness/target"
        with mock.patch.object(runner, "__file__", str(self.root / "scripts/guard.py")), \
             mock.patch.object(runner.sys, "platform", "darwin"), \
             mock.patch.object(runner.sys, "argv", ["guard", "cargo", "test", "--target-dir", str(actual)]), \
             mock.patch.object(runner, "metrics", return_value=(1000, 21, 1000)) as metrics, \
             mock.patch.object(runner.subprocess, "Popen") as spawn:
            with self.assertRaisesRegex(SystemExit, "window-harness/target"):
                runner.main()
            metrics.assert_called_once_with(self.root, actual)
            spawn.assert_not_called()

    def test_runtime_target_budget_stops_same_target_reported_and_passed_to_cargo(self):
        actual = self.root / "isolated"
        process = mock.Mock()
        process.poll.side_effect = [None, 0]
        process.wait.return_value = 0
        output = io.StringIO()
        with mock.patch.object(runner, "__file__", str(self.root / "scripts/guard.py")), \
             mock.patch.object(runner.sys, "platform", "darwin"), \
             mock.patch.object(runner.sys, "argv", ["guard", "env", "CARGO_TARGET_DIR=isolated", "cargo", "test"]), \
             mock.patch.object(runner, "metrics", side_effect=[(1000, 1, 1000), (1000, 21, 1000)]) as metrics, \
             mock.patch.object(runner.subprocess, "Popen", return_value=process) as spawn, \
             mock.patch.object(runner, "stop") as stop, \
             mock.patch.object(runner.sys, "stdout", output):
            self.assertEqual(runner.main(), 125)
            self.assertEqual(metrics.call_args_list, [mock.call(self.root, actual)] * 2)
            self.assertEqual(spawn.call_args.kwargs["env"]["CARGO_TARGET_DIR"], str(actual))
            self.assertEqual(spawn.call_args.args[0][-2:], ["--target-dir", str(actual)])
            stop.assert_called_once_with(process, 1)
        report = json.loads(output.getvalue())
        self.assertEqual(report["reason"], "target-size")
        self.assertEqual(report["targetDirectory"], str(actual))


if __name__ == "__main__":
    unittest.main()
