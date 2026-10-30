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

## Second night: the gates alone now pass two hours, and `check build` is where the time goes (2026-10-30)

The scheduled `host-tests` run 40996190687 (2026-10-30 03:13 UTC, `093456b7e`)
was cancelled the same way. Job 123869758695 `tier-1 gates (just check)`:

> The job has exceeded the maximum execution time of 2h0m0s

Two numbers are different from the first night:

- **The time before the gates shrank.** Step `just ci tier1 gates` started at
  03:24:29, 10.5 minutes into the job (it was about 38 minutes on 10-29). It
  was still running at 05:14:03 when it was killed. So the gates step alone ran
  **1h49m and did not finish**. That rules out a slow setup as the cause: the
  gates themselves no longer fit in the cap.
- **This time the gates log was uploaded.** The `Upload tier-1 gates log` step
  runs after the cancellation, so artifact `host-tier1-gates-ci-logs` holds
  `ci-tier1-gates.log` (7,826 lines). That answers the first-night gap ("this
  run says nothing about which gate was slow").

What the log says:

- `check::default` started at 03:26:25Z. Its recipe is
  `cli-fresh fast api-parity probe-workspace-caps build`.
- `check-fast` finished: `483 gate(s) ran at -P4 ... busy 2246s over wall 567s`.
  That is about 9.5 minutes. Its slowest gate was `declared-qos-registration`
  at 124 s.
- The api-parity output (`c vs rclc+rcl` / `cpp vs rclcpp` / `rust vs rclrs`)
  and the `probe-workspace-caps` checks (`all 5 checks passed`) follow it.
- **The log ends there.** The only recipe left in `check::default` is
  `build`, and the log has no output from it. So `just check build` ran from
  shortly after the api-parity and probe steps until the kill, which is well
  over an hour.

The disk report after the kill reads
`66% used, 50G free — 21G target; 13G build`, so this is not disk (issue 1353).

That narrows what would close this issue. The choice is about `check build`
inside `ci tier1 gates`: give it its own job (the issue 1651 shape), or measure
it and raise `timeout-minutes` by a stated margin. `check-fast` is not the
budget problem. A useful next step is per-gate timing for `check build`, to
match what `check-fast` already prints.
