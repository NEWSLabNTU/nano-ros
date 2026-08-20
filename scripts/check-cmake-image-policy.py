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

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Produces an image: an executable target that links the nano-ros umbrella.
MAKES_EXE = re.compile(r"\b(add_executable|ament_auto_add_executable)\s*\(", re.I)
LINKS_NROS = re.compile(r"NanoRos::NanoRos(Cpp)?\b")

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
    "cmake/compat/NrosRclcppCompat.cmake": "alias layer, included by every consumer",
    "cmake/compat/stubs/Findrclcpp.cmake": "find-module stub for the alias layer",
    # The package config: it is how a consumer REACHES the verbs, not an image
    # path of its own.
    "nano_rosConfig.cmake": "package config, not an image path",
}


def strip_comments(text):
    """cmake comments only. The whole point of the gate is not to read prose."""
    return re.sub(r"(?m)#.*$", "", text)


def cmake_files():
    """TRACKED cmake files only.

    `git ls-files` rather than a walk: build trees and the scratch `tmp/` carry
    generated and throwaway CMakeLists that are not the project's to fix, and a
    gate that reports them teaches people to skim its output. It also means a
    new build-output directory cannot quietly enter the gate's scope.
    """
    import subprocess

    out = subprocess.run(
        ["git", "ls-files", "-z", "*.cmake", "CMakeLists.txt", "*/CMakeLists.txt"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    return [p for p in out.stdout.split("\0") if p]


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
        if not (MAKES_EXE.search(body) and LINKS_NROS.search(body)):
            continue
        if APPLIES.search(body):
            continue
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
    ]
    for body, should_flag, label in cases:
        stripped = strip_comments(body)
        flagged = bool(
            MAKES_EXE.search(stripped)
            and LINKS_NROS.search(stripped)
            and not APPLIES.search(stripped)
        )
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
