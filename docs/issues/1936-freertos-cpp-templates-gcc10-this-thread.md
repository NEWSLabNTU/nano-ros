---
id: 1936
title: "FreeRTOS C++ copy-out templates fail mid-build on the distro arm-none-eabi-gcc 10.3 (`std::this_thread` not declared) instead of being refused at configure"
status: open
type: bug
area: [cmake, toolchain, freertos, cpp, templates]
severity: medium
found: 2026-10-17
related: [1117, 1113, 1453, 1240]
---

## Measured

`origin/main` `8f7e56feeb`, `scripts/check-template-copy-out.sh
cpp-port-minimal-publisher rclcpp-compat-smoke topic-state-monitor-port` with
`NROS_ARM_NONE_EABI_PREFIX=/usr/bin/arm-none-eabi` — Ubuntu 22.04's
`gcc-arm-none-eabi`, `arm-none-eabi-gcc (15:10.3-2021.07-4) 10.3.1 20210621`.
The configure prints the provenance line, the UNPINNED notice and the existing
floor WARNING (GCC 11, issue 1117), then continues:

| project (mps2-an385-freertos) | 10.3.1 | pinned 13.2.1 |
| --- | --- | --- |
| cpp-port-minimal-publisher | builds | builds |
| rclcpp-compat-smoke | `src/talker.cpp:88:14: error: 'std::this_thread' has not been declared` | builds |
| topic-state-monitor-port | `src/topic_state_monitor.cpp:115:14: error: 'std::this_thread' has not been declared` | builds |

Both failures are in the ported program's own spin loop,
`std::this_thread::sleep_for(std::chrono::milliseconds(100))`, after
`#include <thread>`. The error arrives at BUILD time, after cargo has compiled
the runtime, and names nothing about the compiler.

Reduced: a TU containing only `#include <thread>` + one
`std::this_thread::sleep_for` call fails the same way on 10.3.1 and compiles on
13.2.1 (`-mcpu=cortex-m3 -mthumb -std=c++17`).

## Which guard decides it

Not a nano-ros header. `nros-cpp` includes no `<thread>`; the sources that use
it are the templates', which are ROS 2 programs the rclcpp-compat surface
exists to compile unchanged.

- GCC 10.3.1 newlib `c++/10.3.1/thread`: the whole of `namespace this_thread`
  sits inside `#if defined(_GLIBCXX_HAS_GTHREADS)` (lines 40–567). A bare-metal
  newlib libstdc++ has no gthreads, so the namespace does not exist.
- GCC 13.2.1: `<thread>` pulls `bits/this_thread_sleep.h`, which declares
  `sleep_for` outside any gthreads guard (`#ifndef _GLIBCXX_NO_SLEEP`), falling
  back to `__sleep_for` when `_GLIBCXX_USE_NANOSLEEP` is absent — the `sleep`
  the FreeRTOS board provides.

So this is a toolchain floor, not a missing capability probe on our side: the
code is right, and below GCC 11 the library cannot provide it. The tree already
names GCC 11 as the cross floor (`NROS_CROSS_GCC_FLOOR_MAJOR`,
`cmake/toolchain/NanoRosCrossToolchain.cmake`), but only as a WARNING worded
for a different defect (issue 1117's designated-initializer frontend bug),
which it chose not to make fatal because a C-only image may build.
