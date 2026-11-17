#!/usr/bin/env python3
"""`nano_ros_hosted_cxx()` makes a package directory's C++ hosted on FreeRTOS.

WHAT IS BEING MEASURED

The FreeRTOS toolchain compiles C++ `-ffreestanding`, and libstdc++ then makes
every hosted header (`<cmath>`, `<vector>`, `<complex>`) a hard `#error`. A
library such as Eigen cannot compile there. `nano_ros_hosted_cxx()`
(cmake/NanoRosPortedCxx.cmake) is the public opt-in a node package calls to
make its own directory hosted; the book's "Bring your own C++ library" page
tells users to call it.

So this configures a two-directory CMake project with the REAL toolchain file
(`cmake/toolchain/arm-freertos-armcm3.cmake`, the pinned arm-none-eabi-g++),
and compiles one tracked TU (`hosted_cxx_opt_in.cpp`, Eigen 3.4's
standard-library include set) in both directories:

  hosted/        calls nano_ros_hosted_cxx()   must COMPILE
  freestanding/  does not                      must FAIL, in libstdc++'s
                                               freestanding refusal

The second half is the negative control. Without it, the probe would go on
passing the day the toolchain stopped being freestanding, or the day the
include list lost every hosted header.

Then two script-mode cases, no compiler needed:

  flags with `-nostdinc++` (ThreadX's shim)   must be REFUSED, naming the shim
  flags without `-ffreestanding` (POSIX)      must be left as they were

A host without the pinned arm toolchain records a NOT VERIFIED skip; it never
passes silently.

Usage::

    check-hosted-cxx-opt-in.py
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import check_skip  # noqa: E402  the NOT VERIFIED ledger (issue 1345)

NAME = "hosted-cxx-opt-in"
MODULE = os.path.join(ROOT, "cmake", "NanoRosPortedCxx.cmake")
TOOLCHAIN = os.path.join(ROOT, "cmake", "toolchain", "arm-freertos-armcm3.cmake")
PROBE = os.path.join(ROOT, "packages", "api", "nros-cpp", "tests", "compile", "hosted_cxx_opt_in.cpp")
FREESTANDING_REFUSAL = "not available in freestanding mode"
SHIM_REFUSAL = "freestanding shim"


def pinned_arm_gxx() -> str | None:
    """The pinned arm g++, from `nros-sdk-index.toml` through the one shell pin
    reader (issue 1736), under `$NROS_HOME`, else `~/.nros` (RFC-0103 D6)."""
    try:
        pin = subprocess.run(
            ["bash", "-c", '. scripts/lib/sdk-pin.sh && nros_sdk_pinned_version "$1"',
             "_", "arm-none-eabi-gcc"],
            cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    if not pin:
        return None
    store = os.path.join(os.environ.get("NROS_HOME") or os.path.expanduser("~/.nros"), "sdk")
    return os.path.join(store, "arm-none-eabi-gcc", pin, "bin", "arm-none-eabi-g++")


def project(tmp: str) -> str:
    src = os.path.join(tmp, "src")
    for sub, call in (("hosted", "nano_ros_hosted_cxx()\n"), ("freestanding", "")):
        os.makedirs(os.path.join(src, sub))
        with open(os.path.join(src, sub, "CMakeLists.txt"), "w") as f:
            f.write(f"{call}add_library({sub}_probe OBJECT \"{PROBE}\")\n")
    with open(os.path.join(src, "CMakeLists.txt"), "w") as f:
        f.write(
            "cmake_minimum_required(VERSION 3.22)\n"
            "project(hosted_cxx_opt_in CXX)\n"
            f"include(\"{MODULE}\")\n"
            "add_subdirectory(hosted)\n"
            "add_subdirectory(freestanding)\n")
    return src


def run(argv: list[str], cwd: str) -> tuple[int, str]:
    proc = subprocess.run(argv, cwd=cwd, capture_output=True, text=True)
    return proc.returncode, proc.stdout + proc.stderr


def script_case(tmp: str, flags: str) -> tuple[int, str]:
    path = os.path.join(tmp, "case.cmake")
    with open(path, "w") as f:
        f.write(
            f"set(CMAKE_CXX_FLAGS \"{flags}\")\n"
            f"include(\"{MODULE}\")\n"
            "nano_ros_hosted_cxx()\n"
            "message(STATUS \"flags after: [${CMAKE_CXX_FLAGS}]\")\n")
    return run(["cmake", "-P", path], tmp)


def script_cases(tmp: str) -> list[str]:
    failures = []
    rc, out = script_case(tmp, "-std=c++14 -ffreestanding -nostdinc++")
    if rc == 0 or SHIM_REFUSAL not in out:
        failures.append(
            "a toolchain with -nostdinc++ (ThreadX's shim) was not refused naming the "
            "shim; dropping -ffreestanding there only moves the failure into <vector>.\n"
            + out.strip()[:800])
    rc, out = script_case(tmp, "-std=c++17 -O2")
    if rc != 0 or "flags after: [-std=c++17 -O2]" not in out:
        failures.append(
            "a toolchain without -ffreestanding (POSIX, Zephyr) did not keep its flags "
            "unchanged; the call must be a no-op there.\n" + out.strip()[:800])
    return failures


def compile_cases(tmp: str) -> list[str]:
    src = project(tmp)
    build = os.path.join(tmp, "build")
    rc, out = run(["cmake", "-S", src, "-B", build, f"-DCMAKE_TOOLCHAIN_FILE={TOOLCHAIN}"], tmp)
    if rc != 0:
        return ["the probe project did not configure with the FreeRTOS toolchain file.\n"
                + out.strip()[-1500:]]
    failures = []
    rc, out = run(["cmake", "--build", build, "--target", "hosted_probe"], tmp)
    if rc != 0:
        failures.append(
            "the directory that called nano_ros_hosted_cxx() could NOT compile Eigen's "
            "standard-library set under the FreeRTOS toolchain.\n" + out.strip()[-1500:])
    rc, out = run(["cmake", "--build", build, "--target", "freestanding_probe"], tmp)
    if rc == 0:
        failures.append(
            "SELFTEST FAILED: the directory that did NOT opt in compiled the hosted set. "
            "The toolchain is no longer freestanding, or the probe lost its hosted "
            "headers, so the positive half proves nothing.")
    elif FREESTANDING_REFUSAL not in out:
        failures.append(
            "SELFTEST FAILED: the directory that did not opt in failed, but not in "
            f"libstdc++'s freestanding refusal (`{FREESTANDING_REFUSAL}`), so the failure "
            "is not the one this probe exists for.\n" + out.strip()[-1500:])
    return failures


def main() -> int:
    if not os.path.exists(PROBE):
        print(f"check-{NAME}: {PROBE} is MISSING -- this gate would pass on absence",
              file=sys.stderr)
        return 1
    skipped = 0
    with tempfile.TemporaryDirectory(prefix="nros-hosted-cxx-") as tmp:
        failures = script_cases(tmp)
        arm = pinned_arm_gxx()
        if arm and os.path.exists(arm):
            failures += compile_cases(tmp)
            ran = "the FreeRTOS compile pair and "
        else:
            print(f"check-{NAME}: the FreeRTOS compile pair SKIPPED -- pinned "
                  "arm-none-eabi-g++ absent (nros setup mps2-an385-freertos)")
            skipped |= check_skip.unverified(NAME, "pinned arm-none-eabi-g++ absent")
            ran = ""
    if failures:
        for f in failures:
            print(f"FAIL: {f}", file=sys.stderr)
        return 1
    print(f"check-{NAME}: OK -- {ran}the -nostdinc++ refusal and the POSIX no-op")
    return skipped


if __name__ == "__main__":
    sys.exit(main())
