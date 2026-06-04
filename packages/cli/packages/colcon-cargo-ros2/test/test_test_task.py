# Copyright 2024 Open Source Robotics Foundation, Inc.
# Licensed under the Apache License, Version 2.0

"""Unit tests for the AmentCargoTestTask."""

import asyncio
import tempfile
import xml.etree.ElementTree as eTree
from pathlib import Path
from subprocess import CompletedProcess
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock, patch

from colcon_cargo_ros2.task.ament_cargo.test import AmentCargoTestTask


class TestBuildTestCmd:
    """Tests for _build_test_cmd method."""

    def setup_method(self):
        """Set up test fixtures."""
        self.task = AmentCargoTestTask()
        self.task.context = MagicMock()
        self.task.context.pkg.path = "/workspace/src/my_package"
        self.task.context.args = SimpleNamespace(verbose=False)

    def test_basic_cmd_generation(self):
        """Test basic command generation with defaults."""
        workspace_root = Path("/workspace")
        build_base = Path("/workspace/build")
        target_dir = "/workspace/build/my_package"

        cmd = self.task._build_test_cmd(workspace_root, build_base, target_dir, [], [])

        assert cmd[0] == "cargo"
        assert cmd[1] == "test"
        assert "--manifest-path" in cmd
        assert "/workspace/src/my_package/Cargo.toml" in cmd
        assert "--target-dir" in cmd
        assert target_dir in cmd
        assert "--quiet" in cmd
        assert "--" in cmd
        assert "--color" in cmd
        assert "never" in cmd

    def test_config_file_included_when_exists(self):
        """Test that --config is included when ros2_cargo_config.toml exists."""
        with tempfile.TemporaryDirectory() as tmpdir:
            build_base = Path(tmpdir)
            config_file = build_base / "ros2_cargo_config.toml"
            config_file.write_text("[patch]\n")
            target_dir = str(build_base / "my_package")

            cmd = self.task._build_test_cmd(Path("/workspace"), build_base, target_dir, [], [])

            assert "--config" in cmd
            config_idx = cmd.index("--config")
            assert str(config_file) == cmd[config_idx + 1]

    def test_config_file_not_included_when_missing(self):
        """Test that --config is not included when config file doesn't exist."""
        with tempfile.TemporaryDirectory() as tmpdir:
            build_base = Path(tmpdir)
            target_dir = str(build_base / "my_package")
            # Don't create the config file

            cmd = self.task._build_test_cmd(Path("/workspace"), build_base, target_dir, [], [])

            assert "--config" not in cmd

    def test_cargo_args_passed_through(self):
        """Test that cargo args are passed through."""
        cmd = self.task._build_test_cmd(
            Path("/workspace"),
            Path("/workspace/build"),
            "/workspace/build/pkg",
            ["--release", "--features", "test"],
            [],
        )

        assert "--release" in cmd
        assert "--features" in cmd
        assert "test" in cmd

    def test_test_args_after_separator(self):
        """Test that test args come after --."""
        cmd = self.task._build_test_cmd(
            Path("/workspace"),
            Path("/workspace/build"),
            "/workspace/build/pkg",
            [],
            ["--nocapture", "--test-threads=1"],
        )

        separator_idx = cmd.index("--")
        assert "--nocapture" in cmd[separator_idx:]
        assert "--test-threads=1" in cmd[separator_idx:]

    def test_quiet_suppressed_when_verbose(self):
        """Test that --quiet is not added in verbose mode."""
        self.task.context.args.verbose = True

        cmd = self.task._build_test_cmd(
            Path("/workspace"), Path("/workspace/build"), "/workspace/build/pkg", [], []
        )

        assert "--quiet" not in cmd

    def test_quiet_suppressed_when_verbose_in_cargo_args(self):
        """Test that --quiet is not added when --verbose in cargo args."""
        cmd = self.task._build_test_cmd(
            Path("/workspace"),
            Path("/workspace/build"),
            "/workspace/build/pkg",
            ["--verbose"],
            [],
        )

        assert "--quiet" not in cmd

    def test_quiet_not_duplicated(self):
        """Test that --quiet is not added when already in cargo args."""
        cmd = self.task._build_test_cmd(
            Path("/workspace"),
            Path("/workspace/build"),
            "/workspace/build/pkg",
            ["--quiet"],
            [],
        )

        # Should only appear once
        assert cmd.count("--quiet") == 1


