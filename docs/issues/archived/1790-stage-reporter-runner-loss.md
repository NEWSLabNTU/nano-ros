---
id: 1790
title: "A lane whose runner was lost mid-job got three different wrong labels — `DID NOT START` (job name), `RAN and FAILED` (summary), `stopped in the build` (`--history`) — because only an in-job step writes the stage, and it never ran"
status: resolved
type: bug
severity: medium
area: [ci, testing]
related: [1754, 1158, 1043]
found: 2026-10-10
---

## Measured

`run-matrix.yml` run 37893868358 (schedule, main `ea333ea179`), job
113714712359 `tier 1 (cells)`. Its self-hosted runner was stopped on purpose
at 2026-10-09 08:35Z, during a hand-over to another workstation. GitHub's
step record for the job:

| step | status | conclusion |
| --- | --- | --- |
| `just setup tier1` | completed | success |
| `Verify this runner's labels are true` | completed | success |
| `just build tier1` | completed | success (07:26:44Z–08:34:11Z, 68 min) |
| `just ci tier1 run` | **in_progress** | — |
| `Upload junit and logs`, `Sweep orphans and disk`, `Which stage did this lane reach?` | pending | — |

The job completed with conclusion `failure`. Its annotation read "The
self-hosted runner lost communication with the server."

The correct answer is "no verdict: the runner was lost during the cells". The
three places that report a lane's stage each said something else:

| where | said |
| --- | --- |
| coverage job **name** (`run-matrix.yml`) | `tier 1 — DID NOT START` |
| coverage job **summary** (`report-interlock-coverage.sh`) | `RAN and FAILED — see that job's log` |
| `lane-stage.py --history` (`just matrix-triage`) | `NO VERDICT: stopped in the build` |

## Cause

The stage label is written by `Which stage did this lane reach?`, a step
INSIDE the lane job, run under `if: always()`. A lost runner runs nothing more,
so `needs.<job>.outputs.stage_label` was empty. Each consumer then read the
empty label as a different, specific claim:

- The job names spelled `stage_label || 'DID NOT START'`. That fallback was
  written for the interlocked SKIP, where the job never runs. A job that
  started and lost its runner also has no outputs, and only
  `needs.<job>.result` (`skipped` vs `failure`) tells the two apart.
- `report-interlock-coverage.sh` treated `failure` with no stage as "RAN and
  FAILED", the branch meant for a lane whose reporter is not staged.
- `classify()` read only step CONCLUSIONS. A step still `in_progress` has
  none, so the classifier fell back to the last stage that SUCCEEDED, which
  was the build.

Issue 1754 fixed a mislabel from inside the job: the reporter ran, and its
inner markers said that no cell had started. That fix could not reach this case,
because here nothing inside the job reports at all.

## Fixed

One rule in `scripts/ci/lane-stage.py`, reached from every place that turns
a missing label into words:

- **`classify()`**: a job with conclusion `failure`/`cancelled` that still has
  a step with `status: in_progress` lost its runner in that step. It now
  returns `NO VERDICT: runner lost mid-job`, with the step as the failing step
  and the step's stage. Only `gh run view --json jobs` carries `status`, so
  the in-workflow `--report` path is unchanged. That is correct, because when
  the runner is lost, that path does not run.
- **`classify_job()`** is now the one per-job classifier. `--history` and the
  new `--job-label --lane <job name> --run <id>` both use it, so the run list
  and the coverage job cannot give two answers about one job.
- **`report-interlock-coverage.sh`**: when there is no stage and the result is
  `failure`/`cancelled`, it asks `--job-label`, which reads the gated job's own
  step record. A runner-lost label gets its own wording ("about the RUNNER, not
  the code"). Without `GH_TOKEN` it keeps the old wording.
- **The five coverage jobs** that call it (`run-matrix` x2, `nightly`, `queue`,
  `post-submit`) get `permissions: actions: read` and `GH_TOKEN`.
  `post-submit` passed a lane name (`dep-chain`) that is not a job name, so the
  lookup could never have matched. It now passes the job's name.
- **Every `'DID NOT START'` fallback** (`run-matrix` x2, `live-peer` x4,
  including the board job's shell `${label:-DID NOT START}`) now applies only
  when `result == 'skipped'`. Otherwise it reads
  `NO VERDICT: the job ended before it reported`.

Measured against the real run after the fix: `--job-label`, the coverage
script and `--history` all print `NO VERDICT: runner lost mid-job`, and
`--history` adds `at: just ci tier1 run`.

**Gate:** `check-lane-stage-reporting` (`lane-stage.py --selftest`) gained:

- a replay of job 113714712359's recorded steps;
- a reach arm over every workflow, checking that each `'DID NOT START'` is
  guarded by `result == 'skipped'`, that each coverage caller names a job that
  exists in its workflow, and that each such job grants `actions: read` and
  `GH_TOKEN`.

Mutation proofs (selftest rc; clean tree rc=0):

| mutation | rc | failing check |
| --- | --- | --- |
| drop the in-progress rule from `classify()` | 1 | `37893868358: a runner lost in the cells is NOT stopped in the build` |
| bare `\|\| 'DID NOT START'` back on the tier-1 name | 1 | `every DID NOT START fallback is guarded` (`run-matrix.yml:325`) |
| remove `actions: read` from `queue.yml`'s coverage job | 1 | `queue.yml: job coverage can read the gated job's steps` |
| `post-submit` lane back to `dep-chain` | 1 | `coverage lane dep-chain names a job in its workflow` |

## Not covered

- A job's NAME is evaluated before that job runs, so the coverage job's name
  can only say `NO VERDICT: the job ended before it reported`. The precise
  `runner lost mid-job`, and the step, appear in that job's log, its summary
  and `--history`.
- A runner lost between steps, so that no step is `in_progress`, still reads
  as the last completed stage. No such run has been recorded.
