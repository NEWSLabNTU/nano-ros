---
id: 1775
title: "Two `nros setup --source` runs for one submodule raced on git's own
  `shallow.lock`, so in a fresh worktree `check fast` failed whichever parallel
  gate lost — red for a reason unrelated to what that gate checks"
status: resolved
type: bug
area: cli, ci
severity: medium
found: 2026-10-10
related: [1304, 1513]
---
## Measured

2026-10-10, `just ci gate` in a freshly provisioned agent worktree. `check fast`
runs its gates in parallel, and `probe-workspace-caps` failed:

```
fatal: Unable to create '…/.git/worktrees/stall-watchdog/modules/packages/rmw/zenoh/zpico-sys/mbedtls/shallow.lock': File exists.
Error: provision source mbedtls
   0: git fetch --depth 1 5e146adef… (packages/rmw/zenoh/zpico-sys/mbedtls, source mbedtls)
```

Solo it passed 5/5. The lock file was gone afterwards — another gate's
provisioning of the same submodule had held it at that moment.

## Cause

`sdk_store::provision_source` checks whether a submodule is present and, if
not, runs `git submodule update --init` (and a depth-1 fetch-by-SHA fallback).
Nothing serialised two PROCESSES doing that for one source. git guards its own
state with lock files and fails the loser instead of waiting, so two callers
racing in an unprovisioned tree gave one success and one hard error. Only a
fresh tree shows it — once the submodule is present every caller takes the
early `AlreadyPresent` return — which is exactly the state every new agent
worktree starts in.

Reproduced on demand: 3 parallel `nros setup --source mbedtls` in a new
worktree, with a CLI built from this tree minus the fix, failed **2/3, 1/3,
2/3** over three rounds, each loser on the same `shallow.lock` line.

## Fix

`provision_source` takes an exclusive advisory lock (`flock`) for the source
BEFORE its presence checks, so a second caller waits and then finds the work
done (`AlreadyPresent`). Where the lock lives is asked, never modelled:

- a workspace source: `git rev-parse --path-format=absolute --git-path
  nros-provision/<name>.lock` — per worktree, which is the race's scope, and
  outside the work tree so it never shows in `git status`;
- a store source (shared between checkouts) and any source of an INSTALLED SDK
  root (issue 1304: no repository, and git would find whatever repository
  encloses it): beside the source's `dest`.

`--dry-run` takes no lock. `File::lock` is newer than the crate's MSRV, so the
lock is a `libc::flock` (unix; a no-op elsewhere).

Same build, with the fix: 0/3 in three rounds, then 0/4 in three rounds.

## Guard

`sdk_store::tests::two_provisioners_of_one_source_wait_for_each_other`: with
one lock held, a second `provision_lock` for the same source must still be
waiting after 400 ms and must get the lock once the first is dropped, and the
superproject's `git status --porcelain --untracked-files=all` must stay empty.
The network race does not reproduce in a unit test; the mechanism does. The
existing installed-root test caught the first version of this fix asking git
inside an installed root.

## Sweep

`git grep -nE 'git( -C [^ ]+)? submodule update' -- just justfile scripts cmake`
— the direct callers (`justfile` setup-worktree, `scripts/bootstrap.sh`,
`just/px4.just`, `scripts/ci/runner-bootstrap.sh`) are setup recipes a person
or a runner runs once, serially; the rest are remedy strings. The parallel
path into provisioning is `nros setup --source`, which this lock covers.
