---
id: 2193
title: "Tier-2 nightly's zephyr fixture leg fails, and the job log shows only the tier-priority check that runs after it"
status: open
type: bug
severity: medium
area: [ci, zephyr, fixtures]
related: [2180, 2179, phase-466]
found: 2026-11-01
---

**Home:** [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)
— the tier-2 nightly's Zephyr fixture leg fails without naming the image, so the lane's verdict cannot be attributed.

## Symptom

Nightly 41275698351 (2026-11-01 05:07 UTC, `d7dc9ec6b1`), job 124757318303
`tier 2 nightly (pairwise cover)`, runner `nano-ros-runner-newslab-server-241`.
The failing step is `just build tier2-nightly`.

This is the first tier-2 nightly to get past `check fast` since issue 2180.
The `workspace-system-required` gate no longer fails. The build then failed
in the fixture stage:

```
build-test-fixtures: budget=24, make-jobs=5, pool=4×6 + zephyr=24 (solo)
== zephyr ==
== zephyr == FAILED (rc=2)
first error line(s) in …/tmp/build-test-fixtures-20261101-053151-521276/zephyr.log:
  257:error: recipe `build-fixtures` failed with exit code 2
```

## Why the cause is unknown

The failing image is not named anywhere in the job log:

- The "first error line" that `build-test-fixtures` quotes is the recipe's
  own exit line.
- The `log tail` it prints is the output of `tier-priority-plan-image`, which
  `build-fixtures` runs only after a Zephyr fixture build has failed. That
  check reports `OK (0 pin-check(s) over 29 current image(s); 0 STALE, 12 NO
  BAND, 0 NO ROW)`.
- The make driver's `failed:` line and the CMake/ninja error are not in the
  log.
- The run uploads only `tier2-nightly-junit` and `zephyr-gate-inputs`, so the
  per-leaf build logs stay on the runner.

## What it is NOT

- It is not issue 2180: the gate passed.
- It is probably not issue 2179 (the NSOS snippet reaching mps2), which
  #2910 fixed before this run's commit. But with no build error to read,
  that can't be confirmed.
- It is not out of disk: there is no `No space left` in the log.

## What would close it

1. Surface the failure. When a leg fails, `build-test-fixtures` should quote
   the failing leaf's first error, or name the per-leaf log and upload the
   `build-test-fixtures-*` log directory as an artifact.
2. Then fix the Zephyr image that fails. The acceptance test is a tier-2
   nightly that reaches `ci matrix-nightly`.

## Part 1 fixed (2026-11-05): the job log names the leaf and its error

Seen again in tier-2 nightly 41985062052 (2026-11-05 07:07Z). The leaf logs
did reach the `tier2-nightly-junit` artifact; issue 1842 added them after
this issue was filed. The job log still said nothing useful, for two reasons,
both in `scripts/build/log-first-errors.sh`, the one printer every fixture
driver uses:

1. The failing leaf's log began `Error: missing prerequisites for this
   build:`. That is `nros build`'s preflight refusal, reported by eyre with a
   capital E, and the pattern only knew lowercase `error:`. The driver
   printed `no error line matched in …/build-ws-rs-entry-zenoh.log`.
2. In the stage log, the first match was just's own `error: recipe
   `build-fixtures` failed`. Every recipe the failure unwinds through prints
   that line, so it always matches first and says least.

Fix: the pattern also matches `Error:` (but not a class name like
`ValueError:`) and the drivers' `fixture-make-driver: <leaf> failed` /
`fixture-run-one: … failed for <image>` lines. just's recipe lines print only
when nothing else matched. On today's stage-shaped log, the old script printed
only `3:error: recipe `build-fixtures` failed`; the new one prints the leaf's
name and its `Error: missing prerequisites` line. The selftest grew from 6 to
10 cases, two of them verbatim from the nightly's leaf log.

The image that failed this time was a false preflight refusal (PR #3153).
The acceptance below is unchanged: a tier-2 nightly that reaches `ci
matrix-nightly`.
