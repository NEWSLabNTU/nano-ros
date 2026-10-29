---
id: 2141
title: "host-tests' `tier-1 gates (just check)` job has grown from 1h26m to past its 120-minute cap in three nights, so the scheduled run was cancelled and gave no verdict"
status: open
type: bug
area: ci
severity: medium
found: 2026-10-29
related: [1651, 1500, 1353]
---

## What

The scheduled `host-tests` run 40810937288 (2026-10-29 03:13 UTC, `3819c97b8`)
ended `cancelled`. Three of its four jobs passed. The fourth, job 123240821706
`tier-1 gates (just check)`, was killed by the runner:

> The job has exceeded the maximum execution time of 2h0m0s

The step running at the time was `just ci tier1 gates`, which started at
03:51:33 and was still running at 05:13:54, when the 120-minute
`timeout-minutes` set on that job in `.github/workflows/host-tests.yml` ran out.
The step's own output (`build/ci-logs/ci-tier1-gates.log`) is printed only
after `just` returns, so the log has no gate output at all. This run says
nothing about which gate was slow, and nothing about whether any gate failed.

## The trend is the finding

The same job's wall time on the previous scheduled runs:

| run | date | conclusion | job wall time |
| --- | --- | --- | --- |
| 40263677513 | 2026-10-26 | failure | 1h53m |
| 40443391991 | 2026-10-27 | failure | 1h41m |
| 40625461704 | 2026-10-28 | success | 1h26m |
| 40810937288 | 2026-10-29 | cancelled | >2h00m (timeout) |

This run spent about 38 minutes before the gates started: checkout, the
play_launch submodule, a cache restore, an earlier step that ran from 03:21 to
03:49, and a disk reclaim. So the gates
themselves had about 82 minutes this time, against roughly 50–75 minutes on the
nights before. The cap has gone from comfortable to too tight within a week.

## What it is NOT

- **Not issue 1500.** That issue (resolved) was about the integration job being
  cancelled before it started a step, because `cancel-in-progress: false`
  queued runs behind each other. This run started every step, and the runner
  killed one step at the timeout.
- **Not issue 1353 (disk).** The job's own disk report shows `73% used, 40G
  free` after reclaim, and the job did not die. The runner cancelled it.
- **Not a test failure.** No gate reported FAIL before the cancellation. The
  log has none, because the output tail is never printed when the step is
  killed.

## What would close it

The decision is a budget, so it is not mechanical, and more than one fix is
defensible:

1. Find which gates grew. Run `just ci tier1 gates` with per-gate timing (the
   `check-fast` runner already prints `rc=, NNNNms` per gate) and compare
   against the 10-28 run. A cheap first step is to print the log tail on
   cancellation too, for example from an `if: always()` step that tails
   `build/ci-logs/ci-tier1-gates.log`.
2. Then choose between splitting the job (issue 1651 already split
   `ci tier1` once, for the same reason) and raising `timeout-minutes`, with
   the measured number in the commit.

Close this issue when a scheduled `host-tests` run completes `tier-1 gates`
inside its cap, with the cap's headroom stated.
