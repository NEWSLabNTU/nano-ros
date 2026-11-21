---
id: 2805
title: "The host census build of an owned entry does not generate the workspace's own interface packages"
status: resolved
type: bug
area: [cli, census, codegen]
severity: medium
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "the host census build puts the workspace's src/ first on NROS_INTERFACE_SEARCH_PATH"
related: [2433, 1556]
---

## What fails

An image whose program is an OWNED C/C++ entry on a cross board is sized by
the census of that entry (issue 2433): `nros dev image entity-census take`
configures the entry's package ALONE for the host, builds it and runs it.
That host build generates interfaces only for packages the ament install
provides. A `<depend>` on one of the WORKSPACE's own message packages
generates nothing, and the entry's first include of it fails.

Seen on a consumer workspace (an `actuation_module/` with
`src/safety_island_msgs`, `src/visionpilot_msgs` and the owned entry
`src/asi_tests/asi_unit_test`, whose `package.xml` `<depend>`s on both):

    nros build unit_test_bringup:freertos-posix

The host census build under
`build/nros/census-host/unit_test_bringup/freertos-posix` generates
`std_msgs`, `geometry_msgs`, `autoware_*` and stops at

    messages.hpp:36:10: fatal error: safety_island_msgs/msg/approved_request.hpp: No such file or directory

and the take reports

    warning: entity-census take unit_test_bringup:freertos-posix: the census of
    the owned entry `asi_unit_test` could not be taken (... host census build
    exited exit status: 1.)

because the bringup opts into `[census] on_missing = "warn"`; with the
default (`"refuse"`) the build refuses. `asi_dds_loopback_test` fails the
same way.

## Why

`census_host_program` (`packages/cli/nros-cli-core/src/cmd/entity_census.rs`)
configures the package with `nano_ros_DIR`, `NROS_REPO_DIR`,
`NROS_LEAF_BOARD=linux`, the codegen tool, the Zephyr stub and
`CMAKE_PROJECT_INCLUDE`. Nothing puts the workspace's `src/` on
`NROS_INTERFACE_SEARCH_PATH`. The cross build does: it configures the
workspace's generated root, which sets the variable from
`cmake_root::interface_search_path_cmake` before `find_package(nano_ros)`.
So the census build and the cross build resolved a `<depend>` differently.

## The RMW of the census build

The census build compiles with `nros-rmw-zenoh` while the consumer's system
uses Cyclone DDS. That is by design: in census mode the funnel selects the
recording backend (`census_select_backend` in nros-cpp), so the counts a
census records do not depend on the RMW. A program whose own code branches
on the RMW would be counted on the zenoh arm; the consumer's
`actuation_module/src` has no such branch (`NROS_RMW`, `RMW_ZENOH`,
`RMW_CYCLONE`: no match).

## Acceptance

- The host census build of a program inside a workspace puts that
  workspace's `src/` first on `NROS_INTERFACE_SEARCH_PATH`, with the caller's
  exported value after it, the generated root's layering.
- A gate takes the census of an owned entry on a cross board whose publisher
  carries a workspace-local message, and fails on the pre-fix tree.

## Resolution

`census_host_program` now adds `-DNROS_INTERFACE_SEARCH_PATH=<ws>/src[;<caller's>]`
to the host configure, through `census_interface_search_arg`. The workspace is
the one `image_facts::workspace_root_from` finds (an ancestor whose `src/`
holds a bringup), so a standalone leaf outside any workspace gets nothing new;
its `src/` is `ws::interface_search_root`'s, the root the cross configure uses.
The caller's exported value follows the workspace's, as in the generated root.

Tests:

- `just check owned-census-local-msgs` (build tier,
  `scripts/check-owned-census-local-msgs.sh`) takes the census of
  `packages/testing/nros-tests/bins/owned-census-local-msgs`: an owned C entry
  on threadx-linux whose publisher carries the workspace-local
  `owned_census_msgs/msg/Tally`, declared only in its `package.xml`, in a
  bringup that refuses a missing census. On the pre-fix CLI the take refuses
  with `fatal error: owned_census_msgs.h: No such file or directory`; with the
  fix the census records the `/tally` publisher of that type.
- Unit tests `an_owned_census_build_searches_the_workspaces_own_interfaces`
  and `a_standalone_leaf_census_build_adds_no_workspace_search_path`
  (`entity_census.rs`).

Consumer: on the safety island's `actuation_module`, with this CLI,
`entity-census take` of `unit_test_bringup:freertos-posix` records
`asi_unit_test` (1 node, 1 sub, 1 pub, 1 timer, 2 parameters) and
`dds_loopback_test_bringup:freertos-posix` records `asi_dds_loopback_test`
(1 node, 1 sub, 1 pub); neither warns.
