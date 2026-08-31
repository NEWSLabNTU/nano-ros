---
id: 975
title: "`--self-hosted-ready` requires a merge_group-only check, so no PR can enter the queue"
status: resolved
area: ci
severity: high
found: 2026-09-01
resolved: 2026-09-01
related: [0883, phase-410, phase-395]
---

# Symptom

After `just merge-queue --apply --self-hosted-ready`, every open PR sits at
`mergeStateStatus=BLOCKED` with `mergeable=MERGEABLE`, the merge queue is empty,
and nothing merges. It does not present as an error anywhere — no red check, no
message on the PR beyond "merging is blocked". It reads as GitHub being slow.

Measured immediately after the flip, with `CI` green on the PR:

```
required set:  CI
               L3 (cross build + link)

#173 BLOCKED   #172 BLOCKED   #171 BLOCKED   #170 BLOCKED
#168 BLOCKED   #166 BLOCKED   #162 BLOCKED   (#163 DIRTY, unrelated)

merge queue entries: (none)

statusCheckRollup for #172:
  changes                                     SUCCESS
  check (fast on push; full on PR/nightly)    SUCCESS
  CI                                          SUCCESS
  ...                                         (no L3 entry at all)
```

# Cause

`SELF_HOSTED_CHECKS` adds `L3 (cross build + link)` to the ruleset's required
status checks, but `queue.yml`'s `l3` job triggers only on `merge_group` and
`workflow_dispatch`. It therefore **never reports on a pull request**. A PR
cannot satisfy a required check that never runs against it, so it cannot enter
the queue — and because it never enters the queue, the `merge_group` event that
would run `l3` never fires. The deadlock is self-sustaining.

The script already knows this. Eight lines above the list, its own comment says:

> `ci-ok` runs with `if: always()`, and inspects `needs.*.result`, so it ALWAYS
> reports and the required set never has to change when a job is added, renamed,
> filtered or skipped. That is the fix for the class that froze this repo four
> ways in one day — a required check that produces no verdict blocks forever.

`--self-hosted-ready` then does the exact thing that comment forbids. So does
CLAUDE.md:

> ONE required check, the aggregator `CI` — never add a job name to the required
> set, and never path-filter a required workflow: a check that produces no
> verdict blocks forever, which deadlocked two PRs on 2026-08-28.

This is the fifth instance of that class, and the first where the *guard against
it* is what introduced it.

# Why it looked safe

The intent behind `--self-hosted-ready` is sound: once a runner exists, the
heavy lane should actually gate merges rather than being advisory. The flaw is
in *where* the gating is expressed. `L3` is already covered by the aggregator —
`ci-ok` inspects `needs.*.result`, so an L3 failure inside a merge group turns
`CI` red without `L3` being named in the required set. Naming it adds no
enforcement and costs the ability to enqueue.

The dry run does not reveal it either: it prints the planned required set, which
looks correct in isolation. Nothing compares that set against the triggers of
the workflows that produce it.

# Fix

1. **Immediate**: drop `L3 (cross build + link)` from the required set. The
   sanctioned path is `just merge-queue --apply` *without* `--self-hosted-ready`
   — `required_status_checks` is replaced wholesale from `contexts`, and without
   the flag `contexts` is just `("CI")`.
2. **Structural**: `--self-hosted-ready` should gate *whether the self-hosted
   jobs run* (it already does, via `vars.NROS_SELF_HOSTED_READY`) and never
   touch the required set. `SELF_HOSTED_CHECKS` should be empty, or the flag
   should stop feeding `contexts` entirely.
3. **Gated**: a check may be required only if some workflow produces it on a
   `pull_request`. That is statically decidable — parse each workflow's triggers
   and job names and compare against the ruleset's required contexts. Without
   this, the next person re-adds a plausible-looking context and the repo
   freezes again with no error message.

Note the gate must read the *effective* reporting condition, not just the job
name: a job that exists on `pull_request` but carries an `if:` that can evaluate
false still produces no verdict, which is issue 0883's shape one level down.

# Reproduction

```sh
just merge-queue --apply --self-hosted-ready
gh pr view <any-open-pr> --json mergeStateStatus,mergeable
#   -> BLOCKED / MERGEABLE, with CI green and no L3 entry in the rollup
```

# Confounder ruled out

Two other explanations were checked and rejected before landing on this one:

- **`require_extra_approval_for_unattributed_changes: true`** with
  `required_approving_review_count: 0`. Rejected: the last commit's authors both
  resolve to real accounts (`jerry73204`, `claude`), so the changes are
  attributed.
- **The merge queue not being enabled at all.** The apply run printed
  `merge queue: NOT added (pass --with-queue)`, which looked decisive. Rejected:
  the ruleset does carry a `merge_queue` rule — that line describes the
  *queue-parameter* block the run did not rewrite, not the rule's absence.

The remaining evidence is positional: the queue held entries before the flip
(#157 reached position 1) and holds none after, with the required set the only
thing that changed. Confirming it is one command — apply without the flag and
watch the PRs unblock — but that mutates repo settings, so it is left to a
human rather than guessed at.


# Resolution (2026-09-01)

Confirmed by experiment before fixing: applying the ruleset WITHOUT
`--self-hosted-ready` moved six PRs from `BLOCKED` to `CLEAN` within 20 s, and
the queue began accepting entries again. The positional evidence in the section
above is therefore now direct.

Three changes:

1. `--self-hosted-ready` no longer appends to the required set. It sets
   `vars.NROS_SELF_HOSTED_READY`, which is what makes the self-hosted jobs run —
   that is the whole flag. `SELF_HOSTED_CHECKS` survives as a DESCRIPTIVE list
   so the plan output can still name what the flag turns on.
2. The plan text stopped saying "Add them with --self-hosted-ready", which had
   become false, and now states that these lanes gate through the `CI`
   aggregator whichever way the flag goes.
3. New gate `check-required-contexts-reportable` (fast line). It reads the
   script's declared arrays — not the live ruleset, since a gate that needs the
   network is a gate that gets skipped — and rejects a required context that is
   produced by no job, or only by jobs without a `pull_request` trigger.

The gate also rejects a required job whose `if:` depends on `vars.`/`secrets.`.
That is the same defect one level down: `l3` carries
`if: vars.NROS_SELF_HOSTED_READY == 'true'`, so even had it run on
`pull_request`, flipping a variable in Settings would silently stop a REQUIRED
check from reporting with nothing in any diff to show it. Issue 0883's shape.

Verified against the real regression rather than a synthetic one: re-adding
`L3 (cross build + link)` to `HOSTED_CHECKS` makes the gate red with the
diagnosis naming both the workflow and its triggers. Its selftest covers four
shapes (reportable, merge_group-only, variable-gated, produced-by-nothing) and
runs on the normal path.

What this does NOT do: nothing verifies the LIVE ruleset against the script.
A context added by hand in the GitHub UI is still invisible here. That is
deliberate — the alternative is a network-dependent gate — but it means the
script has to stay the only way the required set is edited.
