---
id: 872
title: "The PR/nightly check arm has never run to completion — each fix exposes the next environment gap"
status: open
area: ci
severity: medium
found: 2026-08-28
related: [phase-395, 0816]
---

# The PR/nightly check arm has never run to completion

## What was measured

PR #7 is the first pull request through the merge queue (phase-395 W7). Its
required check — `check (fast on push; full on PR/nightly)` — has failed seven
times, each time on a DIFFERENT gate, each one further into the job than the
last. The push lane runs only `check-fast`, so the PR/nightly arm's steps had
never executed against the CI container at all; every failure below is a
pre-existing gap that the queue's arrival made visible, not a regression from
the PR's payload.

Fixed inside PR #7 (each verified locally, most with a negative control):

| # | gate | gap |
| --- | --- | --- |
| 1 | `check-source-gates` | the compile-check fixtures it consumes were never built by the job |
| 2 | `compile-check-fixtures.sh` | the cmake-lane prereq probe hard-exits even when the selection contains no cmake row |
| 3 | `check-source-gates` | bare `cargo test` counts a `skip!` panic as a failure (no junit rewrite) |
| 4 | `check-build` | `msg_to_cyclone_idl.py` imports `rosidl_adapter` at BUILD time; the ROS env was not sourced |
| 5 | `check-sched-dim-arms` | probes that `arm-none-eabi-gcc` EXISTS; the container's has no newlib, so every arm failed on `<string.h>` |

**Rows 1-3 were fixed independently and better on `main`** (phase-395 W19/W20),
while this branch was in flight: `check-source-gates` now builds its own
`cxx-syntax` lane and runs through the shared `_nextest-tolerant`, and the lane
filter short-circuits the cmake prereq probe before it can exit. Two convergent
diagnoses of the same three defects, from opposite ends. Only rows 4 and 5
survive in PR #7; the rest of this table is history, kept because the PATTERN is
the finding — an arm nothing executes accumulates gaps at roughly one per gate.

## Still open — the gate this issue is filed for

(Whether this still reproduces under phase-395 W20's restructure is UNVERIFIED:
the compile tier moved to `merge_group`, so the example check now runs per
BATCH rather than per PR, and this branch has not yet been through the queue.)

With those five fixed the arm reaches the example check inside `just check`,
which reports:

```
native check: 33 example(s) are missing their generated message bindings
```

The examples' `generated/` trees are USER-side codegen (`nros sync`), absent in
a fresh clone by design. The job builds the CLI and provisions `-sys` sources
but never syncs any example leaf, so this gate cannot pass as the workflow
stands. Two candidate shapes, neither yet chosen:

* run `nros sync` for the leaves the check walks (cost: unmeasured), or
* have the example check SKIP legibly when `generated/` is absent, the way the
  cross-toolchain gates now skip — consistent with "probe what you use, skip
  legibly, never fail on a lane you did not enter", which is the vocabulary the
  other four fixes converged on.

## Why this is not "just fix it in the PR"

Each fix has been one line of insight and one round of ~25 minutes of CI. The
payload of PR #7 (the heap-free tier image, issue 0843) is unrelated to any of
this and is verified locally by `ci-l1` + `ci-l3` + native e2e. Continuing to
grow that PR with workflow archaeology couples an unrelated change to an
open-ended CI debug loop; the remaining gap is worth its own change, with the
skip-vs-sync decision made deliberately rather than under merge pressure.

## Method note

`gh run view --job <id> --log-failed` truncates the failing step's own name to
`UNKNOWN STEP` for container jobs, so the failing RECIPE has to be read out of
the log body (`error: recipe \`X\` failed`) rather than the step header. The
first four rounds were diagnosed that way.
