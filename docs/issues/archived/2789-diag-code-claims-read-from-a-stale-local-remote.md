---
id: 2789
title: "The pre-push diagnostic-code check read claims from a local repository's stale refs, so a code claimed since its last fetch refused a valid push"
status: resolved
type: bug
area: [tooling, ci, hooks]
severity: medium
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "this PR (scripts/ci/diag-codes-check.sh reads a local remote's own origin)"
---

## Measured

Host 139, 2026-11-21, pushing a rebase of an unrelated branch onto a main
that had just gained NROS-0011 (`RetiredKnobSpelling`). The pre-push fast
tier failed in `check hook-repo-side-effects`:

```
diag-codes-check: packages/tooling/nros-diagnostic-codes/codes.lock:
  NROS-0011 (RetiredKnobSpelling) is new here and has no claim (refs/diag-codes/0011) on the remote
pre-push: REFUSING to push — a diagnostic code is unclaimed or reused.
```

`refs/diag-codes/0011` existed on origin. The checkout's own
`refs/diag-codes` held only the claims it had fetched before, not 0011.

## Cause

`check hook-repo-side-effects` runs the real `pre-push` in a `--local
--shared` clone of the checkout, so the hook's remote `origin` IS the
checkout, a local repository. `diag-codes-check.sh` lists claims with
`git ls-remote <remote> 'refs/diag-codes/*'` and believes the answer.

It already handled a stand-in remote that holds NO claim: it reports NOT
VERIFIED. A local repository usually holds a stale SUBSET, which looks
authoritative. Every code claimed since its last fetch reads as unclaimed.
That happens on any host the next time a code lands, whatever branch it
pushes.

## Fix

Claims live on the remote everyone pushes to, never on a local repository.
When the push's remote is a local repository, the check asks that
repository's own `origin` (one hop) for the claim listing. A bare scratch
remote has no `origin` and is asked itself, which keeps the existing
self-test shape. An unreachable upstream is NOT VERIFIED, as before.

The self-test gains the case that failed: a local remote holding only an old
claim, whose origin holds the new one, must pass. With the hop removed, that
case refuses the push, as measured. End to end: with the checkout's
`refs/diag-codes/0011` deleted, `check hook-repo-side-effects` failed before
the fix and passes after it.
