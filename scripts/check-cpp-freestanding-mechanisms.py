#!/usr/bin/env python3
"""The two freestanding mechanisms compile on every toolchain we ship — phase-442 W3.

WHAT IS BEING MEASURED

RFC-0096's claim is that one API can be freestanding AND be the rclcpp API, and
it rests on two mechanisms: a copyable non-owning handle behind `X::SharedPtr`,
and a fixed-capacity inplace callable behind a capturing-lambda callback. A
claim like that is worth exactly the compile that backs it, on the toolchains
that actually differ.

Three configurations, because each removes something different:

  hosted `g++ -std=c++17`                    the reference
  `arm-none-eabi -std=c++14 -ffreestanding`  no hosted library, 32-BIT
  ThreadX shim, `-nostdinc++`                our own minimal `<new>`/traits

The 32-bit arm is not redundant with the other two. Every capture size, the
capacity default and the storage alignment are expressed in pointers precisely
because a byte count is right on one word size and wrong on the other
(phase-442 W0), and only this arm can catch a regression to a literal.

THE NEGATIVE CONTROL IS THE POINT, NOT A FORMALITY

An over-budget capture must be a COMPILE ERROR NAMING THE KNOB. That is the
third item in RFC-0096 D5's list of what is not drop-in, and the design's whole
answer to "what if a capture does not fit": not a silent heap fallback, which
defeats the point on a target with no allocator, and not a runtime failure,
which moves a compile-time fact to the field.

So the probe must fail, and the DIAGNOSTIC TEXT is checked too. A probe that
merely fails passes on any error — a typo, a missing header, a renamed type —
and would go on "passing" after the `static_assert` it exists for was deleted.

WHY THIS IS A FAST-LANE GATE AND NOT PART OF `check cpp`

`check cpp` is `build-serial`, which no merge-gating event runs, so a gate
living there gates nothing — issue 1225 moved `check-cpp-capability-layout` out
for exactly this reason and named it. This needs no build: three `-fsyntax-only`
compiles of one tracked TU.

Usage::

    check-cpp-freestanding-mechanisms.py
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

PROBE = "packages/api/nros-cpp/tests/compile/freestanding_mechanisms.cpp"
OVERBUDGET = "packages/api/nros-cpp/tests/compile/freestanding_mechanisms_overbudget_probe.cpp"
KNOB = "NROS_CPP_CALLBACK_CAPACITY"

CPP_INCLUDE = "packages/api/nros-cpp/include"
PLATFORM_API = "packages/platform/nros-platform-api/include"
THREADX_SHIM = "packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat"

ARM_GXX = os.path.expanduser(
    "~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1/bin/arm-none-eabi-g++")


def arms():
    """(label, argv-prefix, required). A cross arm absent from this host is a
    reported SKIP, never a silent pass: the whole point of the arm is that it
    differs from the host, so "the host compiler was used instead" answers a
    different question."""
    out = [("hosted g++ -std=c++17",
            ["c++", "-std=c++17", "-fsyntax-only"], True)]

    arm = ARM_GXX if os.path.exists(ARM_GXX) else shutil.which("arm-none-eabi-g++")
    if arm:
        out.append(("arm-none-eabi -std=c++14 -ffreestanding (32-bit)",
                    [arm, "-std=c++14", "-ffreestanding", "-fno-exceptions", "-fno-rtti",
                     "-mcpu=cortex-m3", "-mthumb", "-fsyntax-only"], True))
    else:
        out.append(("arm-none-eabi (32-bit)", None, False))

    rv = shutil.which("riscv64-unknown-elf-g++") or "c++"
    out.append(("ThreadX shim -nostdinc++",
                [rv, "-std=c++14", "-ffreestanding", "-fno-exceptions", "-fno-rtti",
                 "-nostdinc++", "-isystem", os.path.join(ROOT, THREADX_SHIM),
                 "-I", os.path.join(ROOT, PLATFORM_API), "-fsyntax-only"], True))
    return out


def compile_tu(argv, tu):
    cmd = list(argv) + ["-I", os.path.join(ROOT, CPP_INCLUDE), os.path.join(ROOT, tu)]
    proc = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    return proc.returncode, proc.stdout + proc.stderr


def selftest(arm_label, argv):
    """The over-budget capture must FAIL, and fail for the stated reason.

    Runs on the NORMAL path, every invocation (phase-395). Returns a list of
    failures, empty when the probe was rejected with the knob named.
    """
    failures = []
    rc, out = compile_tu(argv, OVERBUDGET)
    if rc == 0:
        failures.append(
            "SELFTEST FAILED on %s: the over-budget capture COMPILED. The capacity "
            "`static_assert` is not firing, so an over-large capture would be a silent "
            "heap-free overwrite or a link-time surprise instead of a compile error." % arm_label)
    elif KNOB not in out:
        failures.append(
            "SELFTEST FAILED on %s: the over-budget capture failed, but the diagnostic does "
            "not name `%s`, so the failure is not the one this probe exists for. A probe "
            "that merely fails passes on a typo.\n%s" % (arm_label, KNOB, out.strip()[:800]))
    return failures


def main():
    for rel in (PROBE, OVERBUDGET):
        if not os.path.exists(os.path.join(ROOT, rel)):
            print("check-cpp-freestanding-mechanisms: %s is MISSING -- this gate would pass "
                  "on absence" % rel, file=sys.stderr)
            return 1

    failures = []
    ran = 0
    for label, argv, _required in arms():
        if argv is None:
            print("check-cpp-freestanding-mechanisms: %s SKIPPED -- toolchain absent" % label)
            ledger = os.environ.get("NROS_CHECK_SKIP_LEDGER")
            if ledger:
                with open(ledger, "a", encoding="utf8") as fh:
                    fh.write("check-cpp-freestanding-mechanisms: %s: toolchain absent\n" % label)
            continue

        rc, out = compile_tu(argv, PROBE)
        ran += 1
        if rc != 0:
            failures.append("%s: the mechanisms do NOT compile.\n%s" % (label, out.strip()))
            continue
        failures.extend(selftest(label, argv))

    if failures:
        print("", file=sys.stderr)
        for f in failures:
            print("FAIL: %s" % f, file=sys.stderr)
        print("\nRFC-0096 rests on these two mechanisms being constructible without the "
              "standard library. The probe is %s; its expected-failure half is %s."
              % (PROBE, OVERBUDGET), file=sys.stderr)
        return 1

    print("check-cpp-freestanding-mechanisms: OK -- the handle and the inplace callable compile "
          "on %d toolchain configuration(s), and an over-budget capture fails on each with a "
          "diagnostic naming %s" % (ran, KNOB))
    return 0


if __name__ == "__main__":
    sys.exit(main())
