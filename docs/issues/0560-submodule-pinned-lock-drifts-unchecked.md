---
id: 560
title: "A lock pinned against a submodule-controlled dep drifts when the pointer moves, and only the 40-minute lane finds out"
status: open
type: tech-debt
area: build, tooling
related: [issue-0363, issue-0285, phase-332]
---

## What happened

`just setup-launch-resolve` could not build on main. The `play_launch` submodule
was advanced to `1792e7d3` ("rlm v0.1.5 -> v0.1.6"), whose
`src/ros-launch-resolve/Cargo.toml` requires `ros-launch-manifest` at tag
**v0.1.6** — but `packages/cli/nros-launch-resolve/Cargo.lock` still pinned
**v0.1.4**, last touched by `82c88ef53`, the v0.1.4 bump. With `--locked`
injected project-wide by the `scripts/bin/cargo` shim, cargo refuses:

```
error: cannot update the lock file …/nros-launch-resolve/Cargo.lock
       because --locked was passed to prevent this
```

The instance is fixed (`567101c43`, via `just lock-update`). This issue is the
two structural reasons nobody noticed.

## Reason 1 — a stale worktree MASKS it

The break is invisible to anyone whose submodule worktree is behind. Mine sat at
`0cd95a0` (v0.1.4), which happened to match the stale lock, so the resolver built
fine for a whole session. `git submodule update` brought the worktree to the
recorded v0.1.6 and the failure appeared on the next build.

That is the inverse of the usual submodule hazard, and worth stating plainly:

* worktree BEHIND the pointer → hides a lock/pointer mismatch, and (per
  `a29a4441e`) lets a `git add -u` silently revert someone's bump;
* worktree AHEAD → the ordinary "local commit not pushed" problem CLAUDE.md
  already covers.

Only the second is documented. The first is what bit here.

## Reason 2 — the only consumer is behind the expensive lane

`setup-launch-resolve` is a dependency of exactly one thing:

```
$ git grep -n 'setup-launch-resolve' -- justfile just | grep -v echo
justfile:1738:build-test-fixtures … generate-bindings setup-launch-resolve build-zenoh-posix-fixture …
justfile:3825:setup-launch-resolve:
```

`check-fast` never builds it; neither does `just check`. So a bump that breaks it
is detected only when somebody runs the ~40-minute fixture lane — far from the
commit that caused it, and by whoever happens to run it next rather than by the
author. Between `82c88ef53` and the discovery, main carried an unbuildable
recipe.

## The gate this wants, verified before proposing it

`cargo metadata --locked` in that leaf reproduces the failure in **seconds**, no
build:

```
$ cd packages/cli/nros-launch-resolve && cargo metadata --locked --format-version 1
# with the pre-fix lock:  rc=101, "cannot update the lock file … --locked"
# with the fixed lock:    rc=0
```

Tested both ways against the actual pre-fix lock (`567101c43~1`), not assumed.

So: add a `check-*` that runs `cargo metadata --locked` over the leaves whose
dependency versions are controlled from OUTSIDE their own tree —
`nros-launch-resolve` is the one today, because its manifest lives in the
`play_launch` submodule and names an rlm tag. Wire it into `check-fast`, where it
costs seconds and fires on the commit that breaks it.

Do NOT solve this by making `check-fast` build the resolver — that trades a
seconds-long resolution check for a compile, and the compile is not what was
missing. Resolution is exactly the thing that broke.

## Why this is a class, not a one-off

Any lock whose dependency versions are decided by a submodule's manifest can
drift the moment the pointer moves, and the two halves are updated by different
commits — `82c88ef53` moved the lock, a later commit moved the pointer, and
nothing related them. It is the same shape as the fixture-manifest work in
phase-350: two sides of one fact, no predicate tying them together.
