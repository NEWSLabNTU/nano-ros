#!/usr/bin/env python3
"""issues 1570 + 1580 — a build script's cc-rs compile must declare what the
compiler READ, not the list of files its author could name.

# The defect this gate exists for

cc-rs compiles whatever it is handed and tells cargo nothing about it. A build
script therefore had to declare its inputs by hand, and it can only name what
it knows: a source, a config header, a directory. Never the headers those
sources include — only the compiler knows that set.

Issue 1570 measured the loud case: the NuttX image link
(`nros-board-common::nuttx_ffi_build`) compiled component TUs from an env LIST
and watched the list, so an edit to a component `.c` or `component.h` left the
image a museum binary. Issue 1580 measured the quiet one in every board crate:
`touch`ing the family `FreeRTOSConfig.h` — which each board copy `#include`s by
relative path — left `nros-board-freertos` Fresh, and the hand lists were wrong
as well as short (`freertos_task_glue.c`, the riscv64 `hwtimer.c`, the whole
virtio-net driver and NetX Duo, and every ThreadX kernel source had no edge).

The fix is `nros_cc_flags::header_deps`: `track_header_deps(&mut build)` adds
`-MMD` (and pins the compiler so sccache cannot eat the depfile on a cache
hit), and `emit_header_deps(out_dir)` replays every name in those depfiles as
`cargo:rerun-if-changed` after the compile.

# The rules

  1. REACH. Every tracked Rust source that calls `.compile(` or
     `.try_compile(` (a cc-rs compile — build scripts and the build-script
     libraries they call) must call `emit_header_deps`, and must call
     `track_header_deps` at least once per compile site. A file whose builds
     are tracked through ONE shared configure closure says so in
     `TRACKED_BY_CONFIGURATOR`, with the reason; a file that legitimately
     cannot (or has not yet) adopted the pair is in `EXEMPT`, with the reason
     and, if the reason is "not yet", a tracked issue id.
  2. PAIRING. `emit_header_deps` without `track_header_deps` has no depfile
     to read. `track_header_deps` in a file that compiles nothing is a `-MMD`
     whose output nobody in that file reads; it is allowed only for a helper
     that configures a build its CALLER compiles, listed in `TRACK_HELPERS`.
     (`emit_header_deps` also panics at build time when it finds no depfile,
     so the runtime half fails closed too.)
  3. NO STALE ENTRIES. Every path in the three tables must exist and must
     still be in the shape that earned the entry; an exemption for a file
     that no longer compiles is an exemption nobody can audit.

Rule 1 must MATCH something: zero compiling files means the pattern drifted
away from the tree, and a gate scanning nothing reports OK forever (issue
1220's shape). The classifier self-tests on every run.

Run: python3 scripts/check/check-cc-header-deps.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

COMPILES = re.compile(r"\.(?:try_)?compile\(")
TRACK = re.compile(r"\btrack_header_deps\s*\(")
EMIT = re.compile(r"\bemit_header_deps\s*\(")
# The helper's own definitions are not call sites.
DEFINITION = re.compile(r"\bpub fn (track|emit)_header_deps\b")

# path -> reason. Every build here goes through one configure closure/fn that
# calls `track_header_deps`, so the per-site count does not apply.
TRACKED_BY_CONFIGURATOR = {
    "packages/boards/nros-board-common/src/nuttx_ffi_build.rs": (
        "all five cc::Builds (app_cpp, app_c, app_pkg_N, app_iface_c, "
        "app_iface_cpp) are built through the `configure` closure, which "
        "calls track_header_deps"
    ),
}

# path -> reason. Tracks a build that its CALLER compiles; the caller is
# checked by rule 1.
TRACK_HELPERS = {
    "packages/boards/nros-board-common/src/threadx_sources.rs": (
        "`add_nros_platform_threadx_build` configures the platform-port build "
        "that nros-board-threadx/build.rs compiles and emits for"
    ),
}

# path -> reason. A cc-rs compile with no depfile edge, on purpose.
EXEMPT = {
    "packages/rmw/zenoh/nros-zpico-build/src/runner.rs": (
        "issue 1599 (open): adopting the pair bypasses sccache for zenoh-pico, "
        "the largest C compile in every image group; that cost is to be "
        "measured and decided there. Until then the runner keeps its hand "
        "watches (the vendored library tree by directory, its own c/ files)"
    ),
}


def strip_comments(text: str) -> str:
    """Drop `//` line comments so prose naming a helper earns no credit."""
    return "\n".join(line.split("//", 1)[0] for line in text.splitlines())


def counts(text: str):
    code = strip_comments(text)
    return (
        len(COMPILES.findall(code)),
        len(TRACK.findall(code)),
        len(EMIT.findall(code)),
        bool(DEFINITION.search(code)),
    )


def classify(text: str, rel: str = "<snippet>"):
    """Return a list of (rule, detail) violations for one file's text."""
    n_compile, n_track, n_emit, is_definition = counts(text)
    if is_definition:
        return []
    out = []
    if n_compile:
        if rel in EXEMPT:
            return []
        if not n_emit:
            out.append(
                (
                    "reach",
                    f"{n_compile} cc-rs compile site(s) and no emit_header_deps",
                )
            )
        if n_track < n_compile and rel not in TRACKED_BY_CONFIGURATOR:
            out.append(
                (
                    "reach",
                    f"{n_compile} cc-rs compile site(s) but {n_track} "
                    "track_header_deps call(s) — a build without -MMD declares "
                    "nothing",
                )
            )
    else:
        if n_emit and not n_track:
            out.append(("pairing", "emit_header_deps without track_header_deps"))
        if n_track and not n_emit and rel not in TRACK_HELPERS:
            out.append(
                (
                    "pairing",
                    "track_header_deps in a file that compiles nothing and "
                    "emits nothing (list it in TRACK_HELPERS if a caller "
                    "compiles this build)",
                )
            )
    return out


