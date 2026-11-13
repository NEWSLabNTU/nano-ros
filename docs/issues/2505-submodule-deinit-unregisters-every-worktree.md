---
id: 2505
title: "`git submodule deinit` in one linked worktree unregisters the submodules of every worktree, so tidying a finished agent worktree breaks the live ones"
status: open
type: limitation
severity: medium
area: [git, worktree, multi-session]
related: [issue-1336, issue-1513]
found: 2026-11-13
---

## What

Linked worktrees share the main checkout's `.git/config`. That is where `git submodule init`
records a submodule as `submodule.<name>.active` and `submodule.<name>.url`.
`git submodule deinit` removes those keys. So one worktree's deinit unregisters the
submodule for **every** worktree of the repository: the main checkout and every other agent
session's worktree. The checked-out files stay in place, but `git submodule status` now
prints `-` (not initialised) for each of them.

Running `git submodule deinit --all -f` before `git worktree remove --force` is how agent
sessions here have been tidying finished worktrees, because `git worktree remove` refuses a
worktree that contains submodules. That makes it a multi-session hazard: each session that
finishes a worktree silently de-provisions every other live one.

## Measured

**Scratch clone (2026-11-13).** One super repo with one submodule, cloned, plus two linked
worktrees `wtA` and `wtB`, with the submodule initialised everywhere:

| step | `git config --get-regexp '^submodule\.'` | `git -C wtB submodule status` |
| --- | --- | --- |
| before | `submodule.sub.active true`, `submodule.sub.url …` | ` 9ab2e79… sub (heads/master)` |
| `git -C wtA submodule deinit --all -f` | none | `-9ab2e79… sub` (and the same in the main clone) |

In the same clone, a third worktree `wtC` with an initialised submodule behaved as follows:

- `git worktree remove wtC`: `fatal: working trees containing submodules cannot be moved or removed`.
- `git worktree remove --force wtC`: exit 0. The shared `submodule.*` keys are untouched, and
  `wtB` still reads ` 9ab2e79… sub (heads/master)`.

**In this repo.** Tidying a finished worktree with deinit-then-remove took 5 submodules out of
a live sibling worktree:

- `third-party/nuttx/libc`
- `third-party/nuttx/nuttx-apps`
- `third-party/dds/cyclonedds`
- `packages/cli/third-party/play_launch`
- `packages/cli/testing_workspaces/ros2_rust_examples`

That sibling's next `just ci gate` stopped at `_require-worktree-provisioning` with
"5 unmet provisioning precondition(s)". `just setup-worktree` restored them.

## Why it reads as something else

The victim never ran anything. Its gate fails on provisioning (issue 1513's message), which
reads as "this worktree was never set up", not as "another session just undid it". Each
session re-runs `just setup-worktree`, which re-adds the keys. That masks the cause and
leaves the next tidy-up free to remove them again.

## The safe way to tidy a finished worktree

```sh
git worktree remove --force <worktree>   # no submodule deinit first
git branch -D <branch>                   # if the branch is done
```

`--force` is what lets git remove a worktree that holds submodules. It removes that worktree's
own files and its per-worktree git dir (`.git/worktrees/<name>`, including the submodule
repositories under it). It never edits the shared config. Never run `git submodule deinit` in
a linked worktree. It acts on state that every worktree shares.

## Fix

- **Agents.** Remove a finished worktree with `git worktree remove --force` alone (above).
- **Repo.** A short line in AGENTS.md's multi-session pitfalls would put this where every
  session reads it. Optionally, `just setup-worktree` could report when it is RE-registering
  submodules that a worktree already has checked out, since that is this issue's signature.
