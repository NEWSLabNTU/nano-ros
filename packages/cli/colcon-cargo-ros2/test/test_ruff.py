# Licensed under the Apache License, Version 2.0

import subprocess
import sys
from pathlib import Path

import pytest


@pytest.mark.linter
def test_ruff():
    """Test code style using ruff linter."""
    package_dir = Path(__file__).parents[1]
    config_file = package_dir / "ruff.toml"

    # Check if ruff configuration exists
    if config_file.exists():
        # Use configuration file
        result = subprocess.run(
            ["ruff", "check", str(package_dir)],
            capture_output=True,
            text=True,
        )

        if result.returncode != 0:  # pragma: no cover
            print(result.stdout, file=sys.stderr)
            print(f"ruff reported errors (exit code: {result.returncode})", file=sys.stderr)

        assert result.returncode == 0, f"ruff reported errors:\n{result.stdout}"
        return

    # Fallback to inline configuration if no config file
    source_dir = package_dir / "colcon_cargo_ros2"
    test_dir = package_dir / "test"

    ignore_rules = [
        "D100",  # Missing docstring in public module
        "D104",  # Missing docstring in public package
    ]

    test_ignore_rules = ignore_rules + [
        "D101",  # Missing docstring in public class
        "D102",  # Missing docstring in public method
        "D103",  # Missing docstring in public function
        "D105",  # Missing docstring in magic method
        "D107",  # Missing docstring in __init__
    ]

    # Check main source code
    result_source = subprocess.run(
        [
            "ruff",
            "check",
            str(source_dir),
            "--ignore=" + ",".join(ignore_rules),
            "--line-length=100",
        ],
        capture_output=True,
        text=True,
    )

    # Check test code
    result_tests = subprocess.run(
        [
            "ruff",
            "check",
            str(test_dir),
            "--ignore=" + ",".join(test_ignore_rules),
            "--line-length=100",
        ],
        capture_output=True,
        text=True,
    )

    # Combine outputs
    combined_output = ""
    if result_source.stdout:
        combined_output += f"Source code issues:\n{result_source.stdout}\n"
    if result_tests.stdout:
        combined_output += f"Test code issues:\n{result_tests.stdout}\n"

    # Check for errors
    total_errors = result_source.returncode + result_tests.returncode
    if total_errors:  # pragma: no cover
        print(combined_output, file=sys.stderr)
        print(
            f"ruff reported errors (exit codes: "
            f"source={result_source.returncode}, tests={result_tests.returncode})",
            file=sys.stderr,
        )

    assert total_errors == 0, f"ruff reported errors:\n{combined_output}"