def self_test():
    good = (
        "nros_cc_flags::header_deps::track_header_deps(&mut b);\n"
        'b.compile("app");\n'
        "nros_cc_flags::header_deps::emit_header_deps(&out);\n"
    )
    bare = 'b.file("x.c");\nb.compile("x");\n'
    two_sites_one_track = (
        "track_header_deps(&mut a);\n"
        'a.compile("a");\n'
        'b.compile("b");\n'
        "emit_header_deps(&out);\n"
    )
    try_probe = 'if b.try_compile("probe").is_err() {}\n'
    emit_only = "emit_header_deps(&out);\n"
    track_only = "track_header_deps(&mut b);\n"
    prose = (
        "// track_header_deps(&mut b); emit_header_deps(&out);\n"
        'b.compile("app");\n'
    )
    cases = [
        ("compliant snippet", good, "<s>", []),
        ("compile with no helpers", bare, "<s>", ["reach", "reach"]),
        ("two compiles, one tracked", two_sites_one_track, "<s>", ["reach"]),
        ("try_compile counts as a compile", try_probe, "<s>", ["reach", "reach"]),
        ("emit without track", emit_only, "<s>", ["pairing"]),
        ("track-only, not a listed helper", track_only, "<s>", ["pairing"]),
        ("helpers named only in a comment", prose, "<s>", ["reach", "reach"]),
        (
            "configurator file, one track for two sites",
            two_sites_one_track,
            next(iter(TRACKED_BY_CONFIGURATOR)),
            [],
        ),
        ("exempt file", bare, next(iter(EXEMPT)), []),
        ("listed track helper", track_only, next(iter(TRACK_HELPERS)), []),
    ]
    for name, text, rel, want in cases:
        got = [rule for rule, _ in classify(text, rel)]
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


def stale_entries(texts):
    """Rule 3 — every table entry still names a file in the shape it claims."""
    bad = []
    for table, name, want in (
        (EXEMPT, "EXEMPT", "compiles"),
        (TRACKED_BY_CONFIGURATOR, "TRACKED_BY_CONFIGURATOR", "compiles"),
        (TRACK_HELPERS, "TRACK_HELPERS", "tracks"),
    ):
        for rel in table:
            text = texts.get(rel)
            if text is None:
                bad.append(f"{name}: {rel} is not a tracked Rust source")
                continue
            n_compile, n_track, _, _ = counts(text)
            if want == "compiles" and not n_compile:
                bad.append(f"{name}: {rel} no longer calls .compile(")
            if want == "tracks" and (not n_track or n_compile):
                bad.append(
                    f"{name}: {rel} is no longer a track-only helper "
                    f"(track={n_track}, compile sites={n_compile})"
                )
    return bad


def main():
    self_test()
    sources = tracked_rust_sources()
    texts = {}
    for rel in sources:
        try:
            texts[rel] = open(os.path.join(ROOT, rel), encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
    bad = []
    n_reached = 0
    for rel, text in texts.items():
        if counts(text)[0]:
            n_reached += 1
        for rule, detail in classify(text, rel):
            bad.append((rel, rule, detail))
    stale = stale_entries(texts)
    if n_reached == 0:
        sys.stderr.write(
            "check-cc-header-deps: rule 1 is VACUOUS — no tracked Rust source "
            "calls a cc-rs `.compile(`.\n"
            "  Every board build script did when this gate was widened (issue\n"
            "  1580). If the call spelling changed, widen COMPILES; never leave\n"
            "  the gate scanning nothing (issue 1220).\n"
        )
        sys.exit(1)
    if bad or stale:
        sys.stderr.write(
            "check-cc-header-deps: a cc-rs compile with no rebuild edge to what "
            "it reads (issues 1570, 1580).\n\n"
        )
        for rel, rule, detail in bad:
            sys.stderr.write(f"  {rel}: [{rule}] {detail}\n")
        for line in stale:
            sys.stderr.write(f"  [stale-entry] {line}\n")
        sys.stderr.write(
            "\n  cc-rs tells cargo nothing about the headers a TU includes, and\n"
            "  a hand-written rerun-if-changed list names only what its author\n"
            "  knew. Declare what the compiler actually opened:\n"
            "      nros_cc_flags::header_deps::track_header_deps(&mut build);\n"
            '      build.compile("x");\n'
            "      nros_cc_flags::header_deps::emit_header_deps(&out_dir);\n"
            "  or add the file to EXEMPT in this script with the reason.\n"
        )
        sys.exit(1)
    print(
        f"check-cc-header-deps: OK ({len(sources)} tracked Rust source(s), "
        f"{n_reached} with a cc-rs compile, {len(EXEMPT)} exempt)"
    )


if __name__ == "__main__":
    main()