class TestWriteTestResults:
    """Tests for _write_test_results method."""

    def setup_method(self):
        """Set up test fixtures."""
        self.task = AmentCargoTestTask()

    def test_successful_test_xml(self):
        """Test XML generation for successful tests."""
        with tempfile.TemporaryDirectory() as tmpdir:
            result_path = Path(tmpdir) / "cargo_test.xml"
            test_result = CompletedProcess(args=[], returncode=0)

            self.task._write_test_results(result_path, "my_package", test_result)

            assert result_path.exists()
            tree = eTree.parse(result_path)
            root = tree.getroot()

            testsuite = root.find("testsuite")
            assert testsuite is not None
            assert testsuite.get("name") == "my_package"
            assert testsuite.get("tests") == "1"
            assert testsuite.get("failures") == "0"

            testcase = testsuite.find("testcase[@name='cargo_test']")
            assert testcase is not None
            assert testcase.find("failure") is None

    def test_failed_test_xml(self):
        """Test XML generation for failed tests."""
        with tempfile.TemporaryDirectory() as tmpdir:
            result_path = Path(tmpdir) / "cargo_test.xml"
            test_result = CompletedProcess(args=[], returncode=1, stdout=b"test failed", stderr=b"")

            self.task._write_test_results(result_path, "my_package", test_result)

            tree = eTree.parse(result_path)
            root = tree.getroot()
            testsuite = root.find("testsuite")
            assert testsuite.get("failures") == "1"

            testcase = testsuite.find("testcase[@name='cargo_test']")
            failure = testcase.find("failure")
            assert failure is not None
            assert "cargo test failed" in failure.get("message")
            assert "test failed" in failure.text

    def test_fmt_check_included_when_provided(self):
        """Test XML includes fmt check results when provided."""
        with tempfile.TemporaryDirectory() as tmpdir:
            result_path = Path(tmpdir) / "cargo_test.xml"
            test_result = CompletedProcess(args=[], returncode=0)
            fmt_result = CompletedProcess(args=[], returncode=0)

            self.task._write_test_results(result_path, "my_package", test_result, fmt_result)

            tree = eTree.parse(result_path)
            testsuite = tree.getroot().find("testsuite")
            assert testsuite.get("tests") == "2"

            fmt_testcase = testsuite.find("testcase[@name='cargo_fmt']")
            assert fmt_testcase is not None

    def test_fmt_failure_recorded(self):
        """Test that fmt failures are recorded in XML."""
        with tempfile.TemporaryDirectory() as tmpdir:
            result_path = Path(tmpdir) / "cargo_test.xml"
            test_result = CompletedProcess(args=[], returncode=0)
            fmt_result = CompletedProcess(args=[], returncode=1, stdout=b"Diff in file.rs")

            self.task._write_test_results(result_path, "my_package", test_result, fmt_result)

            tree = eTree.parse(result_path)
            testsuite = tree.getroot().find("testsuite")
            assert testsuite.get("failures") == "1"

            fmt_testcase = testsuite.find("testcase[@name='cargo_fmt']")
            failure = fmt_testcase.find("failure")
            assert failure is not None
            assert "Diff in file.rs" in failure.text

    def test_creates_parent_directories(self):
        """Test that parent directories are created if needed."""
        with tempfile.TemporaryDirectory() as tmpdir:
            result_path = Path(tmpdir) / "nested" / "dir" / "cargo_test.xml"
            test_result = CompletedProcess(args=[], returncode=0)

            self.task._write_test_results(result_path, "my_package", test_result)

            assert result_path.exists()


