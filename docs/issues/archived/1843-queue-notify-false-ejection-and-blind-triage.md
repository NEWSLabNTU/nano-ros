---
id: 1843
title: "queue-notify posts \"Ejected from the merge queue\" for a failure no required check carries, and its INFRA triage has never run because the job has no checkout"
status: resolved
type: bug
area: [ci, merge-queue]
severity: high
found: 2026-10-12
resolved: 2026-10-12
related: [1841, 1338, phase-395, phase-396]
---

## Symptom

Every pull request that went through the merge queue on 2026-10-12 got a
comment from `queue-notify`:

> **Ejected from the merge queue** — `queue` failed on the merge group.
> … **Do not re-queue an unchanged commit.**

and then merged anyway:

| PR | notify run | timeline |
| --- | --- | --- |
| #2045 | 38241201245 | `removed_from_merge_queue` 11:30:16Z, `merged` 11:30:17Z |
| #2047 | 38242035564 | `merged` 11:38:19Z |

So the notice was false on every PR it reached, and its advice was harmful: an
author who believed it would not re-queue a pull request that was never ejected.

## Two faults

### 1. "Ejected" was decided by a workflow's conclusion, not by a required check

`queue-notify.yml` fires on `workflow_run` for `gate` and `queue`, and posted
whenever either concluded `failure` on a `merge_group` event. But what decides a
merge group is the REQUIRED status checks, and on `main` there is exactly one:
the context `CI` (gate.yml's `ci-ok` aggregator; ruleset
`required_status_checks = [{"context": "CI"}]`). `queue.yml` runs only
`L3 (cross build + link)` and `coverage report (did L3 run?)`, neither
required. L3 has been red on every merge group since ~#2039 because the runner
keeps the old `play_launch` pin and setup refuses (issue 1841), so every batch
produced a failed `queue` run and therefore a false ejection notice.

Run 38242035564's jobs, read live: `L3 (cross build + link)` failure,
`coverage report` success — and nothing named `CI`.

### 2. The INFRA triage was unreachable: the job had no checkout

The notify job ran `bash scripts/ci/queue-triage.sh 2>/dev/null` inside an
`if`, with no `actions/checkout` step. The file does not exist in an empty
workspace, the `if` took its else branch, `2>/dev/null` swallowed the error,
and the verdict was `UNKNOWN` on every run (`triage verdict: UNKNOWN` in the
latest run's log). The "this is not your pull request" notice phase-396 W4
added was never posted — nothing failed, so nothing said so. A sweep of every
workflow found this the only job that reached a repo path without a checkout.

A third defect sat behind the second and would have surfaced the moment the
checkout landed: `queue-triage.sh` counted failures per WORKFLOW NAME. Every
`gate` failure has the same name, so two unrelated defects in two PRs read as
"red for everyone", and `queue`'s L3 (red on 11 PRs) out-voted any genuine
`gate` ejection — the notifier would have told a real culprit "do not rebase".

## Fix

* **A. Decide from the required contexts.** `scripts/ci/queue-ejected.sh
  <run-id> [attempt]` reads `main`'s required contexts
  (`gh api repos/<repo>/rules/branches/main`) and the completed attempt's jobs,
  and prints `EJECTED` (a required context concluded `failure`/`timed_out`),
  `NOT_EJECTED`, or `UNVERIFIED` (either list unreadable). `queue-notify`
  posts on `EJECTED`, logs a `::notice::` and posts nothing on `NOT_EJECTED`,
  and posts with a `::warning::` on `UNVERIFIED` — a missed ejection leaves an
  author waiting forever, which is worse than a false notice. Skipping a
  `queue` run loses nothing: the `gate` run that carries `CI` triggers the
  notifier on its own completion. The classifier is pure (text in, verdict
  out) with a `--selftest`.
* **B. Check out the default branch** in the notify job (`actions/checkout@v4`,
  `persist-credentials: false`). A `workflow_run` job checks out the default
  branch — trusted code, never the PR head. `actions: read` added for the jobs
  and run-list APIs.
* **Triage per JOB, scoped to the ejecting workflow.** `queue-triage.sh` expands
  each failed run into its failed jobs (dropping the `CI` aggregator when a
  specific job failed beside it) and takes `NROS_TRIAGE_WORKFLOW`, which the
  notifier sets to the workflow that ejected.
* **C. Gate for the class**: `just check workflow-checkout-before-repo-path`
  (fast line) — a step that runs `just`, names a `./…` path or a tracked
  top-level directory path, or `uses: ./…`, must follow an
  `actions/checkout` step in its job. Commands vs prose via
  `scripts/lib/workflow_commands.py`; the top-level directory list is read
  from git. Composite actions are reached only through `uses: ./…`, which the
  gate already requires a checkout before.
* `just check queue-notify-decision` (fast line) runs both selftests —
  `queue-triage.sh --selftest` had no gate before. Both scripts also run their
  selftest on every normal invocation (`check-gate-selftests`), so a broken
  classifier refuses to decide; the notifier reads that as `UNVERIFIED` and
  posts with a warning.

## Verification

* Gate C on the OLD `queue-notify.yml` (restored from `HEAD`, 0 `checkout`
  lines): rc=1, naming `[notify] Comment on the ejected pull request` and the
  `scripts/ci/queue-triage.sh` line. On the fixed file: rc=0. Selftest 15
  cases.
* Classifier mutation (`if ($1 in req)` → `if (1)`, i.e. every failed job
  counts as required): selftest rc=1, 3 of 9 cases FAIL (the queue-run case
  among them), and the mutant's NORMAL path against run 38242035564 exits 1
  instead of printing a verdict. Restored: rc=0.
* Live, read-only: run 38242035564 (`queue`, #2047) → `NOT_EJECTED  L3 (cross
  build + link)`; 38241201245 (`queue`, #2045) → `NOT_EJECTED`; 38217592378
  (`gate`, #2033, CI failure) → `EJECTED  CI`; an unreadable repo →
  `UNVERIFIED`. Triage scoped to `gate` now names
  `check / test-lane-contracts (merge group + nightly/manual)` across #1995 and
  #2033 rather than the workflow name.

## Related

* [issue 1841](../1841-runner-workspace-keeps-old-play-launch-pin.md) — the L3
  red that made every batch produce a failed `queue` run.
* [issue 1338](1338-arm-auto-merge-reports-armed-and-never-arms.md) — the
  same shape: a merge-queue signal that reports something that did not happen.
* [phase-395](../../roadmap/archived/phase-395-dev-workflow-migration.md) introduced
  `queue-notify`;
  [phase-396](../../roadmap/archived/phase-396-merge-queue-throughput-policy.md)
  W4 added the INFRA verdict that fault 2 hid.
