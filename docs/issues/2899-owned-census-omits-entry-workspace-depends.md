---
id: 2899
title: "The census of an owned entry builds the entry's package alone, without the workspace packages it <depend>s on, so an entry including a sibling package's header cannot be censused"
status: open
type: bug
severity: medium
area: [cli, cmake, census]
related: [phase-478, issue-2680, issue-2433, issue-2900]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
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
