---
id: 2193
title: "Tier-2 nightly's zephyr fixture leg fails, and the job log shows only the tier-priority check that runs after it"
status: open
type: bug
severity: medium
area: [ci, zephyr, fixtures]
related: [2180, 2179]
found: 2026-11-01
---

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
