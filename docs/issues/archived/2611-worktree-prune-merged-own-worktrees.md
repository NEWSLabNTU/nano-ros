---
id: 2611
title: "Finished agent worktrees are never removed, so their build output fills the host (/home at 100 % on 2026-11-15)"
status: resolved
type: tech-debt
severity: medium
area: [ci, tooling]
related: [phase-489, 2602, 2505]
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that filed it"
---

**Home: [phase-489](../../roadmap/phase-489-ci-build-speedup.md)**, beside the
gate host lock (issue 2602): both are about one host shared by many gates.

# Merged worktrees keep their build output

## Symptom

Every agent session makes a linked worktree per task, and one that has run
`just ci gate` holds 50-300 GB (measured 2026-11-15: 285 GB, 206 GB). When the
task's PR merges, nothing removes it. On newslab-243 that day `/home` reached
100 % with 11 GB free, and a gate went red on its own disk floor
(`nextest-gates: 10 GiB free … below 30 GiB`) before reaching any code. Three
merged worktrees of one session, removed by hand, freed 675 GB. Another
session (nros wtf) had to ask this host to run its gate because its own disk
was full.

The hand removal was the risky part: a worktree holds untracked nested
repositories (provisioned sources such as `third-party/nuttx/libc`,
checked-out submodules), so `git worktree remove` refuses and `--force` is
what removes it, which is also what removes unsaved work.

## Fix

`just worktree-prune` (`scripts/worktree-prune.py`). Dry run by default;
`--apply` removes. A linked worktree is removed only when all of these hold:

- **Yours.** Git records no creator, so ownership is a marker,
  `nros-owner` in the worktree's private git dir (never in the tree).
  `just setup-worktree`, the step every agent worktree runs, writes it for
  whoever runs it when no owner is recorded yet; `just worktree-prune --mark
  <path>` does it by hand. The caller is `NROS_WORKTREE_OWNER`, else
  `CLAUDE_CODE_SESSION_ID`. An owned worktree is never re-attributed, the main
  checkout is never marked, and with no identity nothing is written. No marker
  or another owner's: listed and skipped, never assumed.
- **Merged.** Every commit on HEAD is in `origin/main` by patch-id
  (`git cherry`, so a queue rebase-merge counts), or the branch's PR is merged
  (`gh`) and every commit on HEAD is on a remote.
- **Clean.** No tracked change, no untracked file. A nested repository counts
  as clean only if it is clean itself and its HEAD is on one of its own
  remotes.
- **Not special.** Never the main checkout, the worktree the caller is in, or
  a locked one; never `git submodule deinit` (issue 2505).

`git worktree remove` runs without `--force` first; `--force` only when git
refused over nested repositories that the checks above already proved clean
and pushed. The reason for a skip is printed per worktree, with the size of
each candidate.

`just ci gate` prints one line at its end when a worktree of yours that
`--apply` would remove is over 10 GB. It is advisory and cannot change the
verdict.

## Test

`just check worktree-prune` (fast line, ~0.5 s) builds scratch repositories
and asserts each refusal: no marker, another owner, a tracked edit, an
untracked file, an unmerged commit, a commit after the merge, a merged PR
with an unpushed commit, a dirty or unpushed nested repo, a dirty submodule,
a locked worktree, no caller identity, `--mark` over another owner or on the
main checkout, `--mark-if-absent` re-attributing, marking the main checkout or
writing with no identity, the current worktree, and `remove()` forcing past a refusal
that is not about nested repos. Positive cases: a rebase-merged branch, a
merged PR, a clean nested repo, a checked-out submodule. Dry run removes
nothing; `--apply` removes exactly the candidates. Each guard was
mutation-checked (removing it fails the self-test).

## Caveat

A worktree made before this change has no marker, so it is skipped until it
is marked: on its next `just setup-worktree` run, or by a one-time
`just worktree-prune --mark <path>` from its owner. Ownership is never
guessed; an unmarked worktree stays listed and skipped.

## Follow-up: a published pin read as unpublished (2026-11-15)

The first version proved a nested repository's HEAD published with
`git rev-list HEAD --not --remotes`, which asks the clone's LOCAL
remote-tracking branches. That answers "what has this clone fetched", not
"what exists", so two shapes of a published pin read as "HEAD is on none of
its remotes" and kept every merged worktree on the host:

- **A pin at a release TAG.** `third-party/freertos/kernel` is pinned at
  upstream `V11.2.0`, which no branch reaches, so it was refused in every
  gated worktree.
- **A clone whose own `origin` is not the declared URL.** The
  `third-party/threadx/kernel` clone can point at upstream while its pin is
  the tip of our fork's `nros-lp64-ulong`, which only the `.gitmodules` URL
  names.

`unpushed()` now keeps the local check as a fast path. When it leaves
commits, it asks each configured remote AND the `.gitmodules` URL with
`git ls-remote`, and subtracts every advertised tip that is a commit here.
A remote that cannot be asked contributes nothing, so unproven still means
unpublished. What it still cannot prove without a fetch is a pin that is an
ancestor of a remote ref whose tip this clone never fetched. That stays
refused, which is the safe direction.

The self-test adds a tag pin and a declared-fork pin, both removable, and a
local commit on top of the tag pin, which is refused. Its negative control
asks no remote and requires both pins to read unpublished. Measured on this
host: the FreeRTOS kernel pin went from 4 unpublished commits to 0, in 1.4 s.

A third refusal had the same root but a different shape: an untracked build
output inside a nested repo, which nros 241 hit with NuttX. `git status`
already leaves out what the nested repo ignores. NuttX's own `.gitignore`
covers its kernel `.config`, objects and `System.map` (668 ignored paths in
the main checkout's tree). What remained were six paths that nano-ros's own
NuttX build writes at that tree's root and that NuttX does not ignore:
`.nros-nuttx-build-head`, `.nros-nuttx-build-head-<arch>`,
`.nros-nuttx-build.lock` and `nros-nuttx-export-<config id>/`. They are listed
by name in `NROS_GENERATED_IN_NESTED`, for `third-party/nuttx/nuttx` only,
with their producers cited. Any other untracked file is still refused,
because it may be a user's new source.

A fourth shape, from nros 139: a shared module store whose clone still has
UPSTREAM threadx as `origin`, while the pin `9a29f1bd3f` lives on our fork.
That is handled offline, before any remote is asked. A nested HEAD equal to
a gitlink that `origin/main` has recorded at that path (its current pin or
an earlier one, `git log origin/main -- <path>`, up to 500) counts as
published. CI's `check-submodule-commits-reachable` refuses a pin that is on
no remote, so main's pins are published by construction. The order of proof
is now: local remote-tracking refs, then main's pins (0.2 s for threadx,
offline), then `ls-remote` over the clone's remotes and the declared URL.
