---
id: 2240
title: "A self-test whose TMPDIR sits inside a git checkout runs its git commands against that checkout"
status: open
type: bug
severity: high
area: [ci, tooling]
related: [phase-489, issue-0986, issue-0988, issue-1465, issue-2239]
found: 2026-11-04
---

**Home: [phase-489](../roadmap/phase-489-ci-build-speedup.md)** — homed 2026-11-04: same gate self-tests as issue 2239; the TMPDIR workaround for that issue is what exposed this one.

## What

On 2026-11-04 an agent ran `just check fast` in a developer checkout with
`TMPDIR` pointed at a folder inside that same checkout (`<checkout>/tmp/...`,
which is gitignored), to get round a full `/tmp` (issue 2239). Two things
happened:

- About 315 self-tests failed. Each passes with `TMPDIR=/tmp`.
- Worse, the git self-tests ran against the developer's checkout instead of
  their own scratch repos. Afterwards that checkout:
  - had HEAD detached at a synthetic commit, `three`, that only a self-test
    makes;
  - had a new ref, `refs/nros/fixture-tree-head`, which only
    `scripts/ci/fixture-tree.sh` writes;
  - had lost every remote-tracking ref (a `fetch --prune` from a scratch repo
    with no remotes).

The checkout was repaired by hand: branch checked out again, the ref deleted,
`git fetch` to restore the remote-tracking refs, local config and all 151
branches checked unchanged. The forced checkout rewrote tracked files, so every
prebuilt fixture there read as stale.

Nothing stops the setup that caused it. `TMPDIR` inside a checkout is an easy
thing to do: the project's own rule is "temp files in `$project/tmp/`, not
`/tmp`" (CLAUDE.md), and `/tmp` is a small tmpfs on the maintainer's host.

## Why

Git finds a repository by walking up from the working directory, and it keeps
walking when it meets a `.git` it cannot use. Measured with git on this host,
a scratch folder `outer/t/tree` inside a repo `outer`:

| state of `outer/t/tree/.git` | `git -C outer/t/tree rev-parse --show-toplevel` |
| --- | --- |
| missing | `outer` |
| a directory with a junk `HEAD` | `outer` |
| a file with garbage | `fatal: invalid gitfile format` |
| missing, with `GIT_CEILING_DIRECTORIES=outer/t` | `fatal: not a git repository` |

Under `/tmp` the walk ends at `/` with "not a git repository", so any self-test
step that runs before its scratch repo exists, or after it breaks one on
purpose, fails safely. Under a checkout, the same step finds the enclosing
repo and acts on it. `fixture-tree.sh`'s `sync_tree` is a strong fit for the
damage seen (it does `fetch --prune` into `refs/nros/fixture-tree-head` and
`refs/remotes/origin/*`, then `checkout --force --detach`, against
`git -C "$tree"`), and its self-test covers an "unreadable git dir" case. The
exact step that escaped has not been pinned down, and the 315 failures likely
include other self-tests with the same exposure.

This is the class issues 0986/0988 fixed for an inherited `GIT_DIR`
(`nros_clear_inherited_git_env`), reached by a different door: `GIT_DIR` is
cleared, but discovery itself is not bounded. `check-hook-repo-side-effects`
compares a victim repo byte for byte, but it never puts the scratch folder
inside a repo, so it cannot see this.

## What would close it

- One shared guard, next to `nros_clear_inherited_git_env` in
  `scripts/lib/git-hook-env.sh` and `scripts/lib/git_hook_env.py`: every
  self-test that makes a scratch dir bounds discovery to it (set
  `GIT_CEILING_DIRECTORIES` to the scratch dir's parent), or refuses to start
  when its scratch dir is inside a git worktree, naming `TMPDIR` as the cause.
  Bounding is better: it makes a checkout-local `TMPDIR` work instead of fail.
- `check-hook-repo-side-effects` gains a run with `TMPDIR` inside the victim
  repo, so the gate sees this shape.
- Find which self-tests make up the 315 failures and which of them touched a
  repo, by running `just check fast` with `TMPDIR` inside a throwaway clone
  (never a real checkout).

Acceptance: `just check fast` with `TMPDIR=<a throwaway clone>/tmp/x` leaves
that clone byte-identical (refs, HEAD, config, index, tracked files), and the
self-tests pass or refuse with a message naming `TMPDIR`.
