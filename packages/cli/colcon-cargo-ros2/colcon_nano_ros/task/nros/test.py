# Licensed under the Apache License, Version 2.0

import os
import subprocess
from pathlib import Path
from xml.dom import minidom
from xml.etree import ElementTree as ET

from colcon_core.event.test import TestFailure
from colcon_core.logging import colcon_logger
from colcon_core.plugin_system import satisfies_version
from colcon_core.task import TaskExtensionPoint

from .build import parse_nros_type

logger = colcon_logger.getChild(__name__)

# Default timeout in seconds for test execution
NATIVE_TIMEOUT = 30
QEMU_TIMEOUT = 60


class NrosTestTask(TaskExtensionPoint):
    """Test task for nano-ros packages (nros.<lang>.<platform>).

    Runs tests appropriate for the target platform:
    - native: execute binary directly, check exit code
    - freertos/baremetal: launch QEMU, capture semihosting output, timeout
    - zephyr: run native_sim binary directly
    """

    def __init__(self):  # noqa: D107
        super().__init__()
        satisfies_version(TaskExtensionPoint.EXTENSION_POINT_VERSION, "^1.0")

    async def test(self, *, additional_hooks=None):  # noqa: D102
        pkg = self.context.pkg
        args = self.context.args

        lang, platform = parse_nros_type(pkg.type)
        logger.info(f"Testing nros package '{pkg.name}' (lang={lang}, platform={platform})")

        install_base = Path(args.install_base)
        build_base = Path(args.build_base)

        # Place JUnit XML where colcon test-result expects it
        test_result_base = getattr(args, "test_result_base", None)
        if test_result_base:
            test_results_dir = Path(test_result_base) / pkg.name
        else:
            test_results_dir = build_base
        test_results_path = test_results_dir / "nros_test.xml"

        if platform == "native":
            rc, stdout, stderr = self._run_native(pkg, install_base)
        elif platform in ("freertos", "baremetal", "nuttx"):
            rc, stdout, stderr = self._run_qemu(pkg, install_base, platform)
        elif platform == "zephyr":
            rc, stdout, stderr = self._run_zephyr(pkg, install_base, build_base)
        else:
            logger.warning(f"No test runner for platform '{platform}' — skipping")
            self._write_junit(test_results_path, pkg.name, "skipped", 0, "", "")
            return 0

        passed = rc == 0
        self._write_junit(
            test_results_path, pkg.name, "passed" if passed else "failed", rc, stdout, stderr
        )

        if not passed:
            logger.error(
                f"Test failed for '{pkg.name}' (exit code {rc})\n"
                f"stdout:\n{stdout}\nstderr:\n{stderr}"
            )
            self.context.put_event_into_queue(TestFailure(pkg.name))

        return 0

    def _run_native(self, pkg, install_base):
        """Run a native binary directly."""
        lib_dir = install_base / "lib" / pkg.name
        # Find the first executable
        binary = self._find_binary(lib_dir)
        if not binary:
            return 1, "", f"No executable found in {lib_dir}"

        logger.info(f"Running native test: {binary}")
        try:
            result = subprocess.run(
                [str(binary)], capture_output=True, text=True, timeout=NATIVE_TIMEOUT
            )
            return result.returncode, result.stdout, result.stderr
        except subprocess.TimeoutExpired:
            return 1, "", f"Test timed out after {NATIVE_TIMEOUT}s"
        except Exception as e:
            return 1, "", str(e)

    def _run_qemu(self, pkg, install_base, platform):
        """Run firmware on QEMU and capture semihosting output."""
        lib_dir = install_base / "lib" / pkg.name
        binary = self._find_binary(lib_dir)
        if not binary:
            return 1, "", f"No firmware found in {lib_dir}"

        logger.info(f"Running QEMU test: {binary} (platform={platform})")
        cmd = [
            "qemu-system-arm",
            "-cpu",
            "cortex-m3",
            "-machine",
            "mps2-an385",
            "-nographic",
            "-icount",
            "shift=auto",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
            str(binary),
        ]

        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=QEMU_TIMEOUT)
            return result.returncode, result.stdout, result.stderr
        except subprocess.TimeoutExpired:
            return 1, "", f"QEMU timed out after {QEMU_TIMEOUT}s"
        except FileNotFoundError:
            return 1, "", "qemu-system-arm not found"
        except Exception as e:
            return 1, "", str(e)

    def _run_zephyr(self, pkg, install_base, build_base):
        """Run a Zephyr native_sim binary or flash to hardware."""
        lib_dir = install_base / "lib" / pkg.name

        # Check for native_sim binary first
        native_sim = lib_dir / "zephyr.exe"
        if native_sim.exists():
            logger.info(f"Running Zephyr native_sim: {native_sim}")
            try:
                result = subprocess.run(
                    [str(native_sim)], capture_output=True, text=True, timeout=NATIVE_TIMEOUT
                )
                return result.returncode, result.stdout, result.stderr
            except subprocess.TimeoutExpired:
                return 1, "", f"native_sim timed out after {NATIVE_TIMEOUT}s"
            except Exception as e:
                return 1, "", str(e)

        # Check for ELF (hardware target — use west flash)
        elf = lib_dir / "zephyr.elf"
        if elf.exists():
            logger.warning(
                f"Hardware Zephyr binary found ({elf}) — "
                f"west flash not yet supported in colcon test"
            )
            return 1, "", "Hardware Zephyr testing not implemented"

        return 1, "", f"No Zephyr binary found in {lib_dir}"

    def _find_binary(self, lib_dir):
        """Find the first executable file in a directory."""
        if not lib_dir.exists():
            return None
        for f in sorted(lib_dir.iterdir()):
            if f.is_file() and (os.access(str(f), os.X_OK) or f.suffix in (".elf", ".exe")):
                return f
        return None

    def _write_junit(self, path, test_name, status, rc, stdout, stderr):
        """Write a JUnit XML test result file."""
        path.parent.mkdir(parents=True, exist_ok=True)

        testsuite = ET.Element(
            "testsuite",
            {
                "name": test_name,
                "tests": "1",
                "failures": "0" if status != "failed" else "1",
                "errors": "0",
                "skipped": "1" if status == "skipped" else "0",
            },
        )
        testcase = ET.SubElement(
            testsuite,
            "testcase",
            {
                "name": f"{test_name}_run",
                "classname": test_name,
            },
        )

        if status == "failed":
            failure = ET.SubElement(
                testcase,
                "failure",
                {
                    "message": f"Exit code {rc}",
                },
            )
            failure.text = f"stdout:\n{stdout}\nstderr:\n{stderr}"
        elif status == "skipped":
            ET.SubElement(
                testcase,
                "skipped",
                {
                    "message": "No test runner for this platform",
                },
            )

        if stdout:
            so = ET.SubElement(testcase, "system-out")
            so.text = stdout
        if stderr:
            se = ET.SubElement(testcase, "system-err")
            se.text = stderr

        xmlstr = minidom.parseString(ET.tostring(testsuite))
        with open(str(path), "wb") as f:
            f.write(xmlstr.toprettyxml(indent="    ", encoding="utf-8"))
