---
id: 2237
title: "A cross workspace image's census is checked during a build but never re-taken there"
status: resolved
type: limitation
area: [build, configure, census]
found: 2026-11-04
related: [phase-530, rfc-0120, issue-1419]
---

## Symptom

On the cmake WORKSPACE road, a source edit followed by a plain
`cmake --build` of a cross image fails the build:

    census stale: ...
        nros dev image entity-census take --image <bringup>:<image>

The user runs that command and builds again. The leaf roads re-take their
census inside the build (phase-530 W6's census step) and re-configure only
when the entities moved; this road asks for one manual command after every
source edit of a counted component, body edits included, because freshness
is per source and only a re-take can tell whether the entities moved.

## Why (phase-530 W6, Ruling R39)

A workspace image's census is taken from its HOST sibling: `take --image`
plans that image (`build::plan_builds`, stages 1-4) and builds it. Running
that from a cross image's build would be a whole plan in the middle of a
build, writing outputs the cross configure reads -- not the cached host
compile RFC-0120 D7 prices. So W6 gave this road a READ-ONLY edge
(`nros_census_check_step`, `cmake/NanoRosCensusStep.cmake`): the configure's
`ws entity-census check --require-fresh`, before the entry links. It never
under-sizes silently; it does not re-take.

## Fix (phase-530 W6, Ruling R42, which superseded R39)

The workspace road now re-takes its census during a build, the way the leaf
road does, and the read-only check step is deleted (one mechanism).

- A workspace `take --image` RECORDS the host sibling's build step beside the
  census (`<stem>.census.host-build.json`: the handoff's program, args, cwd
  and env, plus `NROS_LEAF_CENSUS_HOST`), after it built it.
- `take --model <m> --workspace <ws> --build-step` is the census step's
  command on this road: a fresh census writes its digest and returns; a stale
  one reruns the recorded build step (an incremental `cmake --build` /
  `cargo build` of a dir that exists -- a host tool, never a re-plan, never a
  target tool) and the census run, which writes the digest write-if-changed.
  With no record (a census an older CLI took) it refuses naming `take --image`.
- `NanoRosEntry.cmake` calls `nros_census_step(MODEL ... WORKSPACE ...)`
  instead of depending on the census FILE: the digest beside the model is the
  configure dependency, so a body edit re-runs no configure and an added
  endpoint exactly one. Under a non-Ninja generator the step fails the build
  that moved the digest (Ruling R40), as on the leaf road.

Tests: `tests/cmake-census-step-tests.sh` cases C (Ninja: body edit green with
0 re-runs, added entity 1 re-run read) and C2 (Makefiles), plus unit tests for
the record, a fresh model census in a build, and the no-record refusal.
