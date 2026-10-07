#!/usr/bin/env python3
"""issue 0719 — a cmake path that produces an image applies the image's policies.

`nano_ros_entry()` is where an image's cross-cutting facts get applied — today
the panic policy, and whatever is added next — and `nano_ros_add_executable()`
delegates to it, so ~160 call sites are covered by construction. A handful of
paths cannot go through the entry: a board seam, an ESP-IDF component, the NuttX
platform file. Those build systems own the image, and the entry is
entry-package shaped (NAME/BOARD/LAUNCH/MODEL/BRINGUP).

Twice those paths were found the hard way, each time as `#[panic_handler]
required, but not found` four crates from its cause — #0688 on the riscv64 board
seam, #0700 on the ESP-IDF shim, a day apart. Neither was a new bug: both had
been fine until something upstream changed how `nros-c` is imported.

So the rule this gate enforces: **if a cmake file links `NanoRos::NanoRos*` into
an executable, it calls `nros_apply_panic_policy`** (directly, or via
`nano_ros_entry` / `nano_ros_add_executable`, which do it for you).

# Why it keys on a CALL, not on a name

Issue 0719 recorded the trap first-hand: a mechanical grep for "goes through the
shared verb" EXCLUDED the ESP-IDF shim, because a COMMENT in that file mentioned
`nano_ros_entry()`. A gate that matches a name in prose reports a clean sweep
over a site it never examined — issue 0196's rule.

So comments are stripped before anything is matched, and the match is the call
form `name(` rather than the bare name. `check-image-panic-policy.py` is the
Rust-side sibling and says outright that it cannot see "the C/C++ side, where
the policy is a cargo feature on the staticlib"; this is that side.

Run: python3 scripts/check-cmake-image-policy.py
"""

