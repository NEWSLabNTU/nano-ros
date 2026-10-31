#!/usr/bin/env python3
"""Issue 0658 — recognising a skip marker has ONE spelling per language.

THE DEFECT

`nros_tests::skip!` emits `[SKIPPED] …`; `skip_class!` (issue 0584) emits
`[SKIPPED:<class>] …`. Five matrix aggregators — `entry_matrix`, `multihost`,
`realtime_tiers`, `roundtrip_xprocess`, `sched_dims_applied` — each
independently wrote

    if msg.contains("[SKIPPED]") { skipped.push(…) } else { failed.push(…) }

which is the BARE marker. `[SKIPPED:lane]` does not contain that substring, so
every classed skip was filed as a FAILED cell. Tier 2 grew five reds that were
skips, and the junit rewriter could not rescue them: by then the marker sat
nested inside an aggregate panic body, and the rewriter matches only at the
START of a payload (deliberately — a real failure may legitimately quote the
word).

THE RULE

Rust classifies a captured panic with `nros_tests::skip_marker`
(`is_skip` / `class_in` / `starts_with_skip`). Python classifies a junit payload
with `scripts/test/skip_marker.py`. Nothing else matches the marker by hand.

WHY A GATE AND NOT JUST THE FIX

This literal was ALREADY fixed once, in the junit rewriter's `_is_skipped_failure`
(phase-340). It came back in five places at once because there was no shared
helper to reach for and no check to notice — the exact "fix the CLASS, not the
reported site" failure CLAUDE.md describes. A sixth aggregator is one copy-paste
away.

THE SECOND RULE — PRODUCING the marker (issue 1900)

Since issue 1758 a test never skips because something is absent: it fails
with `nros_tests::unmet!`. The one legitimate skip is `lane_skip!`
(`[SKIPPED:lane]`), reached from the lane's scope before any probe. So
nothing in Rust outside the macro's home may SPELL a `[SKIPPED…]` literal in
code: a hand-written `panic!("[SKIPPED] …")` is a skip-on-absence the
retirement of `skip!` never reached — twelve survived in `logging_smoke.rs`,
and this gate's own self-test used to assert that producing the marker was
fine. It is not.

One exemption, DERIVED rather than listed: a test that is `#[ignore]`d and
whose lane check names only platforms no `matrix::CELLS` row builds
(`platform_is_built` — every cell a `CarveOut`). Such a test runs in no lane,
so its marker reaches no verdict; today that is the dormant ESP32 test (issue
1525), which is kept unchanged by decision.
"""

import re
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))
import file_kinds  # noqa: E402  phase-472 W5 — the population is a KIND, not `*.rs`

# A hand-rolled test for the marker: `contains`, `starts_with`, `find`, `==`,
# `matches` against a literal beginning `[SKIPPED` — and the Python spellings,
# `startswith` / `endswith` / `in` (the 2026-10-01 re-run: the population was
# `*.rs` only, so the Python half of THE RULE below was never read).
HAND_MATCH = re.compile(
    r"""(?:\.(?:contains|starts_with|ends_with|startswith|endswith|find|matches)
             \s*\(\s*["']\[SKIPPED[^"']*["']|
         ==\s*["']\[SKIPPED[^"']*["']|
         ["']\[SKIPPED[^"']*["']\s+(?:not\s+)?in\b)""",
    re.X,
)

# Issue 1900 — a `[SKIPPED…]` string literal in Rust CODE, i.e. a test
# producing the marker by hand. `lane_skip!` is the only producer.
PRODUCE = re.compile(r"""["']\[SKIPPED""")

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MATRIX = os.path.join(ROOT, "packages/testing/nros-tests/src/matrix.rs")


