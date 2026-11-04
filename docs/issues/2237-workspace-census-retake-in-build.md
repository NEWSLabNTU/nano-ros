---
id: 2237
title: "A cross workspace image's census is checked during a build but never re-taken there"
status: open
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

## Fix direction

A re-take that does not re-plan: the host sibling's build dir already exists
after the first take, so the edge could `cmake --build` it and run the census
(the leaf road's shape), and write the census digest beside the model, which
the cross configure would then depend on instead of the census file
(`NanoRosEntry.cmake`). `nros build` already re-takes through configure, so
the gap is the plain host-build road only.

Home: phase-530 W6.
