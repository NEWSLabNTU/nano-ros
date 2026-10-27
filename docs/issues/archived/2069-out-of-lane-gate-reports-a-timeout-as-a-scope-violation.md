---
id: 2069
title: "`check-out-of-lane-deselected` reports a host-only test's TIMEOUT as a scope violation, with the scope remedy"
status: resolved
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-26
resolved: 2026-10-27
related: [issue-1900, issue-1758, phase-505]
---

# The out-of-lane gate reports a timeout as a scope violation

**Home:** [phase-505](../../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md) W2.

## Problem

`scripts/test/check-out-of-lane-deselected.py` (issue 1900) runs every
platform-token test under the `linux` scope with the emulators hidden, and
`classify()` sorts each junit case into PASS, `[SKIPPED:lane]`, ADMITTED, or an
offender. Anything that is not one of the first three is an offender, and every
offender is reported as "read the host before the scope", with a remedy telling
the author to move `require_platform_in_lane` to the head of the test.

That is the right reading for `[UNMET PRECONDITION]` or a bare `[SKIPPED]`. It
is the wrong reading for a test that RAN under the scope and failed for
another reason. A nextest timeout is one: it has no panic text, so the verdict
reads `(no message)`, and the remedy points at code that is already correct.

The population makes this likely, not rare: it includes host-only source
scans whose names carry a platform token (19 of them PASS on each run), so any
load-sensitive host-only test is in it.

## Evidence

`just ci gate` on branch `audit-checklist-refresh` (PR #2633, a docs-only
change plus one Python script), step 10 of 10, 2026-10-25:

```
TIMEOUT [  60.019s] (200/200) nros-tests::example_shape zephyr_leaf_buildrs_uses_shared_bake
check-out-of-lane-deselected: FAILED — 1 test(s) of a platform the `linux` scope does not select read the host before the scope:
  nros-tests::example_shape zephyr_leaf_buildrs_uses_shared_bake
      (no message)

  Under a scope that does not select its platform, a test must reach
  `lane_skip!` before any probe (issue 1758). Put the scope check FIRST — ...
```

The same test, same tree:

| run | result |
| --- | --- |
| `test-unit`, step 8 of the same `ci gate` | PASS, 0.589 s |
| out-of-lane run, step 10 (200 tests, `--no-fail-fast`) | TIMEOUT, 60.019 s |
| `just check out-of-lane-deselected` re-run solo | PASS, 0.063 s; gate `OK (180 deselected as lane, 19 host-only pass(es), 1 admitted, 0 probed the host first)` |

The test is a static walk of `examples/` that prunes build output
(`nros_tests::treewalk::is_skipped_dir`) and probes nothing. Why it took 60 s
in that run was not measured; the host was carrying other work.

