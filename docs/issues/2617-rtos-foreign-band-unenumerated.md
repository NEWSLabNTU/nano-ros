---
id: 2617
title: "The RTOS priority plans state no reserved.foreign band, so foreign threads such as lwIP's tcpip_thread sit nowhere a plan can check"
status: open
type: limitation
severity: medium
area: [platform, freertos, nuttx, threadx]
found: 2026-11-15
related: [0506, 0623, 0736, phase-555]
---

**Owner:** unassigned. **Home:** RFC-0079 "Open questions" (`reserved.foreign`
verification). Phase-459 is archived.

## What happens

RFC-0079 gives every port a priority address plan: `pool.app`,
`reserved.transport`, and `reserved.foreign` for threads nano-ros does not
create. Only Linux states `reserved.foreign`. Its `= []` is MEASURED, by
`scripts/dev/priority-thread-probe.py` reading `/proc/<pid>/task/*/stat` on a
live image (`packages/boards/linux/nros-board.toml`).

The RTOS plans have no `reserved.foreign` key at all:

| Port | Plan | Known foreign occupants |
| --- | --- | --- |
| FreeRTOS | `packages/platform/nros-platform-freertos/nros-platform.toml` | lwIP's `tcpip_thread` (started in `packages/boards/nros-board-freertos/c/network_glue.c`) |
| NuttX | `packages/boards/nros-board-nuttx-qemu/nros-board.toml` | the kernel work queues |
| ThreadX | `packages/boards/nros-board-threadx-linux/nros-board.toml` | NetX Duo's IP thread |

An absent band means nobody looked (the loader keeps absent and empty apart,
`empty_bands`), so on these ports a sched task can be allocated at or above
a foreign thread's priority and no check reports it.

## What closes it

An enumerator per RTOS port, since there is no procfs: at boot, list every task
or thread with its priority (FreeRTOS `uxTaskGetSystemState`, NuttX `nxsched`
task list, ThreadX `tx_thread_info_get` over the created list) and classify
each against the plan, as the POSIX probe does. Then each plan states
`reserved.foreign` as a measured band, or as `[]`.
