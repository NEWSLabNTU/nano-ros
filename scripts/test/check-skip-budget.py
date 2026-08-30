#!/usr/bin/env python3
"""Assert a run's skips instead of merely counting them — issue 0584.

Classes made skips COUNTABLE (`[SKIPPED:lane]`, `<skipped type="nros:lane">`).
This makes them CHECKABLE. Until now `170 skipped` was indistinguishable from
`170 tests silently did not run`, and nobody eyeballs that number — which is how
a lane greens over a coverage hole (0445's absorbing STALE verdict, 0350's
compile-check lane failing wholesale while reporting skips).

Two assertions, both DERIVED from facts the run already has, so there is no
declaration file to drift:

1.  **No `lane` skip for a coordinate the lane SELECTED.** A lane skip says
    "out of lane: … is at coordinate p,l,r"; the run's coordinate file says what
    it selected. A test skipping as out-of-lane for a coordinate that IS in the
    lane means the resolver and the selector disagree, and the run quietly did
    less than it claimed.

2.  **No skip whose reason is a missing fixture.** Since 0584 part 2 an absent
    in-lane fixture is a hard failure, not a skip. Three `Err`-to-`[SKIPPED]`
    laundering sites survive in `fixtures/binaries` (they match on
    `"not prebuilt"`); they are unreachable in a gated run, but they encode the
    old rule and will be copied. If one ever fires again, this says so.

Deliberately NOT asserted: an expected COUNT per class. Counts drift with the
host's toolchains and with every added test, so they would be edited to match
reality on every red — which is the failure mode `#0196` describes for gates
whose coverage is narrower than the rule they enforce. Both rules above are
properties, not numbers.

Usage::

    check-skip-budget.py [junit] [--coords FILE]

`junit` defaults to the `junit-real.xml` snapshot (issue 0527), falling back to
the live file. Coordinates default to `$NROS_TEST_COORDS`; without them,
assertion 1 is reported as not-checked rather than silently passing.
"""

import os
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

# `out_of_lane_coord` in packages/testing/nros-tests/src/fixtures/lane.rs.
COORD_RE = re.compile(r"is at coordinate ([^,]+),([^,]+),([^\s,]+)")
FIXTURE_RE = re.compile(r"not prebuilt|fixture binary MISSING", re.I)


def skips(root: ET.Element):
    for case in root.iter("testcase"):
        node = case.find("skipped")
        if node is None:
            continue
        cls = (node.get("type") or "nros:unclassed").removeprefix("nros:")
        text = f"{node.get('message') or ''} {node.text or ''}"
        yield case, cls, text



# The reason a test skipped is the `[SKIPPED] …` line inside the panic text —
# `nros_tests::skip!` panics, and the junit rewriter keeps the whole panic
# verbatim. Everything around it (the `thread '…' panicked at file:line` header,
# the RUST_BACKTRACE note) is machinery, identical for every skip, and grouping
# on it would put every skip in one bucket.
SKIP_LINE = re.compile(r"^\s*\[SKIPPED\]\s*(.+?)\s*$", re.M)


def reason_of(text: str) -> str:
    m = SKIP_LINE.search(text or "")
    if m:
        return " ".join(m.group(1).split())
    # No marker: fall back to the first line that is not panic machinery, so a
    # skip raised some other way still says something rather than nothing.
    for line in (text or "").split("\n"):
        line = line.strip()
        if not line or line.startswith(("thread '", "note:")) or " panicked at " in line:
            continue
        return " ".join(line.split())
    return "(no reason recorded)"


def report_reasons(rows, out) -> None:
    """Print the DISTINCT reasons, most frequent first.

    Without this the lane says "provision what the skips name" and then does not
    name them. The reasons are already in the junit — every caller was reading
    the file and throwing them away, so triaging a red nightly meant guessing
    which precondition was missing from a count.
    """
    from collections import Counter
    counted = Counter(reason_of(t) for _, t in rows)
    shown = counted.most_common(12)
    for reason, n in shown:
        if len(reason) > 150:
            reason = reason[:147] + "..."
        print(f"      {n:4}x {reason}", file=out)
    rest = len(counted) - len(shown)
    if rest > 0:
        print(f"      … and {rest} further distinct reason(s)", file=out)


