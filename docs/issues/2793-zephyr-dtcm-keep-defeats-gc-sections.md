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

## Fix (2026-11-21, in review; open until the link witnesses run)

Neither listed direction, but the second option's goal reached from the
linker side. Defining the pool only for an image that declares a sched task
needs a configure-time fact that does not reach the shim: on main
`CONFIG_NROS_ZEPHYR_MAX_SCHED_TASKS` is a plain Kconfig default (4, floor 1),
not derived, and the entry's `NROS_ENTRY_SCHED_TASK_COUNT` is in the
generated entry's own header, which the shim's TU never includes.

So the module owns the placement instead. `zephyr/nros_dtcm_stacks.ld`,
added through `zephyr_linker_sources(SECTIONS ...)` when either DTCM knob is
on, is one NOLOAD output section, `.nros_dtcm_stacks`, placed `> DTCM`, with
no KEEP. Every nano-ros DTCM stack is its own input section under it:

- `.nros_dtcm_stacks.nros_thread_stacks` and
  `.nros_dtcm_stacks.nros_sched_task_stacks` (the shim);
- `.nros_dtcm_stacks.__nros_sched_task_stack_<i>` (a generated C/C++ entry's
  level stack, `rclc/detail/sched_task_memory_zephyr.h`).

One input section per array matters: the linker drops whole input sections,
so one section holding both pools would be kept whole by the used one.
`> DTCM` also retires the orphan case issue 2084's BUILD_ASSERT guards: a
devicetree with no "DTCM" region now fails the link instead of placing an
orphan, and the BUILD_ASSERT still refuses it first, by name. The nucleo
board's `CONFIG_NROS_ZEPHYR_SCHED_TASK_STACKS_IN_DTCM=n` is gone.

Witnesses, in `tests/zephyr_link_layout.rs`:

- `sched_task_stack_pool_follows_stacks_in_dtcm` (the
  `build-cortex-m-c-sched-task-stacks-dtcm-zenoh` row): a sched-task image
  still HAS `nros_sched_task_stacks`, the level stack and `nros_thread_stacks`,
  each inside `.nros_dtcm_stacks`. Dropping the KEEP must not drop a USED pool.
- `an_image_without_sched_tasks_carries_no_pool_in_dtcm` (the
  `build-nucleo-f767zi-c-talker-serial` row): `nros_thread_stacks` in
  `.nros_dtcm_stacks`, and no `nros_sched_task_stacks` symbol at all. Reverting
  to `section("DTCM")` puts the pool back and fails it.

Not yet run: both rows on 3.7 and 4.4 (host 241).