class TestTestMethod:
    """Tests for the main test() method."""

    def setup_method(self):
        """Set up test fixtures."""
        self.task = AmentCargoTestTask()

    def test_posts_test_failure_event_on_failure(self):
        """Test that TestFailure event is posted when tests fail."""
        with tempfile.TemporaryDirectory() as tmpdir:
            build_base = Path(tmpdir) / "build" / "my_package"
            build_base.mkdir(parents=True)

            self.task.context = MagicMock()
            self.task.context.pkg.name = "my_package"
            self.task.context.pkg.path = "/workspace/src/my_package"
            self.task.context.args = SimpleNamespace(
                build_base=str(build_base),
                path="/workspace/src/my_package",
                verbose=False,
                cargo_args=None,
                cargo_test_args=None,
                cargo_fmt_check=False,
                test_result_base=None,
                retest_until_pass=0,
                retest_until_fail=0,
            )
            self.task.context.dependencies = {}

            # Mock get_command_environment
            with patch(
                "colcon_cargo_ros2.task.ament_cargo.test.get_command_environment",
                new_callable=AsyncMock,
            ) as mock_env:
                mock_env.return_value = {}

                # Mock run to return failure
                with patch(
                    "colcon_cargo_ros2.task.ament_cargo.test.run",
                    new_callable=AsyncMock,
                ) as mock_run:
                    mock_run.return_value = CompletedProcess(
                        args=[], returncode=1, stdout=b"", stderr=b""
                    )

                    rc = asyncio.run(self.task.test())

                    # Should return 0 (colcon handles failures via events)
                    assert rc == 0

                    # Should have posted TestFailure event
                    self.task.context.put_event_into_queue.assert_called_once()
                    event = self.task.context.put_event_into_queue.call_args[0][0]
                    assert event.identifier == "my_package"

    def test_no_failure_event_on_success(self):
        """Test that no TestFailure event is posted when tests pass."""
        with tempfile.TemporaryDirectory() as tmpdir:
            build_base = Path(tmpdir) / "build" / "my_package"
            build_base.mkdir(parents=True)

            self.task.context = MagicMock()
            self.task.context.pkg.name = "my_package"
            self.task.context.pkg.path = "/workspace/src/my_package"
            self.task.context.args = SimpleNamespace(
                build_base=str(build_base),
                path="/workspace/src/my_package",
                verbose=False,
                cargo_args=None,
                cargo_test_args=None,
                cargo_fmt_check=False,
                test_result_base=None,
                retest_until_pass=0,
                retest_until_fail=0,
            )
            self.task.context.dependencies = {}

            with patch(
                "colcon_cargo_ros2.task.ament_cargo.test.get_command_environment",
                new_callable=AsyncMock,
            ) as mock_env:
                mock_env.return_value = {}

                with patch(
                    "colcon_cargo_ros2.task.ament_cargo.test.run",
                    new_callable=AsyncMock,
                ) as mock_run:
                    mock_run.return_value = CompletedProcess(
                        args=[], returncode=0, stdout=b"", stderr=b""
                    )

                    rc = asyncio.run(self.task.test())

                    assert rc == 0
                    self.task.context.put_event_into_queue.assert_not_called()

    def test_generates_junit_xml(self):
        """Test that JUnit XML is generated."""
        with tempfile.TemporaryDirectory() as tmpdir:
            build_base = Path(tmpdir) / "build" / "my_package"
            build_base.mkdir(parents=True)

            self.task.context = MagicMock()
            self.task.context.pkg.name = "my_package"
            self.task.context.pkg.path = "/workspace/src/my_package"
            self.task.context.args = SimpleNamespace(
                build_base=str(build_base),
                path="/workspace/src/my_package",
                verbose=False,
                cargo_args=None,
                cargo_test_args=None,
                cargo_fmt_check=False,
                test_result_base=None,
                retest_until_pass=0,
                retest_until_fail=0,
            )
            self.task.context.dependencies = {}

            with patch(
                "colcon_cargo_ros2.task.ament_cargo.test.get_command_environment",
                new_callable=AsyncMock,
            ) as mock_env:
                mock_env.return_value = {}

                with patch(
                    "colcon_cargo_ros2.task.ament_cargo.test.run",
                    new_callable=AsyncMock,
                ) as mock_run:
                    mock_run.return_value = CompletedProcess(
                        args=[], returncode=0, stdout=b"", stderr=b""
                    )

                    asyncio.run(self.task.test())

                    # Check XML was generated
                    xml_path = build_base / "cargo_test.xml"
                    assert xml_path.exists()

                    tree = eTree.parse(xml_path)
                    assert tree.getroot().tag == "testsuites"


class TestAddArguments:
    """Tests for add_arguments method."""

    def test_adds_cargo_args(self):
        """Test that --cargo-args argument is added."""
        task = AmentCargoTestTask()
        parser = MagicMock()

        task.add_arguments(parser=parser)

        # Check that add_argument was called with expected arguments
        calls = [str(call) for call in parser.add_argument.call_args_list]
        assert any("--cargo-args" in call for call in calls)

    def test_adds_cargo_test_args(self):
        """Test that --cargo-test-args argument is added."""
        task = AmentCargoTestTask()
        parser = MagicMock()

        task.add_arguments(parser=parser)

        calls = [str(call) for call in parser.add_argument.call_args_list]
        assert any("--cargo-test-args" in call for call in calls)

    def test_adds_cargo_fmt_check(self):
        """Test that --cargo-fmt-check argument is added."""
        task = AmentCargoTestTask()
        parser = MagicMock()

        task.add_arguments(parser=parser)

        calls = [str(call) for call in parser.add_argument.call_args_list]
        assert any("--cargo-fmt-check" in call for call in calls)