import os
import re
import sys
import sys as _w3_sys  # noqa: E402
from pathlib import Path as _W3Path  # noqa: E402
_w3_sys.path.insert(0, str(_W3Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Produces an image: an executable target that links the nano-ros umbrella.
# issue 1735 — and an ESP-IDF component registration, which is how the IDF shim
# makes its image (the retired `check-image-paths-apply-policy.sh` read it; this
# gate did not).
MAKES_EXE = re.compile(
    r"\b(add_executable|ament_auto_add_executable|idf_component_register)\s*\(", re.I)
# issue 1614 (W5): an image is also one that carries the Rust runtime without
# the umbrella target — `nros_threadx_rv64_rust_app` links its staticlib and
# declares the carrier, and deleting its policy call passed.
# issue 1735 — and one that links the umbrella TRANSITIVELY through a
# generated message library (`<pkg>__nano_ros_c` / `<pkg>__nano_ros_cpp`, which
# PUBLIC-links the runtime umbrella, issue 1467): deleting the policy call from
# `examples/templates/rclcpp-compat-smoke` passed, because image detection keyed
# on the LITERAL `NanoRos::NanoRos(Cpp)`.
LINKS_NROS = re.compile(
    r"NanoRos::NanoRos(Cpp)?\b|\bnros_declare_rust_runtime_carrier\s*\("
    r"|\b[A-Za-z0-9_${}]+__nano_ros_(?:c|cpp)\b")

# Per-image seams that build an image OUTSIDE `nano_ros_entry()`, so the seam
# itself must apply the policy. Each row states why; a row whose file stops
# defining a link seam is stale and fails.
REQUIRED_SEAMS = {
    "cmake/platform/nano-ros-nuttx.cmake":
        "NuttX images do not go through nano_ros_entry(): NuttX's apps build calls "
        "nros_platform_link_app per target (issue 0719)",
}
SEAM_DEF = re.compile(r"\bfunction\s*\(\s*nros_(?:platform|board)_link_app\b")

# Applies the policy — as a CALL. `nano_ros_entry` / `nano_ros_add_executable`
# apply it for their callers, so either satisfies the rule.
APPLIES = re.compile(
    r"\b(nros_apply_panic_policy|nano_ros_entry|nano_ros_add_executable)\s*\(", re.I
)

# Paths that link the umbrella but must NOT claim an ending, with the reason.
EXEMPT = {
    # An alias layer (`rclcpp::rclcpp` -> `NanoRos::NanoRosCpp`) that
    # `nano_rosConfig.cmake` includes for EVERY consumer, image or not. Applying
    # here would impose a policy on builds that never link an image, and would
    # FATAL against an entry that legitimately chose a different ending — the
    # applier treats a second, different policy as a contradiction because the
    # staticlib is shared. The images this shim serves apply it themselves.
    "cmake/NanoRosAmentSurface.cmake": "alias layer, included by every consumer",
    "cmake/find/Findrclcpp.cmake": "find module for the alias layer",
    # The package config: it is how a consumer REACHES the verbs, not an image
    # path of its own.
    "nano_rosConfig.cmake": "package config, not an image path",
}


def strip_comments(text):
    """cmake comments only. The whole point of the gate is not to read prose."""
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, "cmake")


def cmake_files():
    """TRACKED cmake files only.

    `git ls-files` rather than a walk: build trees and the scratch `tmp/` carry
    generated and throwaway CMakeLists that are not the project's to fix, and a
    gate that reports them teaches people to skim its output. It also means a
    new build-output directory cannot quietly enter the gate's scope.
    """
    import subprocess

    import file_kinds  # issue 1614 (W5): the KIND, one definition of "CMake file"

    return file_kinds.files_of_kind("cmake", repo=ROOT)


SCOPE = re.compile(r"\b(function|macro)\s*\((.*?)\n(.*?)\bend\1\s*\(", re.S | re.I)


def scopes(body):
    """Each function()/macro() body, plus the top level with those removed.

    issue 1735 — folded in from the retired `check-image-paths-apply-policy.sh`
    (one rule, issue 0719, had two gates with two populations; issue 1614's W5
    fix landed on one only). A file whose top level applies the policy can
    still hold a function that builds a SECOND image without it.
    """
    out, top, pos = [], [], 0
    for m in SCOPE.finditer(body):
        top.append(body[pos:m.start()])
        out.append(m.group(0))
        pos = m.end()
    top.append(body[pos:])
    return out + ["".join(top)]


# A scope that hands its image to a link SEAM delegates the policy to the seam;
# whether that seam applies it is REQUIRED_SEAMS' question, not this scope's
# (and an open one for most seams — issue 1742).
SEAM_CALL = re.compile(r"\bnros_(?:platform|board)_link_app\s*\(")


def flags(body):
    """Does this (comment-stripped) cmake body build an image with no policy?"""
    if MAKES_EXE.search(body) and LINKS_NROS.search(body) and not APPLIES.search(body):
        return True
    return any(MAKES_EXE.search(sc) and LINKS_NROS.search(sc)
               and not APPLIES.search(sc) and not SEAM_CALL.search(sc)
               for sc in scopes(body))


def offenders():
    out = []
    for rel in cmake_files():
        path = os.path.join(ROOT, rel)
        if rel in EXEMPT:
            continue
        try:
            body = strip_comments(open(path, encoding="utf-8").read())
        except (OSError, UnicodeDecodeError):
            continue
        if rel in REQUIRED_SEAMS:
            if not SEAM_DEF.search(body):
                out.append(f"{rel} (REQUIRED_SEAMS row is stale: no link seam defined here)")
            elif not APPLIES.search(body):
                out.append(rel)
            continue
        if flags(body):
            out.append(rel)
    return sorted(out)


def self_test():
    """Both directions, including the prose trap that motivated the gate."""
    bad = []
    cases = [
        # (body, should_flag, label)
        ('add_executable(a x.c)\ntarget_link_libraries(a NanoRos::NanoRos)\n', True,
         "image path with no policy"),
        ('add_executable(a x.c)\ntarget_link_libraries(a NanoRos::NanoRos)\n'
         'nros_apply_panic_policy(platform "x")\n', False, "applies it directly"),
        ('nano_ros_add_executable(a SOURCES x.c)\ntarget_link_libraries(a NanoRos::NanoRos)\n',
         False, "delegates via the verb"),
        # THE trap: the name appears, but only in prose.
        ('# this used to go through nano_ros_entry()\n'
         'add_executable(a x.c)\ntarget_link_libraries(a NanoRos::NanoRos)\n', True,
         "name in a COMMENT must not satisfy the rule"),
        ('add_library(a x.c)\ntarget_link_libraries(a NanoRos::NanoRos)\n', False,
         "library, not an image"),
        ('add_executable(a x.c)\ntarget_link_libraries(a other::thing)\n', False,
         "executable that does not link nano-ros"),
        # issue 1735 — the transitive link through a generated message library.
        ('ament_auto_add_executable(a x.cpp)\ntarget_link_libraries(a PRIVATE std_msgs__nano_ros_cpp)\n',
         True, "links the umbrella through <msg>__nano_ros_cpp"),
        ('ament_auto_add_executable(a x.cpp)\ntarget_link_libraries(a PRIVATE std_msgs__nano_ros_cpp)\n'
         'nros_apply_panic_policy(platform "t")\n', False, "transitive link, policy applied"),
        # issue 1735 — the ESP-IDF component shape.
        ('idf_component_register(SRCS a.c)\ntarget_link_libraries(${COMPONENT_LIB} NanoRos::NanoRos)\n',
         True, "an ESP-IDF component image with no policy"),
        # issue 1735 — a function building a SECOND image beside a top level that applies.
        ('function(make_img n)\n  add_executable(${n} x.c)\n  target_link_libraries(${n} NanoRos::NanoRos)\n'
         'endfunction()\nadd_executable(b y.c)\ntarget_link_libraries(b NanoRos::NanoRos)\n'
         'nros_apply_panic_policy(platform "b")\n', True, "an unpolicied image in a function scope"),
    ]
    for body, should_flag, label in cases:
        flagged = flags(strip_comments(body))
        if flagged != should_flag:
            bad.append(f"self-test: {label!r} -> flagged={flagged}, expected {should_flag}")
    if bad:
        for b in bad:
            sys.stderr.write(b + "\n")
        sys.exit(2)
    print(f"check-cmake-image-policy --self-test: OK ({len(cases)} case(s))")


def main():
    self_test()
    bad = offenders()
    if bad:
        sys.stderr.write(
            "check-cmake-image-policy: FAILED — image path(s) that apply no ending:\n\n"
        )
        for rel in bad:
            sys.stderr.write(f"  {rel}\n")
        sys.stderr.write(
            "\n  Each links `NanoRos::NanoRos*` into an executable without going\n"
            "  through `nano_ros_entry()` / `nano_ros_add_executable()`, so the\n"
            "  image's cross-cutting facts never arrive (issue 0719). Add:\n\n"
            '      nros_apply_panic_policy(platform "<this path>")\n\n'
            "  A path that genuinely must not claim an ending goes in EXEMPT with\n"
            "  its reason — an alias layer every consumer includes is not an image.\n"
        )
        return 1
    print("cmake image policy: OK (every image path applies an ending)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
