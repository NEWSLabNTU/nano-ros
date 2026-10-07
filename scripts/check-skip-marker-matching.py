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
    # Producing the marker is not matching on it.
    assert not line_offends('panic!("[SKIPPED] {msg}")')
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
        "sched_dims: 10 cell(s) ran, 6 skipped, 7 out of lane\n"
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
    print("skip-marker matching: OK (no hand-rolled marker matches)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
