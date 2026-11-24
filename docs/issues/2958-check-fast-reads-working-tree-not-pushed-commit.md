---
id: 2958
title: "`just check fast` and the pre-push hook judge the WORKING TREE, not the commit being pushed, so an unstaged edit can make a push green locally and red on hosted CI"
status: open
type: bug
area: [ci, gates]
severity: low
found: 2026-11-25
related: [0840, 0196, 2955, 1964, phase-489]
---

**Home: [phase-489](../roadmap/phase-489-ci-build-speedup.md)**, which owns
the fast line's run time and its gate issues (2669, 2955 shape).

## What happened

PR #4821 archived issue 2955. The issue file was edited first (`status:
resolved`, a Resolution section), then moved with `git mv` into
`docs/issues/archived/`. `git mv` stages the file as it is in the INDEX, so
the edits stayed unstaged and the commit archived an issue that still said
`status: open`.

`just check fast` in that worktree was green (664/664), and the branch was
pushed. Hosted `check / fast` on the PR was red:

```
check-archived-issue-status: 2955-hook-gate-changelog-audit-probe-near-budget.md:
frontmatter says `status: open` — an archived issue is not open
```

The local run read the working tree, which had the edits; CI checked out
the commit, which did not. The fix was a follow-up commit (force-push is
not available to agents here).

In this instance the pre-push hook was not installed in that worktree
(`core.hooksPath` unset), so the green came from a manual
`just check fast`. The hook runs the same recipe in the same checkout
(`.githooks/pre-push`: `cd "$repo_root" && just check fast`), so it has the
same reach: it would have passed this push too.

## Why it is a class, not a slip

This is issue 0196's reach gap in a new place: the gate's SUBJECT (the
working tree) is wider than the thing the rule is about (the pushed commit).
Any difference between the two passes locally and fails on CI:

- an edit left unstaged by `git mv`, `git add -p`, or a forgotten path;
- an untracked new file a gate needs (an issue file, a fragment, a golden);
- a deleted file whose deletion was never staged.

The pre-push hook exists (issue 0840) to refuse exactly the reds that are
true in the author's own tree. These are not true in the tree; they are
true in the commit, which nothing local examines.

The committed tree IS examined in one lane: `gate::check::pristine-checkout`
in `just ci gate` clones the commit and runs the fast tier there
(`NROS_GATE_SCOPE=pristine`). That lane costs ~6 min, and `ci gate` is not
what anyone runs before pushing a docs-only change.

## Proposed fix (not built)

Cheapest first; the first alone covers every case above.

1. **The pre-push hook refuses a push whose working tree differs from the
   commit it pushes, for tracked paths.** Before running `check fast`:
   `git diff --quiet HEAD --` (staged or unstaged edits to tracked files) and
   `git ls-files --others --exclude-standard` filtered to paths a gate
   reads. On a difference it names the paths and says the gates would judge
   the working tree, not the push, with `git stash push -u -m …` /
   commit / `NROS_SKIP_PREPUSH_CHECKS=1` as the ways forward. It costs a few
   milliseconds and changes no gate. Caveat to settle before building: the
   pushed ref is not always `HEAD` (`git push origin other-branch`); the hook
   receives the local SHA on stdin and should compare against that, refusing
   only when it equals `HEAD`.
2. **Narrower, if (1) is judged too strict:** refuse only when the
   difference touches `docs/issues/` or `changelog.d/`, the two trees whose
   gates read frontmatter and file presence and which are edited by hand
   right before a commit.
3. **Not proposed:** running `check fast` against a `git worktree` or clone
   of the pushed commit in the hook. That is what `pristine-checkout`
   already does, and it is minutes, not milliseconds; a hook that slow gets
   `--no-verify`'d (the hook's own comment).

Whichever is chosen, `check-hook-repo-side-effects` must still pass: the
check reads git state only and writes nothing.
