#!/usr/bin/env python3
"""check-cmake-build-dir-literals -- issue 2702.

Where `nros build` puts a CMake configure is ONE function, `cmake_coordinate`
(`packages/cli/nros-cli-core/src/cmd/build.rs`):
`build/<platform>-<rmw>[-<board>][-cfg<hash>]-<bringup>-<image>/cmake`.
A fixture row states the result once, as its `build_subdir` (a workspace row)
or `output` (a compile-check row), and a test asks the row
(`fixtures::groups::workspace_artifact_dir`, `require_cmake_fixture_output`).

When issue 2702 added the image to the coordinate, every path spelled by hand
outside that function and the rows kept the old layout: five test files, the
book probe, 26 resolver arguments. A stale one does not fail as "moved"; it
fails as "fixture not built", which reads as a missing build rather than a
wrong path.

This refuses such a literal in CODE (comments are prose, and the rule is about
paths a program uses) outside the allowlist below, and reports an allowlist
entry that no longer spells one. Markdown is not read: a book page or README
shows a user the path to run, which is documentation of the layout, not a
second owner of it.

Buildless, well under a second; self-tests its matcher on every run.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))

import comments  # noqa: E402
import git_grep  # noqa: E402  a failed search is not "no match"

# The platform families a cmake coordinate starts with.
FAMILIES = r"(?:posix|threadx|freertos|nuttx|bare-metal|native)"
PATTERN = re.compile(rf"build/{FAMILIES}-[\w.-]+/(?:cmake\b|CMakeLists\.txt)")

# path -> why it may spell the layout.
ALLOWED = {
    "packages/cli/nros-cli-core/src/cmd/build.rs": (
        "the ONE definition (`cmake_coordinate`) and its own unit tests"
    ),
    "examples/fixtures.toml": (
        "the rows state each fixture's dir once; "
        "`every_generated_workspace_fixture_row_names_its_cmake_build_dir` "
        "checks a workspace row against `cmake_coordinate`"
    ),
    "packages/cli/nros-cli-core/tests/build_verb_pipeline.rs": (
        "the CLI's contract test: it asserts the layout `nros build` writes and "
        "prints, so a layout move turns it red rather than stranding it"
    ),
    "packages/cli/nros-cli-core/src/cmd/build_next.rs": (
        "synthetic `build_dir` inputs to a unit test of the plan printer; "
        "no build reads them"
    ),
    "scripts/probe/verify-first-node.sh": (
        "the book probe checks the binary a user's `nros build` left on disk, and "
        "fails naming the path it expected, so a layout move is a loud red there"
    ),
    "scripts/check/check-cmake-build-dir-literals.py": "this gate's own pattern and self-tests",
}

SELF_TESTS = [
    # (text, lang, should_match)
    ('"build/posix-zenoh-linux-demo_bringup-linux/cmake/linux_entry",', "rust", True),
    ('bin=build/posix-cyclonedds-linux-demo_bringup-linux/cmake/linux_entry', "sh", True),
    ('.join("build/threadx-linux-zenoh-two_prog_bringup-threadx-linux/cmake")', "rust", True),
    ('"examples/workspaces/cpp/build/posix-zenoh-linux-pubsub_bringup/CMakeLists.txt"', "rust", True),
    ('build_subdir = "build/bare-metal-zenoh-mps2-an385-demo_bringup-bm/cmake"', "toml", True),
    ('set(B "${CMAKE_SOURCE_DIR}/build/nuttx-zenoh-qemu-armv7a-nuttx-x-y/cmake")', "cmake", True),
    ('// `build/posix-zenoh-linux-demo_bringup/cmake` was the shared tree', "rust", False),
    ('# (`build/posix-zenoh-linux/cmake`), one tree', "toml", False),
    ('let d = groups::workspace_artifact_dir("workspace-cpp-native-talker")?;', "rust", False),
    ('"build/zephyr-workspace-builds/3.7/build-ws-c-entry-zenoh/zephyr/zephyr.exe"', "rust", False),
    ('"build/moved-here/cmake"', "sh", False),
]


def hits(text: str, lang: str) -> list[int]:
    code = comments.strip_comments(text, lang)
    return [n for n, line in enumerate(code.splitlines(), 1) if PATTERN.search(line)]


def self_test() -> int:
    bad = 0
    for text, lang, want in SELF_TESTS:
        got = bool(hits(text, lang))
        if got != want:
            bad += 1
            print(f"  self-test FAILED: {text!r} ({lang}) matched={got}, want {want}")
    return bad


def main() -> int:
    if self_test():
        print("check-cmake-build-dir-literals --self-test: FAILED")
        return 1
    files = git_grep.run(
        ["-lIP", rf"build/{FAMILIES}-[\w.-]+/(?:cmake\b|CMakeLists\.txt)", "--",
         ":!docs", ":!*.md", ":!book", ":!book-internals", ":!third-party",
         ":!CHANGELOG.md", ":!changelog.d"],
        ROOT,
    ).split()
    found = []
    seen_allowed = set()
    for f in files:
        lang = comments.lang_for(f)
        if lang is None:
            continue
        try:
            text = (ROOT / f).read_text(errors="replace")
        except OSError:
            continue
        lines = hits(text, lang)
        if not lines:
            continue
        if f in ALLOWED:
            seen_allowed.add(f)
            continue
        found += [f"{f}:{n}" for n in lines]
    stale = sorted(set(ALLOWED) - seen_allowed - {"scripts/check/check-cmake-build-dir-literals.py"})
    rc = 0
    if found:
        rc = 1
        print("check-cmake-build-dir-literals: a cmake build dir is SPELLED:\n")
        for h in found:
            print(f"  {h}")
        print(
            "\nAsk the fixture row instead -- a workspace row:"
            " nros_tests::fixtures::groups::workspace_artifact_dir(<id>);"
            "\na compile-check row: nros_tests::fixtures::require_cmake_fixture_output(<id>);"
            "\nthe CLI: cmake_coordinate / cmake_build_dir_for (cmd/build.rs)."
        )
    if stale:
        rc = 1
        print("check-cmake-build-dir-literals: allowlist entries that no longer spell one "
              "(delete them):")
        for f in stale:
            print(f"  {f}")
    if rc == 0:
        print(f"check-cmake-build-dir-literals: OK ({len(SELF_TESTS)} self-test cases; "
              f"{len(ALLOWED)} allowlisted)")
    return rc


if __name__ == "__main__":
    sys.exit(main())
