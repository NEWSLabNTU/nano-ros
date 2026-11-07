#!/usr/bin/env python3
"""check-fixture-interfaces-opt-in — issue 2344.

Since issue 2278 the CMake road refuses to generate messages from nano-ros's
bundled interface copies unless the build names them. A fixture build on a
host with no ROS discoverable (the self-hosted tier-2 runner leaves
AMENT_PREFIX_PATH unset on purpose) means the bundled set, and says so through
ONE helper, `nros_fixture_interfaces` (scripts/build/fixture-interfaces.sh).

Two compile-check rows stopped every tier-2 run on that refusal, because the
fixture builders configure CMake and none of them made the choice. So: every
`scripts/build/*fixture*.sh` that CONFIGURES a build (`cmake … -S`/`-B`, `west
build`, `nros_cmake_configure…`, `nros … configure`) calls the helper, or is
listed below with the reason it need not.

Comments and string/heredoc contents are prose to this gate. `cmake -P` (a
script) and `cmake --build` (a configured tree) configure nothing. Buildless,
~0.1 s; self-tests its matcher, and an allowlist entry that no longer
configures is reported stale.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import comments  # noqa: E402
import git_grep  # noqa: E402  issue 1816 — a failed search is not "no match"

CONFIGURES = re.compile(
    r"(?:^|[\s;&|(])(?:cmake\b[^\n]*\s-[SB]\b"
    # `west build`, or `west "${args[@]}"`; a quoted command then `configure`
    # (the nros CLI held in a variable — the string is blanked before matching).
    r"|\bwest\s+(?:build\b|\")|nros_cmake_configure\w*|\"\s+configure\b)"
)
OPTS_IN = re.compile(r"(?:^|[\s;&|])nros_fixture_interfaces(?:\s|$)")

# path -> why it may configure without calling the helper.
ALLOWED = {
    "scripts/build/fixture-matrix.sh": (
        "a sourced function library; its caller (zephyr-fixture-leaves.sh) opts in "
        "before calling it"
    ),
}

SELF_TESTS = [
    # (text, configures, opts_in)
    ('if ! cmake -S "$src" -B "$bld"; then', True, False),
    ('    west build -b native_sim "$app"', True, False),
    ('    env "${toolchain_env[@]}" west "${args[@]}" || true', True, False),
    ('nros_cmake_configure_if_needed "$dir" "$dir/$sub"', True, False),
    ('"$NROS_CLI" configure --no-provider-index "$dir"', True, False),
    ('image="$(cmake --log-level=WARNING -P "$f")"', False, False),
    ('cmake --build "$build_dir"', False, False),
    ('nros_fixture_interfaces "$repo_root"', False, True),
    ("# cmake -S is mentioned in a comment", False, False),
]


def code_of(text: str) -> str:
    return comments.strip_comments(text, "sh", strings=True)


def configures(text: str) -> bool:
    return any(CONFIGURES.search(line) for line in code_of(text).splitlines())


def opts_in(text: str) -> bool:
    return any(OPTS_IN.search(line) for line in comments.strip_comments(text, "sh").splitlines())


def self_test() -> int:
    bad = 0
    for text, want_c, want_o in SELF_TESTS:
        got_c = configures(text)
        got_o = opts_in(text)
        if (got_c, got_o) != (want_c, want_o):
            bad += 1
            print(f"  self-test FAILED: {text!r} configures={got_c} opts_in={got_o}, "
                  f"want {want_c}/{want_o}")
    return bad


def main() -> int:
    if self_test():
        print("check-fixture-interfaces-opt-in --self-test: FAILED")
        return 1
    files = git_grep.run(["-lE", "cmake|west|configure", "--", "scripts/build/*fixture*.sh"],
                         ROOT).split()
    missing, seen_allowed = [], set()
    for f in files:
        text = (ROOT / f).read_text(errors="replace")
        if not configures(text):
            continue
        if f in ALLOWED:
            seen_allowed.add(f)
            continue
        if not opts_in(text):
            missing.append(f)
    stale = sorted(set(ALLOWED) - seen_allowed)
    rc = 0
    if missing:
        rc = 1
        print("check-fixture-interfaces-opt-in: a fixture builder configures a build without "
              "naming its interface packages (issue 2344):\n")
        for f in missing:
            print(f"  {f}")
        print("\nSource scripts/build/fixture-interfaces.sh and call "
              "`nros_fixture_interfaces \"$repo_root\"` before the first configure.")
    if stale:
        rc = 1
        print("check-fixture-interfaces-opt-in: stale allowlist entries (they no longer configure):")
        for f in stale:
            print(f"  {f}")
    if rc == 0:
        print(f"check-fixture-interfaces-opt-in: OK ({len(files)} fixture script(s) scanned)")
    return rc


if __name__ == "__main__":
    sys.exit(main())
