---
id: 2876
title: "An owned Zephyr west entry gets no workspace `NROS_INTERFACE_SEARCH_PATH`, so its own CMakeLists sets it by hand to find the workspace's message packages"
status: open
type: bug
severity: low
area: [cli, zephyr, cmake]
related: [phase-478, 2610, 2805, 2861]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
I36. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`, nano-ros pin `8d4e70a382`.

## What happens

A generated Zephyr application puts the workspace's `src/` on
`NROS_INTERFACE_SEARCH_PATH` before `find_package(Zephyr)` (issue 2610,
`builder/west_app.rs` `render_cmakelists`, test
`the_workspace_interface_packages_are_on_the_search_path`), and a generated
CMake root does the same (`builder/cmake_root.rs`). An OWNED west entry (an
image whose `entry =` names a hand-written west application, RFC-0071 D19) is
the application itself, so nothing writes that line for it, and the west line
carries no `-DNROS_INTERFACE_SEARCH_PATH`. Issue 2861 gave the owned entry the
deploy and the workspace Zephyr modules a generated app gets; the search path
is the third thing a generated app has and an owned one does not.

Without it, an owned entry whose dependency includes a workspace message
package (`safety_island_msgs`, `visionpilot_msgs` through the island's
`asi_common` umbrella header) fails to find it at configure.

## Island workaround

Each Zephyr test entry states it by hand, after `find_package(nano_ros)`:

- `actuation_module/src/asi_tests/asi_unit_test_zephyr/CMakeLists.txt:39`
- `actuation_module/src/asi_tests/asi_dds_loopback_test_zephyr/CMakeLists.txt:39`
- `actuation_module/src/asi_tests/asi_can_output_test_zephyr/CMakeLists.txt:40`

```cmake
set(NROS_INTERFACE_SEARCH_PATH "${_ACT_ROOT}/src")
```

The FreeRTOS entries of the same programs (`asi_unit_test`,
`asi_dds_loopback_test`) need no such line: the cmake road's generated root
sets it.

## Wanted

An owned west entry configures with the workspace's `src/` on
`NROS_INTERFACE_SEARCH_PATH`, as a generated app does (an exported value kept
after it), with no line in the entry's CMakeLists. Either `nros build` passes
it on the west line beside `ZEPHYR_EXTRA_MODULES`, or the Zephyr module's
image hook sets it from the image's workspace before `find_package(nano_ros)`
runs.

## Acceptance

- The issue-2861 west fixture (`west_owned_entry_workspace_module`), with a
  workspace message package the entry's dependency includes, configures and
  builds with no `NROS_INTERFACE_SEARCH_PATH` in its CMakeLists.
- A test holds the owned and generated carriers to the same search path (the
  parity test 2861 added for modules).
- The island drops the three `set(NROS_INTERFACE_SEARCH_PATH ...)` lines and
  its FVP lane stays green.
