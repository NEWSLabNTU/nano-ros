---
id: 2876
title: "An owned Zephyr west entry gets no workspace `NROS_INTERFACE_SEARCH_PATH`, so its own CMakeLists sets it by hand to find the workspace's message packages"
status: resolved
type: bug
severity: low
area: [cli, zephyr, cmake]
related: [phase-478, 2610, 2805, 2861]
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "fix(cli, issue 2876): an owned west entry gets the workspace interface search path"
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
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

## Resolved, 2026-11-23

`nros build` passes the search path on the owned entry's west line, beside
`ZEPHYR_EXTRA_MODULES` (the first of the two carriers "Wanted" named; the
module hook was not touched).

- **The value** is `configure::owned_interface_search_arg`: the workspace's
  `src/` (`ws::interface_search_root`), then the caller's exported
  `NROS_INTERFACE_SEARCH_PATH`, `;`-joined. That is the generated app's order
  (`cmake_root::interface_search_path_cmake`), and it is the ONE derivation the
  host census build of an owned program already used (issue 2805), which now
  calls it too. A workspace with no interface root gets no `-D`.
- **Where:** `cmd/build.rs`, the west arm, only for an image whose `entry =`
  is hand-written (`image.entry.is_some() && generated.dir.is_none()`). A
  generated app keeps writing its own line. The value is a cache entry, so a
  `set(NROS_INTERFACE_SEARCH_PATH ...)` left in an entry's CMakeLists still
  wins over it; the island's three lines keep working until it drops them.
- **Why the west line and not the module hook:** the line already carries the
  owned entry's other workspace facts (2861's modules, 2436's image
  statement), and the CLI holds the workspace root; the hook would have to
  re-derive it from the application directory.

Tests:

- `an_owned_west_entry_gets_the_interface_search_path_a_generated_app_gets`
  (`cmd/build.rs`): the generated app's `interface_src` and the owned line's
  first root are the same directory, followed by the exported value.
- The control, `a_directory_with_no_interface_root_gets_no_search_path_arg`.
- The 2861 west fixture `west_owned_entry_workspace_module` gains
  `demo_msgs`, a workspace message package the entry `<depend>`s on and
  `find_package`s, with no search-path line in its CMakeLists;
  `an_owned_west_entry_gets_the_workspace_interface_search_path`
  (`tests/west_hand_written_entry.rs`) asserts the cache holds the workspace's
  `src/` first.

The island's half (dropping the three `set(...)` lines, FVP lane green) is
phase-478 I36's, on the island's `nano-ros-next` once its pin passes this fix.