def unbuilt_platforms(matrix_src: str) -> set[str]:
    """`PlatformId`s with no `matrix::CELLS` row that is not a `CarveOut` or `Gated`.

    The Python reading of `matrix::platform_is_built`, from the same table, so
    the exemption is a PROPERTY of the matrix and never a name written here.
    """
    start = matrix_src.find("pub const CELLS: &[Cell] = &[")
    if start < 0:
        raise SystemExit("check-skip-marker-matching: cannot find matrix::CELLS")
    built, seen = set(), set()
    i = start
    while True:
        i = matrix_src.find("cell(", i)
        if i < 0:
            break
        if i > 0 and (matrix_src[i - 1].isalnum() or matrix_src[i - 1] == "_"):
            i += 5
            continue
        depth, j = 0, i + 4
        while j < len(matrix_src):
            ch = matrix_src[j]
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        body = matrix_src[i + 5:j]
        platform = body.split(",", 1)[0].strip().split("::")[-1]
        if platform and platform[0].isupper():
            seen.add(platform)
            # phase-511 — a `Gated` cell is built by its own lane, never by a
            # CI one, so it does not make its platform built either.
            if "CarveOut(" not in body and "Gated(" not in body:
                built.add(platform)
        i = j
    if not seen:
        raise SystemExit("check-skip-marker-matching: matrix::CELLS yielded no cell")
    return seen - built


_FN = re.compile(r"^\s*(?:pub\s+)?fn\s+\w+")
_LANE_PLATFORMS = re.compile(r"require_platform_in_lane\(\s*&\[([^\]]*)\]")


def produce_exempt(lines: list[str], idx: int, unbuilt: set[str]) -> bool:
    """Is the literal on `lines[idx]` inside an `#[ignore]`d test whose lane
    check names only unbuilt platforms?"""
    fn_at = next((k for k in range(idx, -1, -1) if _FN.match(lines[k])), None)
    if fn_at is None:
        return False
    attrs = []
    k = fn_at - 1
    while k >= 0 and (lines[k].strip().startswith("#[") or lines[k].strip().startswith("///")
                      or lines[k].strip().startswith("//")):
        attrs.append(lines[k].strip())
        k -= 1
    if not any(a.startswith("#[ignore") for a in attrs):
        return False
    body = "\n".join(lines[fn_at:idx + 1])
    m = _LANE_PLATFORMS.search(body)
    if not m:
        return False
    named = set(re.findall(r"PlatformId::(\w+)", m.group(1)))
    return bool(named) and named <= unbuilt


def line_produces(line: str) -> bool:
    """The PRODUCE predicate, comment-stripped like `line_offends`."""
    return bool(PRODUCE.search(line.split("//", 1)[0]))


# The helper itself, and the macros that PRODUCE the marker, must be free to
# spell it out. Everything else consumes it through the helper.
EXEMPT_FILES = {
    "packages/testing/nros-tests/src/lib.rs",  # skip!/skip_class! + skip_marker
    "scripts/test/skip_marker.py",  # THE Python helper the rule names
    "scripts/check-skip-marker-matching.py",  # this gate's own specimens
}


def line_offends(line: str, lang: str = "rust") -> bool:
    """The ONE predicate. Both the scan and the self-test go through it.

    Comment stripping lives HERE rather than in the caller because the first
    draft tested `HAND_MATCH` directly and so asserted against a different
    predicate than the one that runs — the self-test failed on a commented-out
    line the real scan would have skipped. A gate whose test exercises a
    different code path than its check is the vacuity this file exists to avoid.
    """
    code = line.split("#", 1)[0] if lang == "python" else line.split("//", 1)[0]
    return bool(HAND_MATCH.search(code))


def offenders() -> list[str]:
    files = file_kinds.files_of_kind("rust", "python")
    if not any(f.endswith(".py") for f in files):
        raise SystemExit("check-skip-marker-matching: the population reached no Python file")
    bad = []
    for path in files:
        if path in EXEMPT_FILES:
            continue
        try:
            src = open(path, encoding="utf-8").read()
        except OSError:
            continue
        if "[SKIPPED" not in src:
            continue
        lang = "python" if path.endswith(".py") else "rust"
        for n, line in enumerate(src.splitlines(), 1):
            if line_offends(line, lang):
                bad.append(f"{path}:{n}: {line.strip()}")
    return bad


def producers(unbuilt: set[str]) -> list[str]:
    """Issue 1900 — Rust code spelling a `[SKIPPED…]` literal by hand."""
    bad = []
    for path in file_kinds.files_of_kind("rust"):
        if path in EXEMPT_FILES:
            continue
        try:
            src = open(path, encoding="utf-8").read()
        except OSError:
            continue
        if "[SKIPPED" not in src:
            continue
        lines = src.splitlines()
        for n, line in enumerate(lines):
            if line_produces(line) and not produce_exempt(lines, n, unbuilt):
                bad.append(f"{path}:{n + 1}: {line.strip()}")
    return bad


