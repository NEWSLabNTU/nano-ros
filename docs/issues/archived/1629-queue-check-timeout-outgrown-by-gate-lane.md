---
id: 1629
title: "The merge queue's 60-minute `check_response_timeout_minutes` is now
  SHORTER than the gate lane, so entries are ejected `checks_timed_out` while
  their own gate goes on to pass — main has not merged for 2 hours"
status: resolved
resolved: 2026-10-21
resolved_in: "PR #2428 (agreement gate, declared 90 min) + ruleset main-rules edit 2026-10-21"
type: bug
area: ci, process
severity: high
found: 2026-10-02
related: [phase-396, issue-1347, issue-1365, issue-1500, phase-505]
---

**Home: [phase-505](../../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## Measured

The ruleset `main-rules` sets, verbatim from the API:

```json
{"check_response_timeout_minutes":60,"grouping_strategy":"ALLGREEN",
 "max_entries_to_build":5,"max_entries_to_merge":5,"merge_method":"REBASE",
 "min_entries_to_merge":3,"min_entries_to_merge_wait_minutes":5}
```

Today's completed `merge_group` **gate** runs, oldest first:

| run | PR | duration | outcome |
| --- | --- | --- | --- |
| 36957967586 | 1535 | 41 min | success → merged |
| 36959236111 | 1550 | 35 min | success → merged |
| 36960043600 | 1550 | 64 min | success → merged |
| 36966239481 | 1538 | 32 min | success → merged |
| 36968982776 | 1551 | 40 min | success, **entry already gone** |
| 36969459579 | 1551 | **73 min** | success, **entry ejected 10 min earlier** |

**The decisive pair.** PR #1551 was enqueued at `05:33:03` and ejected at
`06:36:26` with reason `checks_timed_out` — 63 minutes. Its batch gate,
36969459579, was created at `05:33:21` and **succeeded at 73 minutes**. The entry
was removed ten minutes before the check it was waiting for reported green.

Everything that merged today had a gate of 64 minutes or less. The one that ran
73 was ejected. `main` has not moved since **04:49:21**, two hours.

## It is a stated policy whose premise moved

phase-396 W3 chose this number and wrote down why:

> **`check_response_timeout_minutes: 60`** is right for a 30-ish minute lane;
> the usual rule is ~2× observed duration.

That was sound for the lane it described. The lane now runs **32 to 73 minutes**
on the same event, so 60 is no longer ~2× observed — at the top of the range it
is **0.8×**. The policy did not become wrong; its input moved, which is the same
shape as issue 1627 (a rule whose premise fails) rather than an oversight.

## What this is NOT

- **Not a defect in the ejected pull request.** #1551's gate **succeeded**. The
  ejection reason is about elapsed time, not about the change.
- **Not 1365.** There the tier-2/queue job is queued and unclaimed. Here the jobs
  are claimed, running and passing — just slowly.
- **Not 1347**, which is `just queue-triage` miscounting cancelled as not-failed.
  This is the queue evicting on its own clock; triage would report it correctly.
- **Not 1500.** That is `host-tests` oversubscription on the push lane. This is
  the `merge_group` gate against a queue setting.
- Not constant: four of six gates today finished inside the window. The lane's
  variance is what makes this intermittent, and intermittent is worse than
  constant here, because `checks_timed_out` reads like infrastructure noise.

## Why it is severity high

A queue that evicts entries for being slow, while their checks pass, has no
throughput floor: nothing is wrong with any pull request, and nothing merges.
Two hours of a frozen `main` with two green PRs queued is the measurement. And
the eviction is self-concealing — `checks_timed_out` invites "re-arm and hope",
which is what I did, rather than "the window is too small".

## What would close this

**Needs a human: changing `check_response_timeout_minutes` is a ruleset edit**,
which is repository configuration and outside what an unattended sweep should do.

Two directions, and the doc's own rule prices the first:

1. **Raise the window.** phase-396's stated rule is ~2× observed duration; at a
   73-minute observation that is ~150 minutes. Cheap, immediate, and it makes the
   queue honest about what the lane costs — at the price of a slower verdict when
   something really is wedged (issue 1492's case).
2. **Make the lane fit the window.** The gate's long tail is
   `just check workspace-all`, measured at ~37 minutes on one PR earlier today
   and the step every slow run was sitting in. Moving it off the merge-group
   event (it already runs on the pull-request event) would bring the lane back
   under 60 — but it would also stop re-checking it against the speculative
   base, which is the thing a queue is for.

Acceptance either way: a `merge_group` gate that runs to completion and merges
without an eviction, on a lane whose slowest observed run still fits the window.

## Operator note, from getting this wrong

My own `dequeuePullRequest` at `05:29:46` — taken to push a correction commit —
orphaned gate 36968982776, which then spent 40 minutes succeeding for an entry
that no longer existed. Dequeue-to-push costs a full batch of runner time, and
with `min_entries_to_merge: 3` and only two open PRs it also resets the
5-minute grouping wait. Worth batching such pushes rather than doing them one
PR at a time.

## The same head, two batches, 73 minutes and 38 minutes — it is lane variance (2026-10-02)

PR #1551 merged at 07:32:19, and the way it got there is the cleanest evidence
this issue can have: **the same head was batched twice and the gate took half as
long the second time.**

| batch gate | enqueued | gate duration | outcome |
| --- | --- | --- | --- |
| 36969459579 | 05:33:03 | **73 min** | success — but the entry was ejected `checks_timed_out` at 63 min |
| 36975759614 | 06:53:02 | **38 min** | success, merged |

The same head of PR #1551 both times (a branch commit since rebased away). Same branch, same diff, same gate list. So the
60-minute window is not a verdict on any pull request's content or size — it is a
coin flip on how long the lane happens to take, and this PR lost once and won
once without changing a byte.

That also retires the most tempting reading of the first ejection: that a
docs-only change somehow attracted a slow lane. It did not; the second run of the
identical tree finished in 38 minutes.

### What it costs when it loses

The 05:33 attempt burned a full 73-minute gate whose green verdict arrived for an
entry that no longer existed, and `main` stayed frozen from 04:49:21 to 07:32:19
— **2 h 43 m** — with nothing wrong anywhere. The throughput argument in this
issue is that figure, not the ejection count.

## Progress (2026-10-19): the agreement is gated; the ruleset edit waits on an admin

### The lane now, measured

Completed `merge_group` runs of `gate.yml` from `gh run list`, timed from
creation (when the queue starts waiting) to completion, over the last 300 runs:

| day | runs | p50 | p90 | max |
| --- | --- | --- | --- | --- |
| 2026-10-13 | 34 | 16 | 19 | 19 |
| 2026-10-14 | 75 | 13 | 15 | 25 |
| 2026-10-15 | 68 | 14 | 16 | 17 |
| 2026-10-16 | 38 | 14 | 16 | 27 |
| 2026-10-17 | 17 | 24 | 27 | 28 |
| 2026-10-18 | 68 | 26 | 31 | 31 |

(minutes; the p50/p90/max columns count successful runs only.) Over the last
50 runs, failures included, the figures are p50 27, p90 31 and max 54. The lane
fell well inside the window after 2026-10-02 and has been climbing back since
2026-10-17. At p90 31, 60 minutes is **1.9x**, which is already under the
phase-396 rule of 2x. That is the trend this issue says nothing was watching.

### What landed (branch `fix/phase-505-w2-provision`)

- **A declared value.** `scripts/ci/enable-merge-queue.sh` is the one committed
  spelling of the queue settings. A re-run of it with `--with-queue` would have
  silently set the timeout back to 60. It now declares **90** in both places it
  spells the value (the JSON it PUTs and the plan it prints).
- **`just queue-timeout`** (`scripts/ci/queue-timeout-agreement.py`) reads the
  live ruleset (`gh api …/rulesets/<id>`) and the last 50 completed
  merge_group gate runs. It FAILS on any of three conditions: live differs from
  declared, declared is less than 2x the p90, or live is less than the slowest
  observed run. Each FAIL prints the exact ruleset command and never runs it.
  Without `gh`, auth or the network it reports NOT VERIFIED through the
  `nros_check_skip` ledger (fails under `NROS_CHECK_SKIP_STRICT=1`), never OK.
- **Where it runs.** `nightly-report.yml` runs it daily as a step that writes
  the job summary and raises a `::warning` on FAIL. It never goes red, because
  the remedy is an admin's and not the code's. The fast line runs only
  `check queue-timeout-selftest`, which is offline: both spellings of the
  declared value must agree, and the selftest replays 2026-10-02's runs
  (60 min against a 73-minute run, which must FAIL and recommend 150).

Run today, it fails on exactly one count:

```
queue-timeout-agreement: declared 90, live 60; 50 merge_group gate runs p50 27 / p90 31 / max 54 min
[FAIL] the ruleset holds check_response_timeout_minutes=60, but enable-merge-queue.sh declares 90
```

### What is left: one admin action

Raise the ruleset to the declared value. A ruleset PUT replaces the whole
`rules` array, so the command edits the live array in place:

```sh
gh api repos/NEWSLabNTU/nano-ros/rulesets/21680073 \
  | jq '{rules: (.rules | map(if .type == "merge_queue" then .parameters.check_response_timeout_minutes = 90 else . end))}' \
  | gh api -X PUT repos/NEWSLabNTU/nano-ros/rulesets/21680073 --input -
```

The `jq` half was checked against the live ruleset: it changes only that one
field. The PUT was not run. If the API refuses a field of the round-tripped
`pull_request` rule, make the same edit in the web UI under Settings → Rules →
`main-rules` → merge queue → status check timeout. Do **not** use
`enable-merge-queue.sh --apply --with-queue`. Its other queue values (build 4,
merge-min 1) are older than the live ones (5, 3), so it would change more than
the timeout.

Why 90: it is 2.9x today's p90 and 1.2x the worst run this issue measured
(73 min), so a lane back at its 2026-10-02 tail still fits. The cost is a
slower eviction of a really wedged entry (issue 1492's case). 150 (2x of 73)
would keep that cost for a tail that has not returned. The nightly will say
when the lane outgrows 90.

Close this when `just queue-timeout` reports OK.

## Resolution (2026-10-21)

The ruleset `main-rules` (id 21680073) now sets `check_response_timeout_minutes`
to **90**, the value `scripts/ci/enable-merge-queue.sh` declares since PR #2428.
It was changed with one `PUT` built from the live ruleset. A diff of the rules
before and after shows that field as the only change, and enforcement, target,
conditions and bypass actors are unchanged.

`just queue-timeout` (`scripts/ci/queue-timeout-agreement.py`) now passes:

```
queue-timeout-agreement: declared 90, live 90; 49 merge_group gate runs p50 27 / p90 31 / max 33 min
queue-timeout-agreement: OK
```

90 minutes is 2.9x today's p90. It is also 1.2x the 73-minute run that this
issue measured, so a repeat of that run now merges instead of being ejected.
`nightly-report.yml` runs the check daily, so if the lane outgrows the window
again, the check reports it before the queue starts ejecting entries.
