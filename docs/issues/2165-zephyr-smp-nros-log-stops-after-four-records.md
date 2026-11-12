---
id: 2165
title: "On a Zephyr SMP image only the first four `nros_log` records reach the console; every later record is lost while the application runs on"
status: open
type: bug
area: [zephyr, log]
severity: medium
found: 2026-10-30
related: [2000, 1037, 1123, phase-478]
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
— an island finding. The ASI Zephyr SMP image loses every `nros_log` record after the early ring.

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

## Not reproduced on qemu_cortex_a53 SMP (2026-11-12)

This test asked whether an SMP image on the main tree loses `nros_log` records
from more than one thread, with ASI's log configuration.

**Setup.** nano-ros 4eb8f9110d. The image is
`examples/workspaces/realtime-c`'s `smp_bringup:zephyr_smp`
(`qemu_cortex_a53/qemu_cortex_a53/smp`, Zephyr v3.7.0, `CONFIG_SMP=y`,
`CONFIG_MP_MAX_NUM_CPUS=2`), built with `nros build`. Two edits were made in a
scratch tree and not committed:

- ASI's log settings, `CONFIG_LOG_MODE_IMMEDIATE=y` and
  `CONFIG_LOG_ALWAYS_RUNTIME=y`, were added to the board `prj.conf`. The built
  `.config` also has `CONFIG_NROS_LOG_EARLY_RECORDS=4` and
  `CONFIG_LOG_BACKEND_UART=y`.
- An `RCLC_LOG_INFO` call was added beside each sched task's existing tick
  `printf`. `ctrl` is pinned to CPU 1 at 100 Hz. `telem` is unpinned, at 10 Hz.

**Run.** The image booted on `qemu-system-aarch64 -smp cpus=2 -nic
user,model=e1000` against `rmw_zenohd` on the host, for 30 s.

| sched task | tick `printf` lines | `nros_log` records on the console |
| --- | --- | --- |
| `ctrl` (CPU 1, 100 Hz) | 2892 | 2892 |
| `telem` (10 Hz) | 289 | 289 |

No `log record(s) lost` line appeared. Records from the two threads interleave
on the console (for example `telem tick=0` between `ctrl tick=7` and
`tick=8`).

So two threads on two CPUs, in immediate mode, with the default early ring, do
not reproduce the symptom on this board. Four CPUs could not be tried here:
with `CONFIG_MP_MAX_NUM_CPUS=4` and `-smp cpus=4` the image panics at boot
(`Can't find CPU Core 2 from dts`). The board's devicetree declares two cores.

**What ASI's image has that this run did not:**

- an AArch64 v8-R core with an MPU and no MMU (`fvp_baser_aemv8r_smp`);
- four CPUs;
- records raised from a board `nros_board_network_wait()` hook before DHCP;
- nano-ros 05b7f7d2648f, 9be6bcfa4f and 4fcc75d413, all older than the 2389
  work (2026-11-11) that bounded a stalled zenoh-pico write.

The FVP is not available on this host. The next measurement is on ASI's image
at current main: read `DISPATCH_GUARD`, `early::PENDING` and `early::LOST`, as
"What would decide it" says.

The repro patch (the scratch edits) is not committed. It is two
`RCLC_LOG_INFO` lines and the two Kconfig lines above.
