---
id: 2743
title: "Under `git bisect run` / `git rebase -x`, `just setup-worktree` and the lane preflight report every submodule as an INCOMPLETE checkout"
status: resolved
type: bug
area: [gates, git, tooling]
severity: medium
found: 2026-11-20
resolved: 2026-11-20
resolved_in: "the change that filed it"
related: [issue-0986, issue-1659, issue-2306, issue-2701, issue-0196, issue-1452]
---

## What happens

A bisect of the `custom_transport_loopback` tier-1 red ran `just setup-worktree`
as the first thing in each step. Every step after the first was skipped:
setup refused with

```
submodule-drift: … third-party/qemu/qemu is an INCOMPLETE checkout: 14864 tracked file(s) absent or staged as deleted; …
setup-worktree: the dev lane is not fully provisioned (above).
```

for all 20 submodules, so the bisect reached no verdict at all. The submodules
were intact: `git -C third-party/qemu/qemu status` was clean in the same
worktree, from an ordinary shell.

## Why

`git bisect run` (and `git rebase -x`) exports the worktree's `GIT_DIR` to the
command it runs, with no hook involved. Measured, git 2.34:

```
GIT_DIR=/…/.git/worktrees/nros-wt-2558
GIT_PREFIX=
GIT_EDITOR=true
```

An inherited `GIT_DIR` overrides `git -C <path>` for reads as well as writes.
`nros_submodule_provision_state` asks each submodule
`git -C "$p" ls-files --deleted` and `git -C "$p" diff --cached … HEAD`; with
`GIT_DIR` set, both answer about the SUPERPROJECT's index and HEAD, so every
submodule reads as its whole tree deleted.

Issue 0986 made every script that BUILDS a repository clear the inherited
environment, and issue 1659 did the same for every git the CLI spawns. Neither
reached a shell or Python entry that only READS: `check-hook-repo-side-effects`
said so in its own comment ("Reading the wrong repository under an inherited
GIT_DIR is a correctness bug and worth clearing for, but it is not a repository
side effect").

## Fix

The class is "an entry that provisioning or the lane preflight starts, and
that runs git". Each clears the inherited environment once, at its head, with
the shared `nros_clear_inherited_git_env` (shell or Python twin). What it runs
inherits the cleared environment. Sourced libraries do not clear: that would
rewrite their caller's environment, and their caller is an entry.

- `just setup-worktree`
- `just _require-worktree-provisioning` (the preflight's provisioning probe)
- `scripts/lib/claim-provision.sh` (also CI's provisioner entry)
- `scripts/check/gate-lane-preflight.py`
- `scripts/check-submodule-drift.sh` (also run on its own and by the tier preconditions)

`check-hook-repo-side-effects` gains a READ-side section:

- **Static.** It derives the entries from the command lines of the two roots,
  `setup-worktree` and `gate-lane-preflight.py`, one level deep, with comment
  lines dropped. Any entry that runs git, directly or through a library it
  sources, must call the helper.
  A transitive text harvest was measured first and rejected: it reached 70-odd
  scripts through `just` recipes named in remedy text, which is issue 1452's
  "reach wider than the rule".
- **Dynamic.** It builds a superproject with one submodule and a linked
  worktree, exports that worktree's `GIT_DIR` the way `bisect run` does, and
  runs `check-submodule-drift.sh` from a copy of the checkout's scripts. The
  control is the same copy with the clearing call removed. The control must
  report the false INCOMPLETE; otherwise the fixture no longer reproduces the
  shape.

Mutation, measured: with the five entries reverted to `main`, the gate fails
with 4 static findings and the dynamic one. `setup-worktree` runs no git of its
own, so the static rule rightly does not name it.