The step runs on `merge_group` (`.github/workflows/gate.yml`, "just check
out-of-lane-deselected (completes the gate lane)"). There, a timeout like this
one ejects the whole batch (`grouping_strategy` ALLGREEN) and sends whoever
reads the ejection to fix scope order in a test that has none to fix.

## Fix direction

- Classify by WHAT happened, not by elimination. A case nextest terminated
  for time, or one that failed without any skip marker, is not evidence about
  scope order. Report it as its own class ("ran under the scope and failed:
  not this check's subject"), with the remedy for that class. Check what
  nextest writes in junit for a timeout before keying on it; do not guess the
  attribute.
- Decide whether that class fails the gate. A real red in a host-only test is
  still a red, but `test-unit` already runs it under the default profile, so
  failing here only duplicates that verdict under heavier parallel load.
  Either way, keep "probed the host first" for `[UNMET PRECONDITION]` and
  bare or other-class `[SKIPPED]`.
- Add the timeout case to `self_test()`, both directions, alongside the
  existing `real_red` row, which today asserts that an ordinary assertion
  failure is reported as a scope violation.
- Find out why the walk took 60 s in that run before deciding whether the
  out-of-lane profile needs its own `slow-timeout`.

## Resolution

Resolved 2026-10-27. `classify()` in
`scripts/test/check-out-of-lane-deselected.py` sorts a failed case by what
happened, not by elimination:

- **probed** — the panic message starts with `[UNMET PRECONDITION]`, or the
  case carries a bare or non-`lane` `[SKIPPED…]` marker. This is the check's
  subject. It fails the gate with the scope remedy, as before.
- **ran and failed** — no skip marker. The test ran under the scope and
  failed. This class gets its own remedy: debug it as an ordinary red, re-run
  it solo in the hidden-emulator environment, and treat an `.expect()` on a
  resolver as a missing `unmet!`. Two kinds:
  - an ordinary failure always fails the gate;
  - a **timeout** fails only when `test-lane-contracts` does not also run the
    test. When it does (the target is listed in
    `.config/lane-admission/gate.txt`, or the `<target>::<test>` row is), the
    gate prints a WARNING and does not fail.

**What nextest writes for a timeout, measured, not guessed.** A throwaway
crate under nextest 0.9.140 with
`slow-timeout = { period = "1s", terminate-after = 2 }` produced
`<failure type="test timeout"/>`. The element has no `message` attribute and
no body, and `<system-err>` is empty, which is why the original verdict read
`(no message)`. An ordinary panic produces
`<failure message="thread '…' panicked at …" type="test failure with exit code 101">`.
The classifier keys on `type="test timeout"`.

**Why a timeout does not always fail: the premise was half wrong.** The issue
said `test-unit` already runs these tests. It does not. `test-unit` passes
`--exclude nros-tests`, and every test this gate selects is an `nros-tests`
test. The step-8/9 PASS of `example_shape zephyr_leaf_buildrs_uses_shared_bake`
came from `test-lane-contracts`, which runs the targets admitted in
`gate.txt`, and `example_shape` is one of them. Of the 18 host-only tests that
PASS in this run, `test-lane-contracts` runs 6. The other 12 (most of the
`cases_bound_to_matrix_cells` set, and
`sched_dims_applied_e2e nuttx_level_budget_overrun_is_throttled_by_the_kernel`)
run in no other merge-gating lane. So the duplication argument holds per test,
not for the whole class:

- When another lane covers the test, a timeout here repeats under heavier
  load a verdict that lane already gave in the same `ci gate` and the same
  merge-queue batch. Failing on it ejects a batch over load.
- When no other lane covers the test, this run is its only verdict, so a
  timeout still fails the gate, with the correct diagnosis.

An ordinary failure always fails for a second reason too. With the emulators
hidden, a probe that panics without a marker shows up only in this run.

**Verification.**
- `--self-test` gained rows for both directions: a timeout in an uncovered
  target fails, a timeout in a covered target or a covered single test does
  not, a timeout in a sibling of a single admitted test fails, a plain failure
  in a covered target still fails, and an `[UNMET PRECONDITION]` quoted inside
  an assertion is a plain failure. The `real_red` row now expects "failed"
  rather than a scope violation.
- Each of five classifier mutations makes the self-test fail: the timeout
  branch removed, the marker test forced true, the `UNMET` test dropped,
  coverage forced false, and per-test admission dropped.
- The measured junit file classifies as one pass, one failed and one timeout.
- `just check out-of-lane-deselected` on the real tree: OK (185 deselected as
  lane, 18 host-only passes, 2 admitted).
- With the profile temporarily set to
  `slow-timeout = { period = "30ms", terminate-after = 1 }`, the end-to-end
  run classified 13 tests as timeouts. The one `gate.txt` covers
  (`cmake_platform_matrix cmake_platform_threadx_requires_board`) was printed
  as a WARNING. The 12 it does not cover failed under the new heading. None
  was reported as "read the host first".

**Not investigated:** why the walk took 60 s in the original run. Solo it
takes 0.063 s, and the out-of-lane profile keeps the default timeout. One
related observation from the 30 ms run: several out-of-lane tests
(`zephyr_leaf_staleness *`, `logging_smoke_zephyr_*`, `violation_channel_e2e
zephyr_*`) spend about 5 s before they reach `lane_skip!`. That is work done
ahead of the scope check, and it adds to this run's load.
