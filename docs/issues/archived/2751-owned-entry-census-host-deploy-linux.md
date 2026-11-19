---
id: 2751
title: "The census of an owned C/C++ entry that declares interfaces cannot configure: its host build asks board facts for image `linux`"
status: resolved
resolved: 2026-11-20
resolved_in: "the change that archives this issue"
type: bug
severity: medium
area: [cli, cmake, census, sizing]
related: [2433, 2750, 2752, 2754]
found: 2026-11-19
---

Part of the owned-entry run road theme (see 2750): an image whose
`[image.<id>] entry` names a hand-written C/C++ package. The other parts are
2750, 2752 and 2754.

## Measured

nano-ros 93368ca929, the downstream island workspace (branch
`nano-ros-next`). An owned C++ entry whose `package.xml` `<depend>`s on
message packages (`std_msgs`, `geometry_msgs`, ...), on boards
`freertos-posix` and `mps3-an536-freertos`. Every `nros build` / `nros run`
of such an image takes its census (issue 2433) and the host configure stops:

```
entity-census take unit_test_bringup:freertos-posix: taking the census of its owned entry `.../src/asi_tests/asi_unit_test`
CMake Error at .../cmake/NanoRosBoardFacts.cmake:227 (message):
  nano-ros: board facts for
  .../src/asi_tests/asi_unit_test
  --deploy linux could not be resolved (`nros dev image board-facts`):

  Error:
  .../src/asi_tests/asi_unit_test:
  --deploy linux names an image, but this dir has no system.toml (nor
  src/*/system.toml) to declare it and no Cargo.toml naming a board crate --
  resolve from the dir whose system.toml declares [image.linux]
Call Stack (most recent call first):
  .../cmake/NanoRosGenerateInterfaces.cmake:701 (nros_resolve_board_facts)
  .../cmake/NanoRosCodegenCore.cmake:1447 (nros_generate_interfaces)
  .../cmake/NanoRosVerbs.cmake:109 (nros_find_interfaces)
  .../cmake/NanoRosVerbs.cmake:187 (_nros_generate_declared_interfaces)
```

The default `[census] on_missing = "refuse"` stops the build. The island
states `on_missing = "warn"`, so its pools stay at the knob values; the
failed host configure still prints on every build.

## Cause

The census configures the owned package alone for the host
(`nros_record_owned_entity_facts`). The package's
`nano_ros_add_executable` takes `NROS_HOST_DEPLOY` (`linux`) and generates
its declared interfaces, which resolves board facts for `--deploy linux`
from the package's own directory. That directory has no `system.toml`, so
`board_facts.rs:814` refuses. The census test witnesses presumably declare no
interfaces, so they never reach board facts.

## Direction

A host census configure has no board: it should not ask for board facts, or
should ask with no deploy (the host board's facts), rather than naming an
image no file declares.

## Resolution (2026-11-20)

`nros_resolve_board_facts` falls back to `NROS_LEAF_BOARD` when no board is
named. That is the board the census host configure states
(`-DNROS_LEAF_BOARD=linux`). With `--board`, a dir with no `system.toml`
answers from that board's descriptor, so the host deploy is never asked
for as an image.

A named `NROS_BOARD` still wins.

Reproduced on a scratch copy of `examples/threadx-linux/c/talker`, made a
C++ owned entry depending on `geometry_msgs`. Before the fix it failed with
this issue's exact error; after it, the census and the build pass.

Tests (verdict rows in `cmake_node_register_misuse`):
- `a_census_host_configure_resolves_board_facts_from_its_stated_board`.
  Mutation: disabling the fallback reproduces the error.
- `a_named_board_outranks_the_census_leaf_board`. Mutation: putting
  `NROS_LEAF_BOARD` first turns it red.
