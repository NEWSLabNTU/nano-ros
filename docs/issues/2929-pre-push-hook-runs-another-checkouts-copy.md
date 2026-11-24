---
id: 2929
title: "An absolute core.hooksPath makes every worktree run the main checkout's pre-push hook, however old, and nothing says so at push time"
status: open
type: bug
area: [tooling, git, multi-session]
severity: low
related: [2583, 0840, 1336]
found: 2026-11-24
---

## What happens

On host 133, `git push origin --delete <branch>` from an up-to-date
worktree was refused:

```text
submodule-pins: FAILED — could not read the gitlinks at 'aea422c406d362e1639a587841a4be08c06affe3' or
  '0000000000000000000000000000000000000000' within 60 s; nothing was compared.

pre-push: REFUSING to push — a submodule pin would move backward.
```

That is issue 2583's symptom, and 2583 is fixed on `main`: `.githooks/pre-push`
has skipped a deletion (`nros_prepush_deletes`, an all-zero local sha) since
6ed9f6e8a6 (2026-11-14). The worktree pushing was at `main` plus two docs
commits, so its own `.githooks/pre-push` has the skip.

The hook that ran was not that one:

```text
$ git config --show-origin core.hooksPath
file:/home/aeon/repos/nano-ros/.git/config	/home/aeon/repos/nano-ros/.githooks
```

`core.hooksPath` lives in the shared config, and on this host it holds an
ABSOLUTE path to the main checkout's `.githooks`. That checkout sits on a
branch whose last change to the hook is 7b95e97ff8 (2026-09-08), so every
linked worktree on the host runs a hook more than two months old. This one
predates 2583, and also 1964 and phase-552's changes.

## Why nothing caught it

- `config/git-settings.txt` wants the RELATIVE `.githooks`, which git resolves
  per worktree, so each one runs its own copy. `just doctor` compares the value
  exactly and would report `[MISSING] git hooks`. But doctor is not on the push
  path, and nobody ran it on this host after the value changed.
- The hook cannot see the drift either. It runs from the stale copy, and
  nothing in it compares itself with the pushing worktree's
  `.githooks/pre-push`.
- The failure reads as a REAL refusal ("a submodule pin would move
  backward"), not as a stale tool. That is how it was first reported: as a
  new hook bug and a duplicate of 2583.

## What would close it

The hook knows which copy it is. When the pushing worktree's own
`.githooks/pre-push` differs from the running one, it says so before any
verdict, naming both paths and the remedy (`just setup-hooks`, which writes
the relative value). Whether it then refuses or carries on is a ruling, but
a verdict from another checkout's hook must not read as this checkout's.
Gate: a self-test that runs the hook through an absolute `core.hooksPath`
pointing at an older copy, in a repo the test builds (the issue-1465 rule).

## Host note

Before this was understood, branch deletion on this host went through
`gh api -X DELETE repos/<owner>/<repo>/git/refs/heads/<branch>`, which moves
no code. The operator then reset the value to the relative `.githooks`
(2026-11-24). Each linked worktree now resolves `git rev-parse --git-path
hooks` to its own `.githooks`. The repo-side defect stands: nothing at push
time would have said so.
