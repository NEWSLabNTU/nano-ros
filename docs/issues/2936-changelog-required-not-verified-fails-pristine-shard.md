---
id: 2936
title: "`changelog-required` cannot read a dispatched branch inside `pristine-checkout`, and its NOT VERIFIED fails the host-suite shard"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-11-24
related: [2612]
---

## What happens

A `host-suite` run dispatched on a branch (`gh workflow run host-suite.yml
--ref <branch>`, the phase-591 practice for proving a lane fix before the cron)
failed its last `check-build` shard in `just check pristine-checkout`. The only
red gate in the clone was `changelog-required`:

```
[FAIL] changelog-required: NOT VERIFIED — cannot read the branch here:
`git diff --name-status --no-renames 15e11ffbadaa68bf0b644aba115045a532e372c2 HEAD`
exited 128: fatal: unable to read tree 1f2c75463bfdf02b1aaea90b1d2930f9cb3f3...
```

Run 45331627283, job `check / build tier + no-std [5/5]` (2026-11-24, branch
`ci/phase-591-w6-w8` at 756876b5a3). The four-shard dispatch of the same branch
two hours earlier (run 45319129706) passed the same step, when `main` had not
yet moved past the branch's base. Scheduled runs on `main` pass it: there the
merge base IS `HEAD`, so the diff reads no tree.

## Why

`scripts/changelog-audit.py --require` passes outright only on `merge_group`
and `push` ("the pull request was judged before it queued"). On every other
event it diffs the branch against its merge base with `$NROS_MAIN_BASELINE`.
`scripts/ci/pristine-checkout-fast.sh` hands the clone that baseline (the
job's `origin/main`), but it also unsets `GITHUB_EVENT_NAME`, so the inner
gate cannot know it runs under a dispatch. The job's checkout is shallow
(`actions/checkout` depth 1, plus the pin baseline fetched for
`check-submodule-pins`). `git merge-base` then finds a base, but the trees
between it and `HEAD` are not in the shared object store, and `git diff`
fails.

The gate's answer is right in kind: NOT VERIFIED, through the skip ledger. But
a NOT VERIFIED inside `pristine-checkout` reaches the shard as a failure, so a
gate that could not answer failed a lane whose job is something else (does the
fast tier pass with no submodules).

## Why it matters

phase-591 W2 makes a branch or `main` dispatch the way a lane fix is proven
before the cron, and release evidence is a dispatch on `main` at the release
commit. Every branch dispatch of `host-suite` after `main` moves fails its last
build shard on this, a red with nothing to fix in the branch.

## Directions (not decided)

1. `changelog-required` answers PR branches only. On `schedule` and
   `workflow_dispatch` it passes with the reason, as it does on `merge_group`
   and `push`, because no pull request is being judged there.
   `pristine-checkout-fast.sh` must then hand the event in (or a
   `NROS_CHANGELOG_EVENT`), since it strips `GITHUB_EVENT_NAME`.
2. Or `pristine-checkout` gives the clone the history the gate needs: fetch
   the merge base's trees into the source checkout before cloning. This costs
   a deeper fetch on every run, for a gate that the pristine question
   (no submodules) does not need.
3. In either case, a NOT VERIFIED inside the pristine clone reads as a
   reported skip, not a shard failure, for every `needs:git-history` gate.
   Measured: this one was the only gate in that class to hit it here.

## Closes when

A `host-suite` dispatch on a branch whose base `main` has moved past passes
`pristine-checkout`. The clone either reports `changelog-required` as not
applicable to the event, or reads the branch. A test replays the shallow-clone
case.
