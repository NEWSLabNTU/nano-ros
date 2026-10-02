---
id: 1629
title: "The merge queue's 60-minute `check_response_timeout_minutes` is now
  SHORTER than the gate lane, so entries are ejected `checks_timed_out` while
  their own gate goes on to pass — main has not merged for 2 hours"
status: open
type: bug
area: ci, process
severity: high
found: 2026-10-02
related: [phase-396, issue-1347, issue-1365, issue-1500]
---

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
