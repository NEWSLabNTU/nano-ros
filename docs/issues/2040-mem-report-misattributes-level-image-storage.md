---
id: 2040
title: "mem-report files a level image's executor backing and task stacks under C++ and C, not as nano-ros storage"
status: open
type: tech-debt
area: [tooling, memory, executor]
severity: low
found: 2026-10-25
related: [phase-509, rfc-0108, 2019]
---

## Measured

`scripts/nros-mem-report.py` on the simple-autoware-safety-island board image
(Zephyr, S32K344, three levels on one executor; nano-ros 4f545011ff) attributes
RAM by owner. It credits nano-ros storage roles with 12,896 B:

| symbol | bytes | owner reported |
| --- | --- | --- |
| `__nros_comp_buf_0..2` | 10,104 | `[component storage]` |
| `__nros_tier_executor_storage` | 2,792 | `[executor storage]` |

The image holds far more nano-ros-owned RAM that the report files elsewhere:

| symbol | bytes | owner reported | what it is |
| --- | --- | --- | --- |
| `__nros_levels_backing` | 40,640 | C++ (global namespace) | the core's one executor backing (`NROS_CPP_LEVELS_BACKING_SIZE`) |
| `__nros_tier_stack_1`, `_2` | 32,768 | C++ (global namespace) | spawned levels' stacks (`NROS_TIER_TASK_MEMORY_DEFINE`) |
| `__nros_tier_thread_1`, `_2` | 544 | C++ (global namespace) | their `k_thread` objects |
| `__nros_scheduler_0` | 232 | C++ (global namespace) | the level scheduler (`NROS_CPP_SCHEDULER_BACKING_SIZE`) |
| `__nros_mon_storage`, `__nros_mode_storage`, `__nros_hz_storage`, `__nros_age_storage` | 888 | C++ (global namespace) | monitor tables |
| `nros_thread_stacks` | 20,480-40,960 | C / asm / no path | the platform task-stack pool (`NROS_ZEPHYR_TASK_SLOTS` x `NROS_ZEPHYR_TASK_STACK_SIZE`) |

So the report shows nano-ros at about 165 KB of a 419 KB image when nano-ros
owns about 280 KB of it. The level shape (phase-509) is where most of the gap
comes from: P9b moved the executor into `__nros_levels_backing` and the
per-tier storage symbol it still prices is now a 2,792 B handle.

`nros_thread_stacks` is also not a knob-sized pool in
`scripts/pool-inventory-knobs.txt`, although two Kconfig knobs size it.

## Closes when

- The generated-entry statics above report under storage roles (for example
  `[executor storage]`, `[level task memory]`, `[monitor storage]`), priced
  against their sizes-header macros where one exists.
- `nros_thread_stacks` is a knob-sized pool naming
  `NROS_ZEPHYR_TASK_SLOTS` and `NROS_ZEPHYR_TASK_STACK_SIZE`.
- A selftest row covers a level image's symbols.
