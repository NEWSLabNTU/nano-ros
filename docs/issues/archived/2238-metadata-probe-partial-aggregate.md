---
id: 2238
title: "An incremental metadata probe rewrites the workspace aggregate with only the re-probed components"
status: resolved
type: bug
area: [build, configure, metadata]
found: 2026-11-04
related: [phase-530, rfc-0120, issue-1578, issue-1594]
---

## Symptom

On `examples/workspaces/cpp` (seven C/C++ components), editing ONE component
and re-running `nros configure` made the cross image's configure FATAL:

    nros: this image's entity inventory differs from the answer `nros
    configure` composed before this configure ... it registered
    [talker_pkg::talker::talker_pkg::Talker] and this configure registers
    [... all seven ...]

`build/demo_bringup__threadx/resolved-entity-inputs.cmake` named only
`talker_pkg` in `NROS_RESOLVED_ENTITY_REGISTERED`.

## Why

The workspace's `nros-metadata.json` (under
`build/nros-metadata/metadata-probe-cmake/build/`) has ONE writer: the probe
project's configure, whose `_nros_metadata_emit` (`NanoRosNodeRegister.cmake`)
writes the rows of every package the project `add_subdirectory`s. But
`metadata_refresh` passed `run_probes` only the STALE components, and
`run_probes` rendered the probe `CMakeLists.txt` over exactly those, so an
incremental probe's configure wrote an aggregate of the re-probed components
alone. `registered_population` (`cmd/build.rs`) composes an image over that
aggregate, so the resolve shrank to one component.

Present on `main` (origin/main `metadata_refresh.rs` pushes only stale
components into `cpp_batch` and `run_probes` renders over them). It was
reached there only by the Zephyr west road's launch-closure scope; phase-530
W5 made the cmake workspace road compose over the same aggregate (the root's
packages), and phase-530 W6's census step is what made a one-component edit
followed by a configure a routine event on that road.

## Fix (phase-530 W6, Ruling R46)

`run_probes(probe_dir, members, stale, rmw)`: the probe project ADDS every
member (`probe_project_members` -- the re-probed entry in place of its
member's), and builds and runs only the stale ones. `metadata_refresh` passes
every C/C++ component it would probe, fresh ones included, as the members:
membership is the caller's decision, and a component that left the workspace
is simply not a member. The aggregate keeps one writer (the probe configure)
and no merge step exists anywhere else. Unit tests
`a_re_probe_of_one_component_keeps_every_member_in_the_aggregate`,
`a_member_that_left_drops_out` (`metadata_probe_cmake.rs`).
