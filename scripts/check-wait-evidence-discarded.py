#!/usr/bin/env python3
"""A timed-out wait's evidence must not be thrown away — issue 0670.

`ManagedProcess::wait_for_output*` returns `Err` carrying WHAT THE PROCESS
ACTUALLY PRINTED. `.unwrap_or_default()` on that call replaces it with `""`, so
the assertion that follows reports `got:` with nothing after it — the diagnostic
is destroyed by the very call that gathered it.

That is not hypothetical. `contract_monitor_parity` failed exactly this way, and
the empty `got:` is why the real cause (issue 0671 — an unguarded `epoch_us_fn`
clobber leaving the age monitor with no clock) needed a separate investigation
rather than being readable off the failure.

WHY THIS IS A GATE AND NOT A SWEEP

The obvious mechanical fix is wrong. Replacing `.unwrap_or_default()` with
`.unwrap_or_else(|e| e.to_string())` folds the error text into the string the
test then asserts on — and that text NAMES the pattern it was waiting for
(``did not print `max-age-runtime` ``), so

    assert!(seen.contains(RULE), "expected {RULE}, got:\\n{seen}")

matches the COMPLAINT about the missing rule and the test passes exactly when it
should fail. This was tried on `contract_monitor_parity` and produced a green run
against a pipeline emitting one DIAG line.

So each site needs its ASSERTION read, not just its `unwrap_or_default` swapped.
`ManagedProcess::collect_until_count` returns the output and the diagnostic on
separate channels for that reason; `collect_until` is the single-occurrence
sibling.

THE BASELINE

The sites present when this gate landed are listed below as a SHRINKING
BACKLOG, not an exemption — the same shape `check-required-features-reachable`
uses and says out loud. Gating all of them at once would fail on day one and get
switched off. What it buys immediately is that a FIFTY-FIRST cannot arrive
silently.

Note the population GREW when the class was fixed at source: before
`dd177b7fd`, `wait_for_output_count`'s timeout was a unit `TestError::Timeout`
with nothing to discard. Now every one of these errors carries output, so every
`.unwrap_or_default()` on one is throwing real evidence away.

A count that FALLS must be recorded in the same change (phase-472 W9,
`scripts/lib/ratchet.py`): a row above the tree's count is slack a file can
regrow into. Measured when this landed: 77 sites against a budget of 87 — ten
sites of regrowth nobody would have been told about.

Run: python3 scripts/check-wait-evidence-discarded.py [--self-test]
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from ratchet import fell_instructions, judge  # noqa: E402  phase-472 W9
SCAN_DIRS = ["packages/testing/nros-tests/tests", "packages/testing/nros-tests/src"]

# A `wait_for_*output*(...)` call whose result is `.unwrap_or_default()`ed.
# `wait_for_ALL_output` is in the family too — writing the prefix as
# `wait_for_output` missed it, and the self-test caught that before the
# baseline was taken, which is the only time it is cheap to catch.
# The argument list may span lines and may contain ONE level of nested parens
# (`Duration::from_secs(10)`), which is the shape every real call has.
DISCARD = re.compile(
    r"(wait_for_\w*output\w*)\s*\((?:[^()]|\([^()]*\))*\)\s*\.\s*unwrap_or_default\s*\(\s*\)",
    re.S,
)

# feature-baseline: file -> count of sites. Lowering a count is progress, and
# it must be RECORDED here in the change that makes it — the gate fails until
# it is, naming the exact line (`scripts/lib/ratchet.py`).
BASELINE = {
    "packages/testing/nros-tests/tests/bridge_mixed_rmw.rs": 1,
    "packages/testing/nros-tests/tests/bridge_zenoh_to_cyclonedds.rs": 3,
    "packages/testing/nros-tests/tests/cli_bringup_zephyr.rs": 1,
    "packages/testing/nros-tests/tests/custom_msg.rs": 5,
    "packages/testing/nros-tests/tests/declarative_bridge_zenoh_to_cyclonedds.rs": 2,
    "packages/testing/nros-tests/tests/declarative_bridge_zenoh_to_xrce.rs": 1,
    "packages/testing/nros-tests/tests/emulator.rs": 2,
    "packages/testing/nros-tests/tests/entry_e2e.rs": 3,
    "packages/testing/nros-tests/tests/error_handling.rs": 9,
    "packages/testing/nros-tests/tests/executor.rs": 1,
    "packages/testing/nros-tests/tests/interop_e2e.rs": 5,
    "packages/testing/nros-tests/tests/multi_node.rs": 2,
    "packages/testing/nros-tests/tests/native_api.rs": 1,
    "packages/testing/nros-tests/tests/native_example_reqresp_e2e.rs": 1,
    "packages/testing/nros-tests/tests/params.rs": 3,
    "packages/testing/nros-tests/tests/qos.rs": 1,
    "packages/testing/nros-tests/tests/realtime_tiers_e2e.rs": 5,
    "packages/testing/nros-tests/tests/ros_editions_bridge.rs": 1,
    "packages/testing/nros-tests/tests/ros_editions_nano_interop.rs": 1,
    "packages/testing/nros-tests/tests/rtos_e2e.rs": 2,
    "packages/testing/nros-tests/tests/services.rs": 1,
    "packages/testing/nros-tests/tests/workspace_metadata.rs": 1,
    "packages/testing/nros-tests/tests/xrce_ros2_interop.rs": 5,
    "packages/testing/nros-tests/tests/zephyr.rs": 20,
}


def tracked_sources():
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--"] + SCAN_DIRS,
        capture_output=True, text=True, check=True,
    ).stdout.split()
    return [ROOT / p for p in out if p.endswith(".rs")]


def offenders():
    """{relative path: [line numbers]} for every evidence-discarding wait."""
    found = {}
    for path in tracked_sources():
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        lines = [text[: m.start()].count("\n") + 1 for m in DISCARD.finditer(text)]
        if lines:
            found[str(path.relative_to(ROOT))] = lines
    return found


SELF_TESTS = [
    ("p.wait_for_output_count(RULE, 1, Duration::from_secs(9)).unwrap_or_default()", True),
    ("p.wait_for_output(\"x\", Duration::from_secs(1))\n    .unwrap_or_default()", True),
    ("p.wait_for_all_output(&[\"a\"], t).unwrap_or_default()", True),
    # The remedies must NOT be flagged.
    ("let (seen, why) = p.collect_until_count(RULE, 1, t);", False),
    ("let seen = p.collect_until(RULE, t);", False),
    ("p.wait_for_output_count(RULE, 1, t).expect(\"no rule\")", False),
    # An unrelated `unwrap_or_default` is not this class.
    ("let n: usize = s.parse().unwrap_or_default();", False),
]


def verdict(found, baseline):
    """[failure lines] for {file: [lines]} measured against `baseline`."""
    rose, fell = judge({f: len(v) for f, v in found.items()}, baseline)
    out = []
    for m in rose:
        if m.was == 0:
            out.append(f"check-wait-evidence-discarded: NEW file discarding wait evidence: {m.key}")
            out.extend(f"  {m.key}:{ln}" for ln in found.get(m.key, []))
        else:
            out.append(f"check-wait-evidence-discarded: {m.key} grew {m.was} -> {m.now}")
    if fell:
        out.extend(fell_instructions(
            fell, "BASELINE in scripts/check-wait-evidence-discarded.py",
            lambda f, n: None if n == 0 else f'"{f}": {n},',
            "python3 scripts/check-wait-evidence-discarded.py --baseline"))
    return rose, out


# (description, found, baseline, expect_failure) — both directions of the
# ratchet, through `verdict`, the function `main` runs.
RATCHET_CASES = [
    ("at its recorded count passes", {"a.rs": [1, 2]}, {"a.rs": 2}, False),
    ("a rise fails", {"a.rs": [1, 2, 3]}, {"a.rs": 2}, True),
    ("a new file fails", {"b.rs": [7]}, {"a.rs": 2}, True),
    # The W9 hole: a fall left unrecorded is slack the file can regrow into.
    ("a fall the baseline does not record fails", {"a.rs": [1]}, {"a.rs": 2}, True),
    ("a file fully converted but still listed fails", {}, {"a.rs": 2}, True),
]


def self_test():
    bad = 0
    for desc, found, base, want in RATCHET_CASES:
        got = bool(verdict(found, base)[1])
        if got != want:
            bad += 1
            print(f"  FAIL  ratchet: {desc}: failed={got}, expected={want}")
        else:
            print(f"  ok    ratchet: {desc}")
    for src, should_flag in SELF_TESTS:
        got = bool(DISCARD.search(src))
        if got != should_flag:
            bad += 1
            print(f"  FAIL  {src!r}: flagged={got}, expected={should_flag}")
        else:
            print(f"  ok    flagged={got}  {src.splitlines()[0][:62]}")
    if bad:
        print(f"\ncheck-wait-evidence-discarded --self-test: {bad} case(s) FAILED")
        return 1
    print(f"\ncheck-wait-evidence-discarded --self-test: "
          f"{len(SELF_TESTS) + len(RATCHET_CASES)} case(s) OK")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    # Always, not only behind the flag (phase-472 W9): a negative control
    # nobody runs decays into a comment. Quiet on success.
    import contextlib
    import io
    with contextlib.redirect_stdout(io.StringIO()) as buf:
        rc = self_test()
    if rc:
        sys.stdout.write(buf.getvalue())
        return rc
    if "--baseline" in sys.argv:
        # Regenerate the literal above after a deliberate conversion.
        for f, lines in sorted(offenders().items()):
            print(f'    "{f}": {len(lines)},')
        return 0

    found = offenders()
    if not BASELINE:
        raise SystemExit(
            "check-wait-evidence-discarded: BASELINE is empty — regenerate it with "
            "`--baseline`, or the gate has nothing to compare against"
        )

    rose, problems = verdict(found, BASELINE)
    if not problems:
        total = sum(len(v) for v in found.values())
        print(
            f"check-wait-evidence-discarded: OK ({total} baselined site(s) in "
            f"{len(found)} file(s); every row at its recorded count)"
        )
        return 0

    for line in problems:
        print(line, file=sys.stderr)
    if rose:
        print(
            "\n"
            "  A `wait_for_output*` error carries what the process PRINTED;\n"
            "  `.unwrap_or_default()` replaces it with \"\", so the assertion reports\n"
            "  `got:` with nothing after it (issue 0670).\n"
            "\n"
            "  Do NOT fix it with `.unwrap_or_else(|e| e.to_string())` — that text\n"
            "  names the pattern it waited for, so `seen.contains(<pattern>)` matches\n"
            "  the complaint about the missing pattern and the test passes exactly\n"
            "  when it should fail.\n"
            "\n"
            "  Use `collect_until_count` (output and diagnostic on separate channels)\n"
            "  or `collect_until`, and put the diagnostic in the panic MESSAGE.\n",
            file=sys.stderr,
        )
    return 1


if __name__ == "__main__":
    sys.exit(main())
