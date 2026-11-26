---
id: 2988
title: "The pristine-checkout clone got main's TIP as its pin baseline, so every gate of a branch behind main read main's additions as removals"
status: resolved
resolved: 2026-11-27
resolved_in: "the change that adds this issue"
type: bug
severity: high
area: [ci]
related: [issue-1912]
found: 2026-11-27
---

## What happened

`gate::check::pristine-checkout` failed on every branch whose base was older
than d6a692b849 (#4898, which added NROS-0012, -0013 and -0014):

```text
NROS-0012/0013/0014 removed from codes.lock
```

In the same run, `fast::check::diagnostics` passed on the same tree. On
2026-11-27 it turned three real gates red: #4892 and #4919 on host 142, and
#4777 on host 241.

## Why

`scripts/ci/pristine-checkout-fast.sh` runs the fast lane in a
`git clone --local --shared` of the worktree. It resolves the baseline in the
source checkout and hands it to the clone as `NROS_SUBMODULE_PIN_BASELINE`.
That baseline was `origin/main` itself: the tip, not the branch point.

Every in-tree reader of that variable resolves it as the branch point
(`scripts/ci/pin-baseline.sh resolve`: CI's event base, else
`merge-base(HEAD, origin/main)`). The readers are:

- `check-diagnostics`;
- `check-toolchain-paths-persist`;
- `submodule-pins-check.sh`.

So in the clone, a branch behind main was compared with main's tip, and
anything main had gained since looked like the branch removing it. For
`codes.lock`, that is a removed code.

The tip was chosen to fix an older defect (68c638b3fd): the clone's own
`origin/main` is the source repository's LOCAL `main`, which can be hundreds
of commits stale. That fix was right about which `origin/main` to read, and
wrong about what to read from it. It is the same class as issue 1912 (a
baseline that is main's live tip instead of the branch's base), on a road
1912's fix did not reach.

## Fix

The clone gets `merge-base(HEAD, <candidate>)`. The candidate is the caller's
own `NROS_SUBMODULE_PIN_BASELINE` when one is set, else
`pin-baseline.sh resolve HEAD`, the resolver the in-tree gates use. A
candidate that is already an ancestor of HEAD is unchanged, and a tip can no
longer arrive as a baseline.

A selftest runs on the normal path (about 0.1 s). It builds a scratch
repository with a branch one commit behind main and requires the branch
point for both candidates: main's tip given explicitly, and the resolver.

The selftest is the script's only `git init`, so `check-hook-repo-side-effects`
now probes the script, and it does so with the new `--self-test` flag, which
runs that selftest alone. The normal path runs the whole fast lane, which no
probe budget fits; measured, both probe shapes were killed at 120 s before the
flag existed. Negative control: reverting the resolver to hand the candidate
through unchanged (the old behaviour) fails the selftest, naming main's tip
and the wanted branch point.

`NROS_MAIN_BASELINE` stays main's tip on purpose: its reader
(`changelog-audit.py branch_base`) takes the merge-base itself.

## Measured

The fixed script ran in a worktree at 7ae7d47f7a, the parent of d6a692b849
(behind main, before the three codes). In the clone, `diagnostics` passed.
Before the fix it failed with the message above.

Two other gates were red in that run, and neither comes from this defect:

- `prose-issue-refs`: this issue's file was not written yet.
- `hook-repo-side-effects`: the gate ran 209 s at a host load of 141
  runnable. That is the `changelog-audit.py` probe hitting its budget, which
  #4926 (issue 2955) fixes. The old branch does not have that fix.
