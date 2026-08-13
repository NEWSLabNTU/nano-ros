---
id: 560
title: "A lock pinned against a submodule-controlled dep drifts when the pointer moves, and only the 40-minute lane finds out"
status: resolved
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

## Resolved 2026-08-13

`scripts/check-submodule-pinned-locks.py`, wired into `check-fast` (so `just
check` and every push lane run it). **0.25 s, no network.**

* **Resolution, not a build.** `cargo metadata --locked --offline` — resolution
  is what broke, and checking it costs seconds where building costs minutes.
  `--offline` keeps the gate off the network: a correct lock needs no fetch, and
  an incorrect one fails on the LOCK rather than on connectivity, so a CI
  runner without network reports the real defect instead of a red herring.
* **The leaf set is DERIVED, not listed.** A leaf qualifies when it has a
  tracked `Cargo.lock` and its manifest carries a `path = …` dep resolving
  inside a path registered in `.gitmodules`. Today that is exactly one
  (`packages/cli/nros-launch-resolve`); a hardcoded list would go stale the
  first time another leaf grew a submodule dep, which is the drift class this
  repo keeps paying for.
* **An uninitialised submodule SKIPS with a message**, not a failure — the same
  self-gating `just setup-launch-resolve` does, since a checkout without the
  submodule cannot answer the question. Silence would be wrong too, hence the
  printed SKIP.

**Verified in three directions before being trusted**, against the real pre-fix
lock (`567101c43~1`) rather than a synthetic one:

| condition | result |
| --- | --- |
| fixed lock | `ok … resolves under --locked`, rc=0 |
| the actual pre-fix lock | rc=1, naming the leaf, the cargo error, and `just lock-update` |
| submodule directory absent | `SKIP … not initialised`, rc=0 |

**What this does NOT fix:** reason 2 of the filing — `setup-launch-resolve` is
still built only by `build-test-fixtures`. The gate makes the *lock* drift fail
fast, which is the failure that actually occurred; a compile regression in that
leaf would still wait for the fixture lane. Making `check-fast` build it was
rejected deliberately: it trades a sub-second resolution check for a compile,
and resolution is what broke.
