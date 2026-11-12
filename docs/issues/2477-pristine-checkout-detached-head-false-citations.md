---
id: 2477
title: "From a detached HEAD, `ci gate`'s pristine checkout measures commit citations against the clone's stale local `main` and reports landed commits as missing"
status: open
type: bug
area: [ci, testing]
severity: low
found: 2026-11-13
related: [1205, 1476, 1043]
---

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
