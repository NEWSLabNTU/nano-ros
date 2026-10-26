---
id: 2040
title: "mem-report files a level image's executor backing and task stacks under C++ and C, not as nano-ros storage"
status: resolved
type: tech-debt
area: [tooling, memory, executor]
severity: low
found: 2026-10-25
resolved: 2026-10-26
resolved_in: "8c82c15651 (scripts/nros-mem-report.py)"
related: [phase-509, rfc-0108, 2019, 1147]
---

## Problem

`scripts/nros-mem-report.py` credited only `__nros_comp_buf_<n>` and
`__nros_tier_executor_storage` to nano-ros storage roles. A level image's
other generated statics (`__nros_levels_backing`, `__nros_tier_stack_<i>`,
`__nros_tier_thread_<i>`, `__nros_scheduler_0`, the monitor tables) landed in
`C++ (global namespace)`, and the Zephyr task-stack pool `nros_thread_stacks`
in `(C / asm / no path)`.

## Fix

`STORAGE_ROLES` carries them, each row bound to its defining source in both
directions by `check_storage_roles`:

| role | symbols | priced against |
| --- | --- | --- |
| `[executor storage]` | `__nros_levels_backing`, `__nros_tier_backing_<i>` | `NROS_CPP_LEVELS_BACKING_SIZE`, `NROS_CPP_TIER_BACKING_SIZE_<i>` |
| `[level scheduler]` | `__nros_scheduler_<n>` | `NROS_CPP_SCHEDULER_BACKING_SIZE_<n>` when a sizes header states it |
| `[level task memory]` | `__nros_tier_stack_<i>`, `__nros_tier_thread_<i>`, `__nros_tier_tcb_<i>` | (`--levels` checks the stacks) |
| `[monitor storage]` | `__nros_{mon,age,hz,mode}_storage`, `__nros_mon_rows` | measured |
| `[platform task stacks]` | `nros_thread_stacks` | `CONFIG_NROS_ZEPHYR_TASK_SLOTS` x the task stack size, from the image's `.config` |

Role regexes are now grouped before anchoring (`^(?:rx)$`), so a row that is
an alternation cannot match a prefix. `selftest_level_image` covers the
island image's symbols, the near misses that must stay in their language
bucket, and a MISMATCH for each pricing.

`nros_thread_stacks` is not a row of `scripts/pool-inventory-knobs.txt`: that
table classifies the knobs `gen-pool-inventory.py` scans (Rust env knobs and
`knobs.toml` rows), and the two Kconfig symbols that size the pool are
neither, so a row there would fail as stale. mem-report names and prices
both knobs instead.

## Evidence

simple-autoware-safety-island board image (origin/main 1bfab8c, nano-ros
pin 4f545011ff plus this change), `build-board/zephyr/zephyr.elf`, 379,045 B
of RAM sections:

| | nano-ros roles + `nros_*` crates | C++ and C buckets |
| --- | --- | --- |
| before | 152,774 B | 226,248 B |
| after | 248,326 B | 130,696 B |

95,552 B moved. `__nros_levels_backing` prices at
`NROS_CPP_LEVELS_BACKING_SIZE = 40,640`; `nros_thread_stacks` at
5 x 4,096 B. `__nros_scheduler_0` (232 B) stays measured: its macro is in
the generated entry, not a sizes header. 36 B of `__nros_tier_<i>_groups`
and 12 B of `__nros_comp_<n>` handles remain in the C++ bucket.
