# Licensed under the Apache License, Version 2.0

"""Test task for Rust ROS 2 packages using cargo test."""

import os
import xml.etree.ElementTree as eTree
from pathlib import Path
from xml.dom import minidom

from colcon_core.event.test import TestFailure
from colcon_core.logging import colcon_logger
from colcon_core.plugin_system import satisfies_version
from colcon_core.shell import get_command_environment
from colcon_core.task import TaskExtensionPoint, run

logger = colcon_logger.getChild(__name__)


class AmentCargoTestTask(TaskExtensionPoint):
    """Test Rust ROS 2 packages using cargo test.

    This task runs cargo test with proper workspace configuration:
    - Uses --config build/ros2_cargo_config.toml for ROS message bindings
    - Sources dependency environments via get_command_environment()
    - Generates JUnit XML output for colcon test-result
    - Posts TestFailure events for proper failure reporting
    - Optionally runs cargo fmt --check for style verification
    """

    def __init__(self):  # noqa: D107
        super().__init__()
        satisfies_version(TaskExtensionPoint.EXTENSION_POINT_VERSION, "^1.0")

    def add_arguments(self, *, parser):  # noqa: D102
        parser.add_argument(
            "--cargo-args",
            nargs="*",
            metavar="*",
            type=str.lstrip,
            help="Pass arguments to cargo test. "
            "Arguments matching other options must be prefixed by a space,\n"
            'e.g. --cargo-args " --help"',
        )
        parser.add_argument(
            "--cargo-test-args",
            nargs="*",
            metavar="*",
            type=str.lstrip,
            help='Pass arguments to test binary (after --). e.g. --cargo-test-args " --nocapture"',
        )
        parser.add_argument(
            "--cargo-fmt-check",
            action="store_true",
            help="Run cargo fmt --check as part of testing",
        )

    async def test(self, *, additional_hooks=None):  # noqa: D102
        """Run tests using cargo test with proper workspace configuration."""
        pkg = self.context.pkg
        args = self.context.args

        logger.info(f"Testing Rust package in '{args.path}'")

        # Derive workspace paths (same logic as build task)
        build_base = Path(os.path.abspath(os.path.join(args.build_base, "..")))
        workspace_root = build_base.parent

        # Get command environment with dependencies sourced
        try:
            env = await get_command_environment("test", args.build_base, self.context.dependencies)
        except RuntimeError as e:
            logger.error(str(e))
            return 1

        # Build test command with proper config
        cargo_args = getattr(args, "cargo_args", None) or []
        test_args = getattr(args, "cargo_test_args", None) or []

        cmd = self._build_test_cmd(
            workspace_root, build_base, args.build_base, cargo_args, test_args
        )

        # Support retest options
        retest_until_pass = getattr(args, "retest_until_pass", 0)
        retest_until_fail = getattr(args, "retest_until_fail", 0)

        rerun = 0
        completed = None
        while True:
            # Run cargo test from workspace root (so relative paths in config work)
            completed = await run(
                self.context, cmd, cwd=workspace_root, env=env, capture_output=True
            )

            if not completed.returncode:
                # Tests passed
                if retest_until_fail > rerun:
                    # Keep running until failure
                    rerun += 1
                    logger.info(f"Retest {rerun}/{retest_until_fail} passed, continuing")
                    continue
                break

            # Tests failed
            if retest_until_pass > rerun:
                # Retry failed tests
                rerun += 1
                logger.info(f"Retest {rerun}/{retest_until_pass} failed, retrying")
                continue
            break

        # Optionally run cargo fmt --check
        fmt_check = getattr(args, "cargo_fmt_check", False)
        fmt_completed = None
        if fmt_check:
            fmt_completed = await self._run_fmt_check(args.path, env)

        # Generate JUnit XML test results
        test_result_base = getattr(args, "test_result_base", None)
        test_result_path = Path(test_result_base or args.build_base) / "cargo_test.xml"
        self._write_test_results(test_result_path, pkg.name, completed, fmt_completed)

        # Post TestFailure event if tests failed
        test_failed = completed.returncode != 0
        fmt_failed = fmt_completed is not None and fmt_completed.returncode != 0
        if test_failed or fmt_failed:
            self.context.put_event_into_queue(TestFailure(pkg.name))

        # Always return 0 - colcon handles failure reporting via TestFailure events
        return 0

    def _build_test_cmd(self, workspace_root, build_base, target_dir, cargo_args, test_args):
        """Build the cargo test command with workspace configuration.

        Uses --config flag to include ROS message binding patches from
        build/ros2_cargo_config.toml, and --manifest-path since cargo
        is invoked from workspace root.
        """
        cmd = ["cargo", "test"]

        # Add --manifest-path to specify which package to test
        manifest_path = Path(self.context.pkg.path) / "Cargo.toml"
        cmd.extend(["--manifest-path", str(manifest_path)])

        # Add --target-dir to place test artifacts in expected location
        cmd.extend(["--target-dir", str(target_dir)])

        # Add --config flag to use workspace-level config file
        config_file = build_base / "ros2_cargo_config.toml"
        if config_file.exists():
            cmd.extend(["--config", str(config_file)])

        # Add --quiet flag by default (matching build behavior)
        args = self.context.args
        verbose = getattr(args, "verbose", False)
        has_verbose_flag = "--verbose" in cargo_args or "-v" in cargo_args
        has_quiet_flag = "--quiet" in cargo_args or "-q" in cargo_args

        if not verbose and not has_verbose_flag and not has_quiet_flag:
            cmd.append("--quiet")

        # Add user cargo arguments
        cmd.extend(cargo_args)

        # Add test binary arguments after --
        cmd.append("--")
        cmd.extend(["--color", "never"])
        cmd.extend(test_args)

        return cmd

    async def _run_fmt_check(self, pkg_path, env):
        """Run cargo fmt --check for style verification.

        Returns CompletedProcess or None if fmt check failed to run.
        """
        cmd = ["cargo", "fmt", "--check", "--", "--color=never"]
        try:
            return await run(self.context, cmd, cwd=pkg_path, env=env, capture_output=True)
        except Exception as e:
            logger.warning(f"cargo fmt check failed to run: {e}")
            return None

    def _write_test_results(self, path, pkg_name, test_result, fmt_result=None):
        """Write JUnit XML test results for colcon test-result.

        Generates a JUnit XML file with test cases for:
        - cargo_test: Unit/integration tests
        - cargo_fmt: Style check (if enabled)
        """
        path.parent.mkdir(parents=True, exist_ok=True)

        # Count failures
        test_failures = 1 if test_result.returncode else 0
        fmt_failures = 1 if fmt_result and fmt_result.returncode else 0
        total_tests = 1 + (1 if fmt_result is not None else 0)
        total_failures = test_failures + fmt_failures

        # Build XML structure
        testsuites = eTree.Element("testsuites")
        testsuite = eTree.SubElement(
            testsuites,
            "testsuite",
            {
                "name": pkg_name,
                "tests": str(total_tests),
                "failures": str(total_failures),
                "errors": "0",
                "skipped": "0",
            },
        )

        # Add cargo test result
        test_testcase = eTree.SubElement(
            testsuite, "testcase", {"name": "cargo_test", "classname": pkg_name}
        )
        if test_result.returncode:
            failure = eTree.SubElement(
                test_testcase,
                "failure",
                {"message": f"cargo test failed with code {test_result.returncode}"},
            )
            # Include stdout/stderr in failure message
            output_parts = []
            if hasattr(test_result, "stdout") and test_result.stdout:
                output_parts.append(test_result.stdout.decode("utf-8", errors="replace"))
            if hasattr(test_result, "stderr") and test_result.stderr:
                output_parts.append(test_result.stderr.decode("utf-8", errors="replace"))
            if output_parts:
                failure.text = "\n".join(output_parts)

        # Add cargo fmt result if enabled
        if fmt_result is not None:
            fmt_testcase = eTree.SubElement(
                testsuite, "testcase", {"name": "cargo_fmt", "classname": pkg_name}
            )
            if fmt_result.returncode:
                failure = eTree.SubElement(
                    fmt_testcase,
                    "failure",
                    {"message": "cargo fmt --check found formatting issues"},
                )
                if hasattr(fmt_result, "stdout") and fmt_result.stdout:
                    failure.text = fmt_result.stdout.decode("utf-8", errors="replace")

        # Write pretty-printed XML
        xml_str = minidom.parseString(eTree.tostring(testsuites))
        path.write_bytes(xml_str.toprettyxml(indent="    ", encoding="utf-8"))
        logger.debug(f"Test results written to {path}")
