#!/usr/bin/env python3
"""The FreeRTOS integration shell's configure-time diagnostics fire — phase-349 W4.

`integrations/freertos/NanoRosFreertos.cmake` promises RFC-0072 §7's "what
fails, and how" at CONFIGURE: a target the shell needs and the project did not
define, a capability the platform requires and the shell was not given, an
include directory that does not exist, and a config target with no
FreeRTOSConfig.h. Each is a message a host-project author meets instead of a
missing header deep in a build, so each is only worth anything if it FIRES.
This configures a small host project per case and checks the outcome and the
words: the failing cases must fail naming the thing, and the two passing cases
(`NETSTACK lwip`, `NETSTACK NONE`) must compose `freertos_platform`.

No compiler, no SDK: the project is `LANGUAGES NONE` and every target is an
INTERFACE library over directories the gate creates, so this is ~0.3 s a case
and belongs on the fast line. The in-tree boards reach the same function with
real targets on every FreeRTOS image; this is the part of the contract they
cannot exercise, because a board that tripped a diagnostic would not build.

Usage::

    check-freertos-shell-diagnostics.py
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SHELL = REPO / "integrations/freertos/NanoRosFreertos.cmake"

# Each case: (name, body appended after the targets, expect_ok, words the
# output must contain). `@T@` is the case's own scratch directory.
TARGETS = {
    "config": "add_library(freertos_config INTERFACE)\n"
    'target_include_directories(freertos_config INTERFACE "@T@/config")\n',
    "kernel": "add_library(freertos_kernel_include INTERFACE)\n"
    'target_include_directories(freertos_kernel_include INTERFACE "@T@/kernel/include")\n'
    "add_library(freertos_kernel INTERFACE)\n"
    "target_link_libraries(freertos_kernel INTERFACE freertos_kernel_include freertos_config)\n",
    "lwip": "add_library(lwip INTERFACE)\n"
    'target_include_directories(lwip INTERFACE "@T@/lwip/src/include")\n',
}

CASES = [
    (
        "netstack-lwip-composes",
        ["config", "kernel", "lwip"],
        "nros_freertos_integrate(NETSTACK lwip)\n",
        True,
        ["PLATFORM_OK"],
    ),
    (
        "netstack-none-composes",
        ["config", "kernel"],
        "nros_freertos_integrate(NETSTACK NONE)\n",
        True,
        ["PLATFORM_OK", "HAS_LWIP=FALSE"],
    ),
    (
        # Upstream's port-header table writes one genex over several list
        # elements; directories inside it belong to OTHER ports and are not
        # this build's. Reading them as plain entries reported eleven missing
        # dirs on the first real board configure.
        "multi-element-genex-is-not-read",
        ["config", "kernel", "lwip"],
        "target_include_directories(freertos_kernel_include INTERFACE\n"
        "    $<$<STREQUAL:${FREERTOS_PORT},SOME_OTHER_PORT>:\n"
        '        "@T@/kernel/portable/Other/A"\n'
        '        "@T@/kernel/portable/Other/B">)\n'
        "nros_freertos_integrate(NETSTACK lwip)\n",
        True,
        ["PLATFORM_OK"],
    ),
    (
        "missing-capability-names-ip-stack",
        ["config", "kernel"],
        "nros_freertos_integrate()\n",
        False,
        ["capability `ip_stack`", "nros-platform-freertos/nros-platform.toml", "NETSTACK"],
    ),
    (
        "missing-include-dir-is-named",
        ["config", "kernel", "lwip"],
        'target_include_directories(lwip INTERFACE "@T@/lwip/contrib/not-checked-out")\n'
        "nros_freertos_integrate(NETSTACK lwip)\n",
        False,
        ["@T@/lwip/contrib/not-checked-out", "exported by `lwip`"],
    ),
    (
        "missing-include-dir-reached-transitively",
        ["config", "kernel", "lwip"],
        'target_include_directories(freertos_kernel_include INTERFACE "@T@/kernel/portable/GONE")\n'
        "nros_freertos_integrate(NETSTACK lwip)\n",
        False,
        ["@T@/kernel/portable/GONE", "exported by `freertos_kernel_include`"],
    ),
    (
        "missing-kernel-target-is-named",
        ["config", "lwip"],
        "nros_freertos_integrate(NETSTACK lwip)\n",
        False,
        ["no `freertos_kernel` target"],
    ),
    (
        "missing-freertos-config-header-is-named",
        ["config", "kernel", "lwip"],
        'file(REMOVE "@T@/config/FreeRTOSConfig.h")\n'
        "nros_freertos_integrate(NETSTACK lwip)\n",
        False,
        ["no FreeRTOSConfig.h", "freertos_config"],
    ),
]

# phase-349 W3 — the port resolver the boards and the shell share. A fake
# kernel carries upstream's table shape (a header row per port, one wrapped) and
# a vendor port upstream does not list.
PORT_TABLE = """\
target_include_directories(freertos_kernel_port_headers INTERFACE
    $<$<STREQUAL:${FREERTOS_PORT},GCC_ARM_CM3>:${CMAKE_CURRENT_LIST_DIR}/GCC/ARM_CM3>
    $<$<STREQUAL:${FREERTOS_PORT},GCC_POSIX>:
        ${CMAKE_CURRENT_LIST_DIR}/ThirdParty/GCC/Posix
        ${CMAKE_CURRENT_LIST_DIR}/ThirdParty/GCC/Posix/utils>
)
"""
PORT_PROLOGUE = 'set(FREERTOS_DIR "@T@/kernel")\n'
PORT_CASES = [
    (
        "port-enum-resolves",
        PORT_PROLOGUE
        + "nros_freertos_port_dir(d PORT GCC_POSIX OUT_ENUM e)\n"
        + 'message(STATUS "DIR=${d} ENUM=${e}")\n',
        True,
        ["DIR=@T@/kernel/portable/ThirdParty/GCC/Posix ENUM=GCC_POSIX"],
        ["Deprecated"],
    ),
    (
        "port-listed-path-is-deprecated",
        PORT_PROLOGUE
        + "nros_freertos_port_dir(d PORT GCC/ARM_CM3 OUT_ENUM e)\n"
        + "nros_freertos_port_dir(d PORT GCC/ARM_CM3 OUT_ENUM e)\n"
        + 'message(STATUS "DIR=${d} ENUM=${e}")\n',
        True,
        ["DIR=@T@/kernel/portable/GCC/ARM_CM3 ENUM=GCC_ARM_CM3", "Write 'GCC_ARM_CM3'"],
        [],
    ),
    (
        "port-vendor-path-is-its-own-spelling",
        PORT_PROLOGUE
        + "nros_freertos_port_dir(d PORT GCC/VENDOR_R52 OUT_ENUM e)\n"
        + 'message(STATUS "DIR=${d} ENUM=${e}")\n',
        True,
        ["DIR=@T@/kernel/portable/GCC/VENDOR_R52 ENUM=A_CUSTOM_PORT"],
        ["Deprecated"],
    ),
    (
        "port-unknown-is-refused-naming-the-table",
        PORT_PROLOGUE + "nros_freertos_port_dir(d PORT GCC_ARM_CM99)\n",
        False,
        ["FREERTOS_PORT 'GCC_ARM_CM99'", "portable/CMakeLists.txt"],
        [],
    ),
]

TAIL = (
    "if(NOT TARGET freertos_platform)\n"
    '    message(FATAL_ERROR "composed no freertos_platform")\n'
    "endif()\n"
    'message(STATUS "PLATFORM_OK")\n'
    'message(STATUS "HAS_LWIP=${NROS_FREERTOS_BOARD_HAS_LWIP}")\n'
)


def project(scratch: Path, targets: list[str], body: str) -> str:
    text = "cmake_minimum_required(VERSION 3.22)\nproject(shell_probe LANGUAGES NONE)\n"
    for t in targets:
        text += TARGETS[t]
    text += f'include("{SHELL}")\n' + body + TAIL
    return text.replace("@T@", str(scratch))


def run_case(root: Path, name, targets, body, expect_ok, words) -> list[str]:
    scratch = root / name
    for d in ("config", "kernel/include", "lwip/src/include", "src"):
        (scratch / d).mkdir(parents=True)
    (scratch / "config/FreeRTOSConfig.h").write_text("/* probe */\n")
    (scratch / "src/CMakeLists.txt").write_text(project(scratch, targets, body))
    # copy-out-root-ok: a LANGUAGES NONE probe of one cmake module, not a
    # nano-ros consumer; it reads the shell by absolute path.
    proc = subprocess.run(
        ["cmake", "-S", str(scratch / "src"), "-B", str(scratch / "build")],
        capture_output=True,
        text=True,
    )
    out = proc.stdout + proc.stderr
    errors = []
    if (proc.returncode == 0) != expect_ok:
        want = "configure" if expect_ok else "fail"
        errors.append(f"{name}: expected the project to {want}, rc={proc.returncode}")
    for w in words:
        w = w.replace("@T@", str(scratch))
        # cmake wraps long message lines; compare with whitespace collapsed.
        if " ".join(w.split()) not in " ".join(out.split()):
            errors.append(f"{name}: output does not contain {w!r}")
    if errors:
        errors.append("    " + "\n    ".join(out.strip().splitlines()[-25:]))
    return errors


def run_port_case(root: Path, name, body, expect_ok, words, absent) -> list[str]:
    scratch = root / name
    for d in ("kernel/portable/GCC/ARM_CM3", "kernel/portable/ThirdParty/GCC/Posix",
              "kernel/portable/GCC/VENDOR_R52", "src"):
        (scratch / d).mkdir(parents=True)
    for d in ("GCC/ARM_CM3", "ThirdParty/GCC/Posix", "GCC/VENDOR_R52"):
        (scratch / "kernel/portable" / d / "port.c").write_text("")
    (scratch / "kernel/portable/CMakeLists.txt").write_text(PORT_TABLE)
    text = (
        "cmake_minimum_required(VERSION 3.22)\nproject(port_probe LANGUAGES NONE)\n"
        f'include("{SHELL}")\n' + body
    ).replace("@T@", str(scratch))
    (scratch / "src/CMakeLists.txt").write_text(text)
    # copy-out-root-ok: a LANGUAGES NONE probe of one cmake module.
    proc = subprocess.run(
        ["cmake", "-S", str(scratch / "src"), "-B", str(scratch / "build")],
        capture_output=True,
        text=True,
    )
    out = " ".join((proc.stdout + proc.stderr).split())
    errors = []
    if (proc.returncode == 0) != expect_ok:
        errors.append(f"{name}: rc={proc.returncode}, expected {'success' if expect_ok else 'failure'}")
    for w in words:
        w = " ".join(w.replace("@T@", str(scratch)).split())
        if w not in out:
            errors.append(f"{name}: output does not contain {w!r}")
    for w in absent:
        if w in out:
            errors.append(f"{name}: output contains {w!r}")
    # One deprecation per spelling per configure, however often it resolves.
    said = out.count("Write 'GCC_ARM_CM3'")
    if name == "port-listed-path-is-deprecated" and said != 1:
        errors.append(f"{name}: the deprecation was said {said} times, not once")
    if errors:
        errors.append("    " + out[-1500:])
    return errors


def self_test(root: Path) -> list[str]:
    """The checker itself can report a red: a project that FAILS, judged as one
    that must configure, and a word that is absent, must each be reported."""
    problems = []
    if not run_case(root, "selftest-wrong-outcome", ["config", "kernel"],
                    "nros_freertos_integrate()\n", True, []):
        problems.append("self-test: a failing configure judged as passing was not reported")
    if not run_case(root, "selftest-absent-word", ["config", "kernel", "lwip"],
                    "nros_freertos_integrate(NETSTACK lwip)\n", True,
                    ["a phrase no configure prints"]):
        problems.append("self-test: an absent expected word was not reported")
    return problems


def main() -> int:
    if shutil.which("cmake") is None:
        print("check-freertos-shell-diagnostics: cmake not on PATH", file=sys.stderr)
        return 1
    errors: list[str] = []
    with tempfile.TemporaryDirectory(prefix="nros-freertos-shell-") as tmp:
        errors += self_test(Path(tmp))
        for case in CASES:
            errors += run_case(Path(tmp), *case)
        for case in PORT_CASES:
            errors += run_port_case(Path(tmp), *case)
    if errors:
        print("check-freertos-shell-diagnostics: FAILED", file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        return 1
    print(f"check-freertos-shell-diagnostics: {len(CASES) + len(PORT_CASES)} cases OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
