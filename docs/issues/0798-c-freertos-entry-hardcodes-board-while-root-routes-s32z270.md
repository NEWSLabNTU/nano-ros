---
id: 798
title: "`examples/workspaces/c`'s root routes `s32z270-freertos` to an entry
  that hardcodes `mps2-an385-freertos` — the pairing fails all three arms of
  `_nra_board_active`, so the image links without its platform glue"
status: open
type: bug
area: examples
related: [issue-0735, issue-0196, rfc-0065]
---

## Problem

`examples/workspaces/c/CMakeLists.txt` routes a second board to the FreeRTOS
entry leaf:

```cmake
# phase-372 W2 — the S32Z270 cross cell reuses the same embedded FreeRTOS entry.
elseif(NANO_ROS_BOARD STREQUAL "s32z270-freertos")
    list(APPEND _ws_subdirs src/freertos_entry)
```

…but that leaf still declares exactly one board:

```cmake
# examples/workspaces/c/src/freertos_entry/CMakeLists.txt
nano_ros_add_executable(freertos_entry
    BOARD   mps2-an385-freertos
    …
    DEPLOY  mps2-an385-freertos)
```

`nano_ros_entry` gates its embedded work on `_nra_board_active`, which is true
when any of three spellings appears in the entry's `DEPLOY` list — the board
name, the platform, or the normalised platform. With
`NANO_ROS_BOARD=s32z270-freertos` and `NANO_ROS_PLATFORM=freertos`, against
`DEPLOY = [mps2-an385-freertos]`:

| arm | value tested | in `DEPLOY`? |
| --- | --- | --- |
| board name | `s32z270-freertos` | no |
| platform | `freertos` | no |
| normalised platform | `freertos` | no |

So `_nra_board_active` is FALSE, and the entry silently skips the two blocks
gated on it:

* `nros_platform_link_app_deferred()` — no startup source, no `app_define`, no
  linker script, no kernel/netstack umbrella;
* the `NROS_ENTRY_LOCATOR` + domain bake — the image connects nowhere.

`BOARD mps2-an385-freertos` additionally makes the codegen emit the entry shape
for the wrong board.

This is verbatim the failure mode issue 0735 documented one arm over:
**"Configure succeeded, the build succeeded, the image was wrong."**

## Why it is latent rather than red

The only s32z270 fixture row targets the **C++** workspace, not the C one:

```toml
id = "workspace-cpp-s32z270-freertos"
dir = "examples/workspaces/cpp"
cmake_defs = { NANO_ROS_PLATFORM = "freertos", NANO_ROS_BOARD = "s32z270-freertos", … }
```

So nothing builds the C route, and nothing notices.

## Root cause — the fix landed at the reported site, not the class

phase-372 W2 fixed the **C++** entry to follow the active board:

```cmake
# examples/workspaces/cpp/src/freertos_entry/CMakeLists.txt
if(NOT DEFINED NANO_ROS_BOARD)
    set(NANO_ROS_BOARD mps2-an385-freertos)
endif()
nano_ros_add_executable(freertos_entry BOARD ${NANO_ROS_BOARD} … DEPLOY ${NANO_ROS_BOARD})
```

It edited the C workspace's **root** to route the new board, and left the C
workspace's **entry** hardcoded. One of the two halves of the pairing moved.

## Fix

Short term: give the C entry the same shape the C++ entry already has, and add a
`workspace-c-s32z270-freertos` fixture row so the route is exercised.

Structurally: this is the (entry × board) pairing being implicit.
[RFC-0065](../design/0065-colcon-like-workspace-builder.md) makes the pair
explicit and derived — an entry synthesised for a declared image cannot disagree
with the board it is built for, because it is generated from it. This issue is a
good acceptance case for that work.

## Sweep

Every `nano_ros_add_executable` / `nano_ros_entry` call whose `BOARD` or `DEPLOY`
is a literal, in a workspace whose root routes more than one board to that leaf:

```sh
grep -rn 'DEPLOY\|BOARD' examples/workspaces/*/src/*/CMakeLists.txt \
  | grep -v '\${' 
```

Cross-check each hit against its workspace root's `NANO_ROS_BOARD` branches.
