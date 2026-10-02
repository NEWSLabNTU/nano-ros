---
id: 1651
title: "`host-tests.yml` has not gone green since 2026-06-17 — most runs are cancelled by the next merge, and the rest fail at `just ci tier1`"
status: open
type: bug
area: ci, testing
severity: high
found: 2026-10-03
related: [1521, 1644, 1226, 1158]
---

## What this is

`.github/workflows/host-tests.yml` is the only workflow that runs the
fixture-backed `nros-tests` integration suite on the host. Measured 2026-10-03
over its last 30 runs:

| outcome | count |
| --- | --- |
| `cancelled` | 24 |
| `failure` | 4 |
| in progress | 2 |
| `success` | **0** |

The most recent successful run is **2026-06-17** (a `schedule` event). Three and
a half months of a lane that reports nothing, while being the one place this
suite runs.

## Two mechanisms, both measured; neither root-caused here

1. **Cancellation.** The workflow fires on `push` (i.e. after every merge to
   `main`), `schedule` and `workflow_dispatch`. Its `workspace unit tests` job
   has a concurrency group with `cancel-in-progress: ${{ github.event_name ==
   'push' }}`, so each merge cancels the previous run's unit job — and the RUN
   then reads `cancelled`. The merge queue lands merges faster than the
   integration job finishes, so almost every run is superseded. The integration
   job's own group has `cancel-in-progress: false`, so its verdict may still
   exist inside a run the summary calls cancelled — but nobody reads per-job
   results of a cancelled run, which is the same signal loss.
2. **Failure.** When a run completes, `nros-tests integration (host)` fails at
   step `just ci tier1` (latest: run 36968648766); `workspace unit tests`
   passes. Which tests fail was not investigated here.

This is CLAUDE.md's "a red CI lane answers one of two questions and they look
identical" in its strongest form: a lane that never reports a pass cannot
report a regression either. Issue 1521 (tests no merge-gating lane ran) and
the three fixture-free tests PR #1581 found already red on `main` both surfaced
because nothing here was being read.

## What to do

1. **Get a verdict.** Run the integration job to completion on current `main`
   (`workflow_dispatch`) and record which tests fail. Retest any QEMU red solo
   before believing it (CLAUDE.md: full-sweep QEMU lanes flake under load), and
   check fixture mtimes against the code blamed before filing from it (issues
   0859-0862).
2. **Stop the cancellation from hiding the verdict.** Options, not decided
   here: scope `cancel-in-progress` so it never cancels the integration verdict
   on `main`; or drop `push` and run on `schedule` only, so each run completes;
   or make the run's summary reflect the integration job's own outcome. The
   decision belongs with whoever owns the CI budget.
3. **Then fix the reds**, one issue each if they are unrelated.

## Acceptance

A `host-tests.yml` run on `main` completes and reports `success`, and the lane's
configuration makes a later regression produce a `failure` someone sees rather
than a `cancelled`.
