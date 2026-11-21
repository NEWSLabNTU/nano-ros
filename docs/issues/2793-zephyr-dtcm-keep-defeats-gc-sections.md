---
id: 2793
title: "In DTCM, an unused nano-ros stack pool is never dropped: Zephyr's memory-region output section is KEEP, so --gc-sections cannot discard it"
status: open
type: bug
severity: low
area: [zephyr, memory, linker]
related: [issue-2084, issue-2790, issue-0880]
found: 2026-11-21
---

## What happens

`CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM=y` places `nros_thread_stacks` in the
devicetree's `DTCM` region, and `CONFIG_NROS_ZEPHYR_SCHED_TASK_STACKS_IN_DTCM`,
which defaults to it, places the sched-task pool `nros_sched_task_stacks` there
too (`zephyr/nros_platform_zephyr_shims.c`, `__attribute__((section("DTCM")))`).

Zephyr generates each devicetree memory region's output section as

```
DTCM (NOLOAD) : { __DTCM_start = .; KEEP(*(DTCM)) KEEP(*(DTCM.*)) __DTCM_end = .; } > DTCM
```

so nothing placed in `DTCM` can be discarded. The sched-task pool is only
referenced by the sched-task spawn path. In sram0, an image that declares no
sched task has that pool dropped by `--gc-sections`: the shim's comment
measures it as dropped in 87 of 91 built images. In DTCM it is kept whole.

## Measured (2026-11-21)

`nros new` Rust talker, `nucleo-f767zi-zephyr`, Zephyr 3.7, with
`CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM=y` and the sched-task knob left at its
default (on):

| | sram0 | DTCM |
| --- | ---: | ---: |
| `nros_thread_stacks` | moved out (32,896 B) | 32,896 B |
| `nros_sched_task_stacks` | (dropped before) | 65,792 B, kept |
| DTCM used | | 98,688 of 131,072 B (75 %) |

The talker declares no sched task, so the 65,792 B are dead. They cost no
sram0, but they take half of the DTCM that `STACKS_IN_DTCM` exists to make
available.

## Workaround

The nucleo board package (issue 2790) sets
`CONFIG_NROS_ZEPHYR_SCHED_TASK_STACKS_IN_DTCM=n`, which keeps the pool in
sram0, where it is still dropped when unused. Any other board that turns on
`STACKS_IN_DTCM` carries the dead pool in DTCM, unless its images use sched
tasks.

## Fix direction

Place the pool so the linker can still drop it. Two options:

- Its own input section, for example `section("DTCM.nros_sched_task_stacks")`.
  This alone does not help, because the generated rule KEEPs `DTCM.*` too.
- Define the pool only when the image declares a sched task (the entry knows
  its levels at configure time). That is the same fact the derived
  `NROS_ZEPHYR_MAX_SCHED_TASKS` is computed from.

The shim's comment that `--gc-sections` drops an unused pool should be
qualified to sram0 either way.
