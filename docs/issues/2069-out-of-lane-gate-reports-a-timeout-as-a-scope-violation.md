---
id: 2069
title: "`check-out-of-lane-deselected` reports a host-only test's TIMEOUT as a scope violation, with the scope remedy"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-26
related: [issue-1900, issue-1758, phase-505]
---

# The out-of-lane gate reports a timeout as a scope violation

**Home:** [phase-505](../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md) W2.

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
