---
id: 2071
title: "On a port with no RTC (FreeRTOS, ThreadX), ROS time never advanced, so every rclcpp::create_timer(this, get_clock(), ...) timer never fired"
status: resolved
resolved: 2026-10-26
resolved_in: "phase-518"
type: bug
area: [core, clock, freertos, threadx, cpp, c]
severity: high
found: 2026-10-26
related: [1334, 0758, phase-518, phase-488]
---

## What

`nros_platform_time_now_ns` returns 0 on the FreeRTOS and ThreadX ports
(`packages/platform/nros-platform-{freertos,threadx}/src/platform.c`), because
neither board has a wall clock.

`nros_core::clock::Clock::wall_now` reads 0 as "no RTC" and falls back to the
in-image counter (`atomic_time::get_steady`). Nothing advances that counter on
an image with a port linked. `ClockType::RosTime` with no `/clock` source reads
`wall_now` (issue 1334), so a ROS-time timer's clock was the constant 0 and the
timer never fired.

The C surface had the same gap: `nros-c`'s `platform::get_system_time_ns`
passed the 0 through, so `RCL_ROS_TIME` / `RCL_SYSTEM_TIME` `now()` was 0 for
the life of the image.

`rclcpp::create_timer(this, get_clock(), period, cb)` is the stock Autoware
spelling. The in-tree examples use `create_wall_timer` (steady time), which is
why nothing caught this.

## How it showed

Phase 518 node 1 (`autoware_external_cmd_selector`) on QEMU mps2-an385
FreeRTOS joined the ROS graph and spun: `nros_cpp_spin_once` was hit more than
100 times. Its timer callback was never hit.

## Fix

When the port answers 0, both readers fall back to the port's MONOTONIC clock
(`nros_platform_clock_ns`). That is boot-relative time. Issue 0758 already
records that those images stamp with it, and it advances, which is what a timer
needs. A port that can state an epoch still wins.

- `packages/core/nros-core/src/clock.rs` — `Clock::wall_now`.
- `packages/api/nros-c/src/platform.rs` — `get_system_time_ns`.

After the fix, the same image's timer fired, and `ros2 topic echo` against ROS
2 Humble received `current_selector_mode`.

## Still open

Issue 0758: an epoch source (SNTP) for those ports, so the stamps are real
wall time and not boot-relative.
