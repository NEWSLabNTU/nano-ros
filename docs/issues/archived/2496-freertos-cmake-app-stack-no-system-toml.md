---
id: 2496
title: "The cmake road's FreeRTOS app stack has no system.toml spelling: [image.<id>.knobs.freertos] app_stack_kb reaches only cargo"
status: resolved
type: limitation
severity: low
area: [build, freertos, memory]
related: [issue-2400, phase-557]
found: 2026-11-13
resolved_in: "fix(issue 2496): a configure-time cmake reader of a knob sees the image's [image.<id>.knobs] row"
---

## What

A FreeRTOS image's app-task stack is `NROS_FREERTOS_APP_STACK_KB`. How a project states it differs by road.

- **The cargo road.** `[image.<id>.knobs.freertos] app_stack_kb = <kb>` in `system.toml` works. (An `[image.<id>] env` row naming `NROS_FREERTOS_APP_STACK_KB` is refused since phase-484 W8 and points there.) The knob renders to an `NROS_FREERTOS_APP_STACK_KB` row in the generated `nros-cargo.toml` `[env]`. From there it reaches both the board build script (which sizes the heap) and the `option_env!` that sizes the task.
- **The cmake road (the C/C++ carrier).** The stack is a CMAKE variable that `_nros_freertos_entry_app_stack` (`cmake/NanoRosBoardNet.cmake`) reads at configure. The same knob row reaches only the cargo commands the configure spawns (`cmake/NanoRosImageEnv.cmake`, issue 1712). The C carrier never sees it. Measured 2026-11-13 on `examples/mps2-an385-freertos/c/talker`: `app_stack_kb = 20` left `.app_stack_bytes = 65536u`.

The only way to set it on this road is a configure flag: `nros build --cmake-args=-DNROS_FREERTOS_APP_STACK_KB=<kb>`. That is a one-off command line, not part of the project. The cmake cache then keeps it, which `system.toml` does not show.

## Why it matters now

Issue 2400 W2b lets an image opt in to `stack_floor = "raise"`. Each build then derives the stack it needs and says what it used. A user who wants to pin that value cannot write it into the project on the cmake road. W2b therefore prints the `system.toml` line on the cargo road. On the cmake road it says that no such spelling exists yet.

## Fix

Give the carrier a `system.toml` spelling that reaches its configure. Two options:

Have the carrier read the image's `[image.<id>.knobs]` rows at configure time. `nano_ros_read_leaf_system` already reads `system.toml`, so it can hand the knob to `_nros_freertos_entry_app_stack` instead of only writing the cargo `[env]` file. The spelling then becomes one for both roads, as the registry rule asks.

Note that the cargo road's `Config::default()` reads only `option_env!`, not the ladder's `memory_value`.

## Resolution (2026-11-18)

The class, not just the row. A configure-time CMake reader of a knob
asks `nros_image_env_value`, which resolves cargo's own way: a `-D` or cache
value first (at the reader), then the environment, then the image's row. The
row comes from the cmake road's image-env file, or on the west road the row
the module hook records (`NROS_IMAGE_ENV_ROW_<name>`), so one function answers
on both roads.

- `_nros_freertos_entry_app_stack` (`cmake/NanoRosBoardNet.cmake`) reads
  `[image.<id>.knobs.freertos] app_stack_kb`, as the cargo road does. A bad
  value is refused, naming that spelling.
- The Cyclone descriptor cap
  (`packages/rmw/cyclonedds/.../NrosRmwCycloneddsDescriptorCap.cmake`) had
  the same gap (`-D`, then environment only) and asks the same function. It
  was invisible because `knob_roads` scanned only `cmake/` and `zephyr/`;
  the scan now includes `.cmake` files under `packages/`.
- `knob_road_coverage.tsv`: no measured row reads `no` any more. The `?`
  rows are readers not found by name; they are unchanged and out of scope.
- `stack_floor = "raise"` gives the `[image.<id>.knobs.<group>]` line on
  every road. Before, it said the cmake road had no spelling.

Not changed: `build_args::KNOB_REFUSAL_ON` (refusing a knob's `-D`) is
phase-568 W2c's switch, to be turned on per row once its roads are carried.

Test: `just check image-knob-readers` (`tests/cmake-image-knob-readers-tests.sh`,
script mode).
- A: the image's row reaches the app stack, 20 becomes 20480.
- B: a `-D` wins over it.
- C: the environment wins over it.
- D: no row keeps the 64 KiB default.
- E: the west road's recorded row reaches it.
- F: a bad row is refused, naming `app_stack_kb`.
- G: the Cyclone module reads the row.
With the old reader, A, C, E and F fail.
