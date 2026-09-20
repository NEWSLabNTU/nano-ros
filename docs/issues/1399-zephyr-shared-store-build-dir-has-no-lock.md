---
id: 1399
title: "Two checkouts building the same Zephyr leaf into the shared store
  workspace collide with no lock — west's sanity check catches the sequential
  case and not the concurrent one"
status: open
type: bug
area: [build, zephyr]
severity: medium
found: 2026-09-20
related: [issue-1379, issue-1258, issue-1016, phase-440, phase-449]
---

## What happens

Since phase-440 W4 the default Zephyr workspace is
`$NROS_STORE/workspaces/zephyr/<version>` — one directory shared by every
checkout on the host — and the build directory inside it is named from the leaf
alone (`build-<example>-<rmw>`, `just/zephyr-dev.just`). Two checkouts building
the same example therefore target the same cmake build directory.

SEQUENTIALLY that is loud, and measuring it is how issue 1379 ruled it out as
the cause of the mixed image it reported. West's `_sanity_check`
(`zephyr/scripts/west_commands/build.py`) compares the cached
`APPLICATION_SOURCE_DIR` with the one being built and, at the default
`pristine=never`, refuses:

> Build directory "…" is for application "…", but source directory "…" was
> specified; please clean it, use --pristine, or use --build-dir

CONCURRENTLY it is not. Nothing locks the directory, and the sanity check reads
a cache the other build may not have written yet, so two agent worktrees
starting `just zephyr build-one rust/listener zenoh` at the same time both
configure into one tree. Parallel agent sessions are the normal working shape
in this repo (CLAUDE.md prescribes linked worktrees), so this is not an exotic
arrangement.

A `-p auto` site is the quieter variant even sequentially: west pristines the
other checkout's build directory instead of refusing, which is correct for the
artifact and throws away work nobody asked it to.

## Why it is not simply "key the directory by checkout"

Issue 1379's own first suggestion. It was not taken there, for a reason that
still applies: a west build-dir NAME is a routing key. `check-west-leaf-vocabulary`
models the set, `require_west_leaf_in_lane` fails OPEN on a name it does not
know, and issue 1016 measured what that costs — a name no lane models produces
a STALE verdict indistinguishable from a cell that ran and failed. Renaming
trades a loud failure for a silent one unless the vocabulary, the lane
membership and the fixture resolvers move in the same change.

## Directions

* An advisory lock on the build directory (`flock` around the `west build`),
  so the second builder waits or says who holds it. Smallest, and it addresses
  the concurrency case exactly.
* Or make the store workspace a SOURCE location only and land builds under the
  invoking checkout — closer to CLAUDE.md's "BUILD WHERE YOU RUN", and the
  larger change, because it moves every path the fixture resolvers read.

## Acceptance

Two checkouts running the same `just zephyr build-one` concurrently either
serialise, or one refuses naming the holder. Neither silently configures into
the other's build directory.
