---
id: 2496
title: "The cmake road's FreeRTOS app stack has no system.toml spelling: [image.<id>.knobs.freertos] app_stack_kb reaches only cargo"
status: open
type: limitation
severity: low
area: [build, freertos, memory]
related: [issue-2400, phase-557]
found: 2026-11-13
---

## What

A FreeRTOS image's app-task stack is `NROS_FREERTOS_APP_STACK_KB`. How a project states it differs by road.

- **The cargo road.** `[image.<id>.knobs.freertos] app_stack_kb = <kb>` in `system.toml` works. (An `[image.<id>] env` row naming `NROS_FREERTOS_APP_STACK_KB` is refused since phase-484 W8 and points there.) The knob renders to an `NROS_FREERTOS_APP_STACK_KB` row in the generated `nros-cargo.toml` `[env]`. From there it reaches both the board build script (which sizes the heap) and the `option_env!` that sizes the task.
- **The cmake road (the C/C++ carrier).** The stack is a CMAKE variable that `_nros_freertos_entry_app_stack` (`cmake/NanoRosBoardNet.cmake`) reads at configure. The same knob row reaches only the cargo commands the configure spawns (`cmake/NanoRosImageEnv.cmake`, issue 1712). The C carrier never sees it. Measured 2026-11-13 on `examples/mps2-an385-freertos/c/talker`: `app_stack_kb = 20` left `.app_stack_bytes = 65536u`.

The only way to set it on this road is a configure flag: `nros build --cmake-args -DNROS_FREERTOS_APP_STACK_KB=<kb>`. That is a one-off command line, not part of the project. The cmake cache then keeps it, which `system.toml` does not show.

## Why it matters now

Issue 2400 W2b lets an image opt in to `stack_floor = "raise"`. Each build then derives the stack it needs and says what it used. A user who wants to pin that value cannot write it into the project on the cmake road. W2b therefore prints the `system.toml` line on the cargo road. On the cmake road it says that no such spelling exists yet.

## Fix

Give the carrier a `system.toml` spelling that reaches its configure. Two options:

Have the carrier read the image's `[image.<id>.knobs]` rows at configure time. `nano_ros_read_leaf_system` already reads `system.toml`, so it can hand the knob to `_nros_freertos_entry_app_stack` instead of only writing the cargo `[env]` file. The spelling then becomes one for both roads, as the registry rule asks.

Note that the cargo road's `Config::default()` reads only `option_env!`, not the ladder's `memory_value`.
