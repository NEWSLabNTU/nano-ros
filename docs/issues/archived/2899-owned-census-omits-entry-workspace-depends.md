---
id: 2899
title: "The census of an owned entry builds the entry's package alone, without the workspace packages it <depend>s on, so an entry including a sibling package's header cannot be censused"
status: resolved
type: bug
severity: medium
area: [cli, cmake, census]
related: [phase-478, issue-2680, issue-2433, issue-2900]
found: 2026-11-23
resolved: 2026-11-25
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
I50. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`.

## What happens

The census of an owned entry (issue 2433, `take_owned_census` in
`packages/cli/nros-cli-core/src/cmd/entity_census.rs`) is a host build of the
entry's package alone (`census_host_program` over the package). The entry's
workspace `<depend>`s are not added to that build. Issue 2680 fixed the same
gap for the C/C++ metadata probe only; the owned census still has it.

## Repro (island)

The FreeRTOS test entries `asi_unit_test` and `asi_dds_loopback_test` include
`asi_common` headers and `<depend>` on `asi_common`. With their
`add_subdirectory(asi_common)` fallback removed, the configure fails in the
census build:

```text
unit_test.cpp:23: fatal error: common/clock/clock.hpp: No such file or directory
```

and the leaf road refuses at `NanoRosLeafEntityFacts.cmake:291` ("builds the
owned entry ..., which is sized by its census, and the census could not be
taken"). Island CI run 45143801025.

The island keeps the fallback until this is fixed.

## Expected

The owned census build adds the entry's workspace `<depend>` closure as the
probe does since issue 2680 (one helper for both, so they cannot drift).

## Acceptance

- A fixture owned entry that includes a header of a sibling workspace package
  it `<depend>`s on, with no `add_subdirectory` of its own, takes its census.
- The island deletes the `asi_common` fallback from its two FreeRTOS test
  entries and its FreeRTOS lanes stay green.

## Resolution

Fixed 2026-11-25. The owned census and the metadata probe now take the
entry's workspace dependencies from one helper, `owned_entry_closure` in
`entity_census.rs`, which is the probe's `probe_deps::workspace_deps`. The
host census build adds those packages from its project include: once, after
the entry's own `project()`, with `nano_ros` found first and dependencies in
order. A package added this way counts toward the program's sources, but the
census never runs an executable it builds. The census program walk in
`NanoRosLeafEntityFacts.cmake` skips them.

Fixture: `owned_two_programs_threadx` now has `two_prog_timing`, a header-only
sibling package. `prog_large` includes its header, and `two_prog`'s
CMakeLists does not add it. Built on host 241: the census took both programs
and the cross image built.

Left for the island (phase-478 T4): delete the `asi_common` fallback from the
two FreeRTOS test entries.