def self_test() -> None:
    """The gate must reject the code it was written for, and accept the fix.

    Three gates shipped vacuous in this tree in one day, each caught only by a
    self-test, so this one asserts against the ACTUAL pre-fix line rather than
    trusting the regex to be obviously right.
    """
    pre_fix = '            if msg.contains("[SKIPPED]") {'
    post_fix = "            if nros_tests::skip_marker::is_skip(&msg) {"
    assert line_offends(pre_fix), "the gate would not have caught issue 0658"
    assert not line_offends(post_fix), "the gate rejects its own fix"
    # The classed spelling is just as wrong — it misses the BARE marker.
    assert line_offends('x.contains("[SKIPPED:lane]")')
    # Producing the marker is not MATCHING on it — but since issue 1900 it is
    # its own offence: `lane_skip!` is the only producer, and an absent tool
    # is `unmet!`. This line used to be asserted harmless, which is how twelve
    # `panic!("[SKIPPED] …")` sites outlived the retirement of `skip!`.
    assert not line_offends('panic!("[SKIPPED] {msg}")')
    assert line_produces('        panic!("[SKIPPED] qemu-system-arm not available");')
    assert line_produces('    panic!("[SKIPPED:lane] {why}")')
    assert not line_produces('        nros_tests::unmet!("qemu-system-arm not available");')
    assert not line_produces('    // was: panic!("[SKIPPED] qemu-system-arm not available");')
    produce_exemption_self_test()
    # A comment mentioning the old code must not trip the gate (the
    # check-goal-cdr-stripped lesson: gates that pass on prose about themselves).
    assert not line_offends('    // was: msg.contains("[SKIPPED]")')
    # The Python half of the rule (2026-10-01 re-run): `*.rs` was the whole
    # population, so a hand-rolled match in a junit post-processor passed.
    assert line_offends('    if line.find("[SKIPPED") >= 0:', "python")
    assert line_offends('    if "[SKIPPED]" in msg:', "python")
    assert line_offends("    return text.startswith('[SKIPPED')", "python")
    assert not line_offends('    # was: "[SKIPPED]" in msg', "python")
    assert not line_offends('    return skip_marker.is_skip(msg)', "python")
    nested_marker_self_test()


def produce_exemption_self_test() -> None:
    """The derived exemption, both directions, on synthetic sources."""
    matrix = (
        "pub const CELLS: &[Cell] = &[\n"
        "    cell(Linux, Rust, Zenoh, Pubsub, Example, Runtime),\n"
        "    cell(Dormant, Rust, Zenoh, Pubsub, Example, CarveOut(PARKED)),\n"
        "    cell(Mixed, Rust, Zenoh, Pubsub, Example, CarveOut(\"x\")),\n"
        "    cell(Mixed, C, Zenoh, Pubsub, Example,\n"
        "         Runtime),\n"
        "];\n"
    )
    assert unbuilt_platforms(matrix) == {"Dormant"}, unbuilt_platforms(matrix)

    def src(ignore: bool, platform: str) -> list[str]:
        return ([
            "#[test]",
        ] + (['#[ignore = "parked"]'] if ignore else []) + [
            "fn t() {",
            "    nros_tests::fixtures::lane::require_platform_in_lane(",
            f"        &[nros_tests::matrix::PlatformId::{platform}],",
            '        "x",',
            "    );",
            "    if !probe() {",
            '        panic!("[SKIPPED] tool not available");',
            "    }",
            "}",
        ])

    def at(lines: list[str]) -> int:
        return next(i for i, l in enumerate(lines) if "[SKIPPED" in l)

    s1 = src(True, "Dormant")
    assert produce_exempt(s1, at(s1), {"Dormant"}), "ignored + unbuilt must be exempt"
    s2 = src(False, "Dormant")
    assert not produce_exempt(s2, at(s2), {"Dormant"}), "a RUNNABLE test is never exempt"
    s3 = src(True, "Mixed")
    assert not produce_exempt(s3, at(s3), {"Dormant"}), "a BUILT platform is never exempt"
    s4 = ["#[test]", '#[ignore = "x"]', "fn t() {", '    panic!("[SKIPPED] y");', "}"]
    assert not produce_exempt(s4, at(s4), {"Dormant"}), "no lane check, no exemption"