def self_test() -> int:
    """`reason_of` must survive the shapes the junit actually carries.

    The reason is buried in a panic message, so every case here is a way that
    burial can go wrong. Getting it wrong is not a crash — it is a plausible
    wrong string in a triage report, which is worse.
    """
    cases = [
        (
            "thread 't' (1) panicked at a.rs:1:1:\n[SKIPPED] idlc not found on PATH\n"
            "note: run with `RUST_BACKTRACE=1`",
            "idlc not found on PATH",
        ),
        # whitespace and wrapping collapse, so two identical reasons group
        ("[SKIPPED]   spaced    out   reason  ", "spaced out reason"),
        # no marker: fall through to the first line that is not machinery
        (
            "thread 't' (1) panicked at a.rs:1:1:\nsome other assertion\nnote: x",
            "some other assertion",
        ),
        # machinery only
        ("thread 't' (1) panicked at a.rs:1:1:\nnote: run with `RUST_BACKTRACE=1`",
         "(no reason recorded)"),
        ("", "(no reason recorded)"),
    ]
    failures = 0
    for text, want in cases:
        got = reason_of(text)
        if got != want:
            print(f"  self-test FAIL: reason_of({text[:34]!r}...) -> {got!r}, want {want!r}")
            failures += 1

    # Grouping: identical reasons collapse, distinct ones do not.
    import io
    buf = io.StringIO()
    report_reasons(
        [("a", "[SKIPPED] same"), ("b", "[SKIPPED] same"), ("c", "[SKIPPED] other")], buf
    )
    out = buf.getvalue()
    if "2x same" not in out.replace("   ", "") or "1x other" not in out.replace("   ", ""):
        print(f"  self-test FAIL: grouping produced:\n{out}")
        failures += 1

    if failures:
        print(f"check-skip-budget self-test: {failures} case(s) FAILED")
        return 1
    print(f"check-skip-budget self-test: OK ({len(cases)} extraction cases + grouping)")
    return 0


