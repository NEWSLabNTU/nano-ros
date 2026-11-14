---
id: 2564
title: "A hand-written entry naming BRINGUP and IMAGE bakes domain 0 instead of the image's `domain_id`, with no refusal"
status: open
type: bug
area: [cmake, cli]
severity: medium
found: 2026-11-14
related: [phase-478, 1946, 2433]
---

GitHub: NEWSLabNTU/nano-ros#3856

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)**,
which holds what the island's port meets while building its image. Found on
the Autoware Safety Island port (`actuation_module/src/freertos_an536_entry`),
on main `37232ecff1`.

## What happens

A hand-written FreeRTOS entry package that names its bringup and image:

```cmake
nano_ros_add_executable(actuation_an536_entry
  BOARD   mps3-an536-freertos
  LANG    cpp
  BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../controller_bringup"
  LAUNCH  default
  TYPED
  IMAGE   an536
  DEPLOY  mps3-an536-freertos)
```

builds with its session on domain 0, although the bringup states

```toml
[image.an536]
board = "mps3-an536-freertos"
domain_id = 2
```

Both generated files bake 0: `<name>_nros_main_generated.cpp`
(`.domain_id = 0`) and `<name>_nros_app_config_def.c` (`.domain_id = 0`).
No warning and no refusal.

## Why

Issue 1946's fix carries the image's domain as a `DOMAIN_ID <n>` keyword
that `nros build` writes into the entries it GENERATES
(`builder/cmake_root.rs`); issue 2433 extended that to owned entries on the
generated root. A hand-written `nano_ros_add_executable(... BRINGUP ...
IMAGE <id>)` configured on its own gets no `DOMAIN_ID`, so
`NanoRosEntry.cmake` falls back to `-DNROS_DOMAIN_ID`, then to
`NROS_TOP_LEAF_DOMAIN_ID` (set only for an `add_subdirectory` leaf), and
bakes 0. `nros dev image facts --cmake <image>` does not print the domain
either, so the entry has nowhere to read it from.

The entry already names the bringup and the image, so the domain is known:
`leaf_system::read_image` (`[image.<id>] domain_id`, the image defaults,
then `[system] domain_id`).

## Direction

Either resolve the domain from BRINGUP + IMAGE when no `DOMAIN_ID` is given
(for example through `nros dev image facts`), or have the configure refuse an
image whose stated domain it cannot deliver, as issue 1946's "or the build
refuses" clause asks.

## Workaround

Pass `-DNROS_DOMAIN_ID=2` from the build script, or move the image to a
generated entry (`nros build an536`).
