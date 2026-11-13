---
id: 2477
title: "From a detached HEAD, `ci gate`'s pristine checkout measures commit citations against the clone's stale local `main` and reports landed commits as missing"
status: resolved
type: bug
area: [ci, testing]
severity: low
found: 2026-11-13
related: [1205, 1476, 1043]
resolved: 2026-11-13
resolved_in: "`default_branch_ref()` follows a local `origin` to the source's own `origin/main`, and reports NOT VERIFIED when the source has none"
---

> **Resolved (2026-11-13).** A clone whose `origin` is a local path now
> measures citations against the SOURCE's `origin/main`, never the source's
> local `main`. See "Fix".

## What happens

`just ci gate` run in a worktree whose HEAD is DETACHED failed at step 4,
`check::pristine-checkout`, because `check-doc-commit-citations` printed
"49 citation(s) name a commit this repository does not have". Every one of
the 49 is on `origin/main` (for example `10d35dc209`). The same commit gated
from a NAMED branch passes that step.

Measured 2026-11-12/13 on commit 499b5fc457 (PR #3634): detached HEAD, step 4
FAILED; the same commit on branch `rfc-one-ros-cmake-build`, step 4 ok.

## Why

`scripts/ci/pristine-checkout-fast.sh` builds the clone with
`git clone --local --shared <worktree>`, so the clone's `origin` is the local
repository, not GitHub: its `refs/remotes/origin/*` are the local
repository's `refs/heads/*`. `check-doc-commit-citations.py`
`default_branch_ref()` asks for `refs/remotes/origin/HEAD`, then
`refs/remotes/origin/main`:

- from a named branch, the clone records `origin/HEAD` as that branch, whose
  history holds every landed commit, so citations resolve;
- from a detached HEAD the clone has no `origin/HEAD`, so the check falls to
  `origin/main`, which is the local repository's `main` branch. Here that
  branch was last moved 2026-09-28 (`e3de8bda14`), 3,721 commits behind
  `origin/main`, and does not contain `10d35dc209`.

So the check answers "is this cited in the local `main` branch", which nobody
maintains in an agent setup where work happens on worktree branches. Issue
1205 chose the default branch on purpose; the clone's view of "the default
branch" is what is wrong here.

It is not truncation (issue 1476's class): the clone is not shallow
(`--shared` sees the whole object store), so `git_history.is_truncated()`
says false, correctly.

## Fix direction

Give the pristine clone the source repository's own default-branch view:
after cloning, set the clone's `refs/remotes/origin/main` (and `origin/HEAD`)
from the SOURCE's `refs/remotes/origin/main`, not its `refs/heads/main`. Or
have `default_branch_ref()` refuse to treat a local clone's `origin/main` as
the default branch when the source's `origin/main` exists. A negative control:
gate from a detached HEAD with a stale local `main`, and expect step 4 to
pass.

## Fix

`check-doc-commit-citations.py` `default_branch_ref()` now asks whether
`origin` is a local path (`_local_origin`). When it is, the clone's
`refs/remotes/origin/*` are the source's local branches, so the function
follows the source and returns the commit the SOURCE's default-branch ref
(`origin/HEAD`, else `origin/main`) names. It follows at most four local
hops, and checks that the commit exists in the clone. When the source has no
upstream view, it returns `None` and the gate exits 78: NOT VERIFIED (issue
1043's third outcome), recorded in the skip ledger. It never answers against
a stale local branch.

This also fixes the named-branch case, which passed for the wrong reason.
There the clone's `origin/HEAD` was the source's checked-out work branch, so
that branch's unlanded commits resolved, against issue 1205's rule.

Negative control: `local_clone_self_test`, which runs on the normal path. It
builds a source whose local `main` is stale and whose `origin/main` is
current, clones it `--local --shared` and detaches the clone, then expects
the source's `origin/main`. With the source's remote removed, it expects
`None`. With the local-origin branch reverted, both cases fail.

Found on the way: the self-test's "object present but unreachable" case ran
`git commit-tree` in the checkout under test, so every run of the gate wrote
an object into that repository's store. `check-hook-repo-side-effects` had
never run this script, because it only covers scripts that build a
repository. Once the new fixture added a `git init`, it refused the script.
That case now builds its object in the fixture repository.
