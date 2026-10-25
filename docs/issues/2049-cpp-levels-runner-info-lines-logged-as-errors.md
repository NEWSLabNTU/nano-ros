---
id: 2049
title: "The C++ tier and level runners log their informational lines at ERROR, so the boot report counts every multi-level boot as an error"
status: open
type: bug
area: [api, logging]
severity: low
found: 2026-10-25
related: [1036, 2018, phase-162, phase-516]
---

## What

`nros-cpp`'s `cpp_diag!` is `nros_log::log_error!` (`packages/api/nros-cpp/src/lib.rs`,
the macro at the top of the crate). Most of its 43 call sites are real
failures. Three are not, and each fires on every boot that reaches it:

| Line printed | Where |
| --- | --- |
| `nros: multi-tier run — N tier(s) over one session` | the tier runner |
| `nros: multi-level run — N level(s) over one executor` | `nros_board_native_run_levels_in` |
| ``nros: level budget tier=`<name>` Bus/Pus enforced by the KERNEL\|EXECUTOR`` | `native_level_sched_policy` (issue 2018) |

## Measured

Every `levels-cpp` Linux image, 2026-10-25, prints for example:

```
[ERROR] nros: [847560.879394] nros: multi-level run — 2 level(s) over one executor
[ERROR] nros: [847692.132019] nros: level budget tier=`capped` 2000us/10000us enforced by the EXECUTOR
```

The boot record counts every `ERROR`/`FATAL` log record and keeps the first
one's source location (issue 1036). A healthy multi-level boot therefore reads
as one with errors, and the first error it records is a status line, which
hides a real failure behind it.

The Rust runners print the same lines with `println!` or a board console
write, not at ERROR, so this is the C++ arm alone.

## Fix

Log the three at INFO (`nros_log::log_info!`, or a `cpp_info!` beside
`cpp_diag!`), and keep `cpp_diag!` for failures. Check the e2e greps that read
these lines (`realtime_tiers_e2e`, `rt_sched_levels_e2e`): they match the
message text, not the level, so they are unaffected.

## Closes when

A `levels-cpp` boot's console shows the three lines at INFO, and its boot
record counts no error.
