#!/usr/bin/env python3
"""issue 1570 — a build script that compiles sources HANDED TO IT must declare
what the compiler read, not the list it was handed.

# The defect this gate exists for

The NuttX image is linked by a cargo build script
(`nros-board-common::nuttx_ffi_build`) that compiles every component TU of the
image from env LISTS (`APP_EXTRA_SOURCES`, `APP_INTERFACE_SOURCES`). It
declared `rerun-if-env-changed` on the lists and nothing for the files in them
or the headers they include, so an edit to a component `.c`, to
`component.h`, or to the committed NuttX config snapshot left cargo's
fingerprint unchanged. Measured: the ctrl component instance read 0x2d8 in the
object cmake had just rebuilt and 0x288 in the linked image, with the build
reporting success — a museum binary, issue 0475's class one lane over.

The fix is `nros_cc_flags::header_deps`: `track_header_deps(&mut build)` adds
`-MMD` so the compiler writes a depfile per object, and
`emit_header_deps(out_dir)` replays every name in those depfiles as
`cargo:rerun-if-changed` after the compile. Cargo copies those into the
artifact's own dep-info, which the cmake rule consumes as its DEPFILE, so the
one list is also the edge that re-runs cargo at all.

# The two rules

  1. REACH. A tracked Rust source that compiles with cc-rs (`.compile(`) and
     takes a source list from an environment variable whose name contains
     `SOURCES` must call BOTH helpers. Nobody hand-listing headers can match
     what the compiler opens, and a list from the environment is by
     construction a set of files the crate does not own and cannot name.
  2. PAIRING. In any file, `track_header_deps` without `emit_header_deps` is a
     `-MMD` whose output nobody reads (it looks like coverage and declares
     nothing); `emit_header_deps` without `track_header_deps` has no depfile to
     read. Both are refused. (`emit_header_deps` also panics at build time when
     it finds no depfile, so the runtime half fails closed too.)

Rule 1 must MATCH something: zero matching files means the pattern drifted
away from the tree, and a gate scanning nothing reports OK forever (issue
1220's shape), so that is a failure too. The classifier self-tests on every
run against a violating and a compliant snippet.

Run: python3 scripts/check/check-cc-header-deps.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

COMPILES = re.compile(r"\.compile\(")
ENV_SOURCE_LIST = re.compile(r'env::var\(\s*"([A-Z0-9_]*SOURCES[A-Z0-9_]*)"\s*\)')
TRACK = re.compile(r"\btrack_header_deps\s*\(")
EMIT = re.compile(r"\bemit_header_deps\s*\(")
# The helper's own definitions are not call sites.
DEFINITION = re.compile(r"\bpub fn (track|emit)_header_deps\b")


def strip_comments(text: str) -> str:
    """Drop `//` line comments so prose naming a helper earns no credit."""
    return "\n".join(line.split("//", 1)[0] for line in text.splitlines())


def classify(text: str):
    """Return a list of (rule, detail) violations for one file's text."""
    code = strip_comments(text)
    if DEFINITION.search(code):
        return []
    has_track = bool(TRACK.search(code))
    has_emit = bool(EMIT.search(code))
    out = []
    lists = sorted(set(ENV_SOURCE_LIST.findall(code)))
    if COMPILES.search(code) and lists and not (has_track and has_emit):
        out.append(
            (
                "reach",
                f"compiles sources from env {', '.join(lists)} without "
                "track_header_deps + emit_header_deps",
            )
        )
    elif has_track != has_emit:
        missing = "emit_header_deps" if has_track else "track_header_deps"
        out.append(("pairing", f"calls one header_deps helper without {missing}"))
    return out


def reached(text: str) -> bool:
    code = strip_comments(text)
    return bool(COMPILES.search(code) and ENV_SOURCE_LIST.search(code))


def self_test():
    bad = (
        'let s = env::var("APP_EXTRA_SOURCES").unwrap();\n'
        "b.file(&s);\n"
        'b.compile("app");\n'
    )
    good = (
        'let s = env::var("APP_EXTRA_SOURCES").unwrap();\n'
        "nros_cc_flags::header_deps::track_header_deps(&mut b);\n"
        'b.compile("app");\n'
        "nros_cc_flags::header_deps::emit_header_deps(&out);\n"
    )
    half = "track_header_deps(&mut b);\nb.compile(\"x\");\n"
    prose = (
        'let s = env::var("APP_EXTRA_SOURCES").unwrap();\n'
        "// track_header_deps(&mut b); emit_header_deps(&out);\n"
        'b.compile("app");\n'
    )
    cases = [
        ("violating snippet", bad, ["reach"]),
        ("compliant snippet", good, []),
        ("track without emit", half, ["pairing"]),
        ("helpers named only in a comment", prose, ["reach"]),
    ]
    for name, text, want in cases:
        got = [rule for rule, _ in classify(text)]
        if got != want:
            sys.stderr.write(
                f"check-cc-header-deps: SELF-TEST FAILED on {name}: "
                f"expected {want}, got {got}\n"
            )
            sys.exit(2)


def tracked_rust_sources():
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "-z", "--", "*.rs"],
        check=True,
        capture_output=True,
    ).stdout.decode()
    return [
        p
        for p in out.split("\0")
        if p and "/third-party/" not in p and not p.startswith("third-party/")
    ]


def main():
    self_test()
    sources = tracked_rust_sources()
    bad = []
    n_reached = 0
    for rel in sources:
        try:
            text = open(os.path.join(ROOT, rel), encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        if reached(text):
            n_reached += 1
        for rule, detail in classify(text):
            bad.append((rel, rule, detail))
    if n_reached == 0:
        sys.stderr.write(
            "check-cc-header-deps: rule 1 is VACUOUS — no tracked Rust source "
            "compiles with cc-rs from an env `*SOURCES*` list.\n"
            "  The NuttX image link (`nuttx_ffi_build.rs`) did when this gate\n"
            "  was written. If it moved or was renamed, widen ENV_SOURCE_LIST;\n"
            "  never leave the gate scanning nothing (issue 1220).\n"
        )
        sys.exit(1)
    if bad:
        sys.stderr.write(
            "check-cc-header-deps: a cc-rs compile with no rebuild edge to what "
            "it reads (issue 1570).\n\n"
        )
        for rel, rule, detail in bad:
            sys.stderr.write(f"  {rel}: [{rule}] {detail}\n")
        sys.stderr.write(
            "\n  cc-rs tells cargo nothing about the headers a TU includes, and\n"
            "  a source LIST from the environment names files the crate does\n"
            "  not own. Declare what the compiler actually opened:\n"
            "      nros_cc_flags::header_deps::track_header_deps(&mut build);\n"
            "      build.compile(\"x\");\n"
            "      nros_cc_flags::header_deps::emit_header_deps(&out_dir);\n"
        )
        sys.exit(1)
    print(
        f"check-cc-header-deps: OK ({len(sources)} tracked Rust source(s), "
        f"{n_reached} compiling an env source list)"
    )


if __name__ == "__main__":
    main()
