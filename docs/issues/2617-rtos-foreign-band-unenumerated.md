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

## FreeRTOS: measured (2026-11-20)

The FreeRTOS image now prints a task census once every level is up
(`nros: task census name=`<name>` prio=<n>`, from
`nros_board_freertos_report_task_census` in `freertos_task_glue.c`, called at
the end of both runners' spawn chains). On the mps2-an385 realtime images,
Rust and C, it measured:

| Task | Priority | Created by |
| --- | ---: | --- |
| the levels (`high` / `nros_sched_task`, `app` / `nros_app`) | 3, 2 | nano-ros |
| `zpico_read`, `net_poll` / `poll` | 4 | nano-ros (transport) |
| `heappeak` / `stkpeak` | 1 | nano-ros |
| `tcpip_thread` | 4 | lwIP |
| `Tmr Svc` | **2**, now 4 | the kernel (`configTIMER_TASK_PRIORITY`) |
| `IDLE` | 0 | the kernel, below `range` |

The census found a live defect, not just a missing statement: the kernel's
timer service task sat at 2, inside `pool.app` [1, 3]. It shared a priority
with an application level, and a level at 3 could starve it; that task runs
the software timers, including a level's budget refill.
`configTIMER_TASK_PRIORITY` is 4 now. The FreeRTOS plan states
`reserved.foreign = [4, 4]` (the descriptor and the Rust literal in
`priority_plan.rs`). `check-sched-task-priority-plan` holds both
`configTIMER_TASK_PRIORITY` and `TCPIP_THREAD_PRIO` to that band.
`freertos_task_census_is_placed_by_the_priority_plan` reads the census and
fails on a task no band holds, or on a foreign task outside the foreign band.

Still open: the FreeRTOS POSIX simulator board's own `FreeRTOSConfig.h` sets
`configTIMER_TASK_PRIORITY` to `configMAX_PRIORITIES - 1`, and nothing has
measured it. NuttX and ThreadX have no enumerator yet.

## What closes it

An enumerator per RTOS port, since there is no procfs: at boot, list every task
or thread with its priority (FreeRTOS `uxTaskGetSystemState`, NuttX `nxsched`
task list, ThreadX `tx_thread_info_get` over the created list) and classify
each against the plan, as the POSIX probe does. Then each plan states
`reserved.foreign` as a measured band, or as `[]`.