def nested_marker_self_test() -> None:
    """Issue 1729 — the Python helper's NESTED-marker reading, both directions.

    Through `name-real-failures.py`'s own `has_nested_marker` on a real junit
    `<testcase>`, so the annotation that run prints is the one under test. A
    consolidated matrix test PRINTS a `[SKIPPED:lane]` note per cell it did not
    run; the annotation must look inside the failure's own panic message, never
    at that summary, or every real failure of such a test reads as a laundered
    skip (it sent issue 1729's triage after a skip that was not there).
    """
    import importlib.util
    import xml.etree.ElementTree as ET

    here = os.path.dirname(os.path.abspath(__file__))
    sys.path.insert(0, os.path.join(here, "test"))
    spec = importlib.util.spec_from_file_location(
        "name_real_failures", os.path.join(here, "test", "name-real-failures.py")
    )
    nrf = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(nrf)

    summary = (
        "sched_dims: 10 cell(s) executed (10 passed, 0 failed), 7 out of lane, 0 skipped\n"
        "  - nuttx/rust: [SKIPPED:lane] out of lane: no NuttxArm coordinate\n"
    )
    trailer = "\nnote: run with `RUST_BACKTRACE=1`"
    real = ("thread 't' panicked at t.rs:1:5:\nsched_dims: 1 of 10 cell(s) FAILED:\n"
            "  x: tier never dispatched" + trailer)
    nested = ("thread 't' panicked at t.rs:1:5:\nmulti: 1 of 2 cell(s) FAILED:\n"
              "  y: [SKIPPED:lane] out of lane" + trailer)
    for desc, out, err, want in (
        ("summary in stdout, real panic", summary, real, False),
        ("marker inside the panic message", "", nested, True),
        ("summary AND a nested panic", summary, nested, True),
    ):
        case = ET.fromstring(
            "<testcase classname='c' name='t'><failure/>"
            "<system-out></system-out><system-err></system-err></testcase>"
        )
        case.find("system-out").text = out
        case.find("system-err").text = err
        got = nrf.has_nested_marker(case.find("failure"), case)
        assert got == want, f"nested-marker self-test: {desc}: got {got}, want {want}"


def main() -> int:
    self_test()
    with open(MATRIX, encoding="utf-8") as fh:
        unbuilt = unbuilt_platforms(fh.read())
    made = producers(unbuilt)
    if made:
        print(
            "ERROR: a skip marker is PRODUCED by hand (issue 1900):",
            file=sys.stderr,
        )
        for b in made:
            print(f"  {b}", file=sys.stderr)
        print(
            "\n  A test never skips because something is absent — it fails\n"
            "  (issue 1758). Put the lane check FIRST\n"
            "  (`nros_tests::fixtures::lane::require_platform_in_lane`), and turn\n"
            "  the absence after it into `nros_tests::unmet!(…)`. `lane_skip!` is\n"
            "  the only producer of `[SKIPPED:lane]`.",
            file=sys.stderr,
        )
    bad = offenders()
    if bad:
        print(
            "ERROR: a skip marker is matched by hand instead of via "
            "`nros_tests::skip_marker`:",
            file=sys.stderr,
        )
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        print(
            "\n  `[SKIPPED:<class>]` does not contain `[SKIPPED]`, so a literal\n"
            "  match silently reclassifies every CLASSED skip as a failure\n"
            "  (issue 0658: five tier-2 reds that were lane skips).\n"
            "  Fix:  nros_tests::skip_marker::is_skip(&msg)\n"
            "        nros_tests::skip_marker::class_in(&msg)",
            file=sys.stderr,
        )
        return 1
    if made:
        return 1
    print(
        "skip-marker matching: OK (no hand-rolled marker matches; no hand-produced "
        f"marker outside an ignored test of an unbuilt platform {sorted(unbuilt)})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