def main(argv: list[str]) -> int:
    if "--self-test" in argv[1:]:
        return self_test()

    args = [a for a in argv[1:] if not a.startswith("--")]
    coords_path = None
    for a in argv[1:]:
        if a.startswith("--coords="):
            coords_path = a.split("=", 1)[1]
    coords_path = coords_path or os.environ.get("NROS_TEST_COORDS")

    candidates = [Path(args[0])] if args else [
        Path("target/nextest/default/junit-real.xml"),
        Path("target/nextest/default/junit.xml"),
    ]
    junit = next((p for p in candidates if p.is_file()), None)
    if junit is None:
        print("check-skip-budget: no junit.xml — nothing to check")
        return 0

    try:
        root = ET.parse(junit).getroot()
    except ET.ParseError as exc:
        print(f"check-skip-budget: cannot parse {junit}: {exc}", file=sys.stderr)
        return 2

    selected = set()
    if coords_path and Path(coords_path).is_file():
        for line in Path(coords_path).read_text().splitlines():
            line = line.strip()
            if line:
                selected.add(tuple(p.strip() for p in line.split(",")))

    by_class: dict[str, int] = {}
    surprises: list[str] = []
    laundered: list[str] = []

    actionable: list[tuple[str, str]] = []
    for case, cls, text in skips(root):
        by_class[cls] = by_class.get(cls, 0) + 1
        ident = f"{case.get('classname')} {case.get('name')}"
        # `lane` means out of scope by design; everything else is a precondition
        # this host did not meet, which is the only class anyone can act on.
        if cls != "lane":
            actionable.append((ident, text))
        if cls == "lane" and selected:
            m = COORD_RE.search(text)
            if m and tuple(g.strip() for g in m.groups()) in selected:
                surprises.append(f"{ident}  (coordinate {','.join(m.groups())} IS in this lane)")
        if FIXTURE_RE.search(text):
            laundered.append(ident)

    total = sum(by_class.values())
    breakdown = "  ".join(f"{k}={v}" for k, v in sorted(by_class.items())) or "none"

    # The ZERO-RAN FLOOR. Everything above asserts PROPERTIES of the skips, on
    # purpose — an expected COUNT drifts with the host's toolchains and gets
    # edited to match reality on every red, which is the failure #0196
    # describes. But there is one bound that never drifts and was missing: a run
    # in which NOTHING actually executed is not a pass.
    #
    # `_nextest-tolerant`'s own comment names the hazard — "'all failures were
    # skips' is the sentence a lane that ran nothing also prints" — and the
    # response stopped at properties. So a lane whose every precondition was
    # absent printed `treating as pass` having verified nothing at all, which is
    # the vacuous green this whole family of gates exists to prevent.
    #
    # A FLOOR, not a budget: "at least one test really ran". It cannot be
    # tuned, so it cannot rot.
    executed = 0
    for case in root.iter("testcase"):
        if case.find("skipped") is None and case.find("failure") is None:
            executed += 1
    # DESELECTION is not a skip, and reporting them as one number makes "lots of
    # skips" unreadable. They mean opposite things to a reader:
    #
    #   lane        this test was never in this lane's scope. BY DESIGN wherever
    #               the scope is not name-expressible — tier 2 is 1-wise over
    #               platform, so the lang×rmw narrowing can only happen at the
    #               fixture binding, and the resolver IS the selector there
    #               (issues 0357/0482). Nobody should act on these.
    #   capability  the test WAS in scope and this host could not run it. That is
    #               a provisioning gap and the only class anyone can act on.
    #   resource    in scope, host lacks a runtime resource (ports, devices).
    #
    # Burying the second in the first is how a lane with a real provisioning hole
    # reads the same as one that simply narrowed its scope.
    deselected = by_class.get("lane", 0)
    unprovisioned = total - deselected
    print(f"check-skip-budget: {executed} ran, {deselected} deselected (out of lane), "
          f"{unprovisioned} skipped for an unmet precondition — {breakdown}")
    if unprovisioned:
        print(f"  {unprovisioned} skip(s) name something this host lacks; "
              f"those are the actionable ones:")
        report_reasons(actionable, sys.stdout)
    if not selected:
        print("  (no coordinate file; the out-of-lane assertion was NOT checked)")

    rc = 0
    # Deterministic ordering: stderr is unbuffered and stdout is not, so without
    # this the error block lands ABOVE the summary it says to read above.
    sys.stdout.flush()
    if executed == 0 and total > 0:
        print("", file=sys.stderr)
        print(
            f"ERROR: {total} skip(s) and NOT ONE test actually ran.\n"
            "  This lane verified nothing. 'All failures were [SKIPPED] preconditions'\n"
            "  is true and means only that every precondition was absent — it is the\n"
            "  same sentence a correctly-provisioned lane prints, which is what makes\n"
            "  it dangerous.\n"
            "  Provision what the skips name, or run a lane this host can satisfy.",
            file=sys.stderr,
        )
        if actionable:
            # Named once, in the summary above — repeating them here would put
            # the same list on stdout and stderr and make the interleaved CI log
            # read as two different findings.
            print(
                "  The reasons are listed above, most frequent first.",
                file=sys.stderr,
            )
        else:
            # Every skip was an out-of-lane DESELECTION, so there is nothing to
            # provision: the lane selected no test it could run. That is a
            # scoping fault, not a missing tool, and the remedies are opposite.
            print(
                "  Every skip was out-of-lane deselection, so there is nothing to\n"
                "  provision — this lane selected no test it could run. Check the\n"
                "  lane's coordinates, not the host.",
                file=sys.stderr,
            )
        rc = 1
    if surprises:
        print("", file=sys.stderr)
        print(
            f"ERROR: {len(surprises)} test(s) skipped as OUT OF LANE for a coordinate "
            "this lane selected:", file=sys.stderr,
        )
        for s in surprises:
            print(f"  {s}", file=sys.stderr)
        print(
            "  The resolver and the lane selector disagree, so the run did less\n"
            "  than it reported. Neither number is wrong on its own, which is why\n"
            "  this is invisible without the comparison.", file=sys.stderr,
        )
        rc = 1
    if laundered:
        print("", file=sys.stderr)
        print(
            f"ERROR: {len(laundered)} test(s) SKIPPED for a missing fixture:", file=sys.stderr
        )
        for s in laundered:
            print(f"  {s}", file=sys.stderr)
        print(
            "  Since issue 0584 an absent in-lane fixture is a hard failure, not a\n"
            "  skip — a gated run already asserted the lane's fixtures are built.\n"
            "  Something is still laundering the resolver's Err into a [SKIPPED]\n"
            "  (see the `not prebuilt` matches in fixtures/binaries).", file=sys.stderr,
        )
        rc = 1
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv))
