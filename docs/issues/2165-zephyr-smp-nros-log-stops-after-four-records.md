---
id: 2165
title: "On a Zephyr SMP image only the first four `nros_log` records reach the console; every later record is lost while the application runs on"
status: open
type: bug
area: [zephyr, log]
severity: medium
found: 2026-10-30
related: [2000, 1037, 1123]
---

## What was measured

Consumer: autoware-safety-island (ASI), branch `nano-ros-rebase`, Zephyr image
on `fvp_baser_aemv8r_smp` (`CONFIG_SMP=y`, `CONFIG_MP_MAX_NUM_CPUS=4`,
`CONFIG_LOG=y`, `CONFIG_LOG_MODE_IMMEDIATE=y`, `CONFIG_LOG_ALWAYS_RUNTIME=y`,
`CONFIG_NROS_LOG_EARLY_RECORDS=4`). Seen on nano-ros 05b7f7d2648f, main
9be6bcfa4f and main 4fcc75d413.

* The console shows exactly four records from the C++ `rclc_log_emit_fmt`
  path, then none for the rest of the run: the three banner lines and
  `Waiting for DHCP to get IP address...`, all raised on the main thread from
  the board's `nros_board_network_wait()` hook.
* A `printk` placed beside every `rclc_log_emit_fmt` call printed the whole
  boot, `Actuation Safety Island is Live` and the control loop. The
  application is alive; only the records are gone.
* No `N log record(s) lost` warning (`report_lost`) ever appears.
* Rust-side `nros_log` records on the same image are lost too (ASI's
  workaround below covers only its own C++ lines).
* The same sources on `freertos-posix` and the MPS3-AN536 FreeRTOS image log
  normally.

The ELF links ONE `nros_log` (one `nros_log::DISPATCH_GUARD`, one
`nros_log::early::SLOTS`, one `SINKS_PTR` in `nm zephyr.elf`), so a split
between two crate copies is ruled out.

## Reading (4fcc75d413; the lines are unchanged on main de7de3d0bc)

Four is the default of `NROS_LOG_EARLY_RECORDS`, which sizes the ring in
`packages/core/nros-log/src/early.rs` (`SLOTS`, early.rs:187). A record goes to
that ring in two places only:

* `packages/core/nros-log/src/lib.rs:1155-1167` - no sink installed yet;
* `lib.rs:1169-1178` - the dispatch guard (`DISPATCH_GUARD`, lib.rs:1309) is
  held by another delivery (issue 2000). The raiser retries the guard once and
  returns.

Once the ring is full, `early::hold` (early.rs:248-283) counts the record in
`LOST` and drops it.

On a C/C++ image there is a sink from the first record:
`rclc_log_emit_at` calls `ensure_default_sinks()` (`packages/api/nros-c/src/log.rs:170`),
which runs `nros_log::init(default_sinks())`. Held records are drained by
`drain_and_release` (lib.rs:1220) on the NEXT dispatch that takes the guard,
and that same path is the only one that runs `report_lost` (lib.rs:1248).

So "four delivered, then nothing, and no loss report" fits two mechanisms
only. Neither is confirmed yet:

1. **The dispatch guard is never released.** A delivery entered a sink and did
   not return. Later records go to the ring, fill it, and are counted lost,
   and the loss report never runs because it also needs the guard. The sink
   on Zephyr is `nros_platform_log_write`
   (`packages/platform/nros-platform-zephyr/src/platform.c:1474`), which calls
   `LOG_INF/LOG_WRN/LOG_ERR` in IMMEDIATE mode, and on SMP that runs under the
   log core's spinlock. The thread that raised record 5 is not the stuck one:
   its `printk` lines continue. A stuck delivery would have to be another
   thread's (a Rust `log::` record from a framework thread, for example),
   which fits the Rust records being lost too.
2. **The sink accepts the records and Zephyr's log core discards them.** Then
   `nros-log` works as designed, and the four-record match with
   `NROS_LOG_EARLY_RECORDS` is a coincidence.

## What would decide it

On the FVP after boot, read `nros_log::DISPATCH_GUARD`,
`nros_log::early::PENDING` and `nros_log::early::LOST` (all in `.bss`;
addresses from `nm -C zephyr.elf`). Mechanism 1 shows `DISPATCH_GUARD = 1`,
`PENDING = 4` and a `LOST` that grows with uptime. For mechanism 2, add a
`printk` at the top of `nros_platform_log_write` and check whether record 5
reaches it.

## Workaround in the consumer

ASI 20e0aae prints its own `log_*` lines with `printk` on Zephyr and keeps
`rclc_log_emit_fmt` on the other platforms. That covers only the
application's lines; framework records are still lost.

## What would close it

A Zephyr SMP image (the `fvp_baser_aemv8r_smp` board or a `qemu_cortex_a53`
SMP build) that logs continuously from more than one thread for the length
of a run. Every record reaches the console, or a loss count is reported.
The test should be a gate cell, so this cannot come back.
