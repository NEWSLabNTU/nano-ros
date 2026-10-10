#!/usr/bin/env python3
"""check-artifact-path-literals — phase-484 W6b (RFC-0103 D7).

Where a cargo-built nano-ros artifact lands (`libnros_c.a`, `libnros_cpp.a`,
the per-build `nros-{c,cpp}-generated/` config headers) is ONE function:
`nros_build_paths::artifact`, joined with cargo's own target dir. Rust calls
it; cmake asks `nros_locate_artifact()`, shell `nros_locate_artifact`, make
`$(call nros_artifact,…)` — all three print `nros locate artifact`.

The spelling this replaces was `<root>/target/...` — PX4's cmake module
(`${NANO_ROS_ROOT}/target/release/libnros_cpp.a`), NuttX's `Make.defs`, two
fixture scripts. Each is right only while nothing sets `CARGO_TARGET_DIR` or
`[build] target-dir`; under either the archive cargo just built reads "not
found", and a header beside it is another build's (the 1046/1050 pairing
class, one level up).

This refuses a NEW spelling in code (comments are prose, and the rule is about
paths a program uses). A `target/` + artifact name in CODE outside the path
function and its twins fails; an allowlisted file names its reason.

Buildless, ~0.2 s; self-tests its matcher on every run.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import comments  # noqa: E402

# `target/` (any prefix) followed by an artifact this module owns.
PATTERN = re.compile(
    r"target/(?:[\w.${}()-]+/){0,2}?(?:nros-c(?:pp)?-generated\b|libnros_c(?:pp)?\.a\b)"
)

# path -> why it may spell the layout.
ALLOWED = {
    "scripts/check/check-artifact-path-literals.py": "this gate's own pattern and self-tests",
    "scripts/check-px4-archive-header-pairing.sh": (
        "synthetic consumer snippets the pairing scan is self-tested against; "
        "they are inputs to a matcher, never a path a build reads"
    ),
    "packages/cli/rosidl-codegen/tests/heap_compile_check.rs": (
        "a test in the packages/cli workspace reading the ROOT workspace's "
        "header; that workspace has no edge to nros-build-paths, and adding one "
        "moves its lockfile — routed when it gains one"
    ),
}

# Files whose language `comments` cannot strip, read as hash-comment text.
HASH_NAMES = {"Make.defs", "Makefile"}

SELF_TESTS = [
    # (text, lang, should_match)
    ('set(A "${NANO_ROS_ROOT}/target/release/libnros_cpp.a")', "cmake", True),
    ('CFLAGS += -I$(ROOT)/target/nros-c-generated', "sh", True),
    ('lib="$repo_root/target/debug/libnros_c.a"', "sh", True),
    ('let d = root.join("target/nros-cpp-generated");', "rust", True),
    ('x = "$t/target/armv7a-nuttx-eabihf/release/libnros_c.a"', "sh", True),
    ('# `<root>/target/nros-c-generated` is what this replaced', "sh", False),
    ('// target/release/libnros_cpp.a, prose', "rust", False),
    ('lib="$(nros_locate_artifact c-archive --profile dev)"', "sh", False),
    ('let p = target.join("release/libfoo.a");', "rust", False),
    ('cargo build --target-dir target-xrce', "sh", False),
]


def lang_of(path: str) -> str | None:
    name = Path(path).name
    if name in HASH_NAMES:
        return "sh"
    return comments.lang_for(path)


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
        print("check-artifact-path-literals --self-test: FAILED")
        return 1
    files = subprocess.run(
        ["git", "grep", "-lIE", r"target/.*(nros-c(pp)?-generated|libnros_c(pp)?\.a)", "--",
         ":!docs", ":!*.md", ":!book", ":!third-party"],
        cwd=ROOT, capture_output=True, text=True, check=False,
    ).stdout.split()
    found = []
    seen_allowed = set()
    for f in files:
        lang = lang_of(f)
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
    stale = sorted(set(ALLOWED) - seen_allowed - {"scripts/check/check-artifact-path-literals.py"})
    rc = 0
    if found:
        rc = 1
        print("check-artifact-path-literals: a cargo-built artifact path is SPELLED:\n")
        for h in found:
            print(f"  {h}")
        print(
            "\nAsk where it lands instead — Rust: nros_build_paths::artifact::Artifact;"
            "\ncmake: nros_locate_artifact(<out> <name>) (cmake/NanoRosLocate.cmake);"
            "\nshell: nros_locate_artifact <name> (scripts/build/cargo.sh);"
            "\nmake:  $(call nros_artifact,<name>,<args>) (integrations/nuttx/Make.defs)."
        )
    if stale:
        rc = 1
        print("check-artifact-path-literals: allowlist entries that no longer spell one "
              "(delete them):")
        for f in stale:
            print(f"  {f}")
    if rc == 0:
        print(f"check-artifact-path-literals: OK ({len(SELF_TESTS)} self-test cases; "
              f"{len(ALLOWED)} allowlisted)")
    return rc


if __name__ == "__main__":
    sys.exit(main())
