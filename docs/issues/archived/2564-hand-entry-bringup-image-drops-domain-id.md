---
id: 2564
title: "A hand-written entry naming BRINGUP and IMAGE bakes domain 0 instead of the image's `domain_id`, with no refusal"
status: resolved
type: bug
area: [cmake, cli]
severity: medium
found: 2026-11-14
resolved: 2026-11-15
resolved_in: "the change that resolved it (one change for issues 2564 and 2582)"
related: [phase-478, 1946, 2433]
---

GitHub: NEWSLabNTU/nano-ros#3856

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)**,
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

## Resolution

The two issues are one gap: a hand-written CMake configure that NAMES its image
got none of that image's facts, because the one reader that delivers them
(`nano_ros_read_leaf_system`, over `nros dev image leaf-system`) spoke for the
configure only when the configure's top-level directory was a standalone leaf.
Issue 2564 is the domain half and issue 2582 the knob-row half; one change
fixed both.

- `nano_ros_read_leaf_system(DIR <d> IMAGE <id> CLAIM)` reads one image of a
  system.toml that is not the top-level project as the image this configure
  builds: its image env reaches the cargo carrier (`NROS_IMAGE_ENV_CONFIG`),
  and its domain, edition and network are the configure's, under the same
  agreement rules as a standalone leaf.
- `nano_ros_workspace(SYSTEM <bringup>)` claims the image the configure names
  (`-DNROS_IMAGE=<id>`) before its import, which is where the carrier
  attaches. A generated root (`NROS_CONFIGURED_IMAGE`) already sets the
  carrier and is left alone. A root that names no image while its bringup
  states per-image knob or env rows stops the configure, naming
  `-DNROS_IMAGE` and `nros build`.
- `nano_ros_add_executable(… BRINGUP <dir> IMAGE <id>)` with no `DOMAIN_ID`
  asks the reader for that image's domain (`nros_image_domain_id`), and holds
  it to the issue-1946 agreement rule against a `-DNROS_DOMAIN_ID`.

Test: `tests/cmake-hand-written-image-facts-tests.sh`
(`just check hand-written-image-facts`), with a stand-in CLI. It asserts the
domain query, a claimed read and an unclaimed one, the refusal of a
disagreeing -D, and the reach. Mutation-checked: removing CLAIM's effect turns
the claimed-read and refusal cases red.

The book's "Per-image build knobs" now says which roads apply the rows, and
how a hand-written root names its image.
