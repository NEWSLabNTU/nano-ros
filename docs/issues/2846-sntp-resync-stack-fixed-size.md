---
id: 2846
title: "Zephyr's SNTP re-sync thread had a fixed 2 KiB stack, below a measured 7 KiB peak"
status: open
type: bug
severity: medium
area: [zephyr, clock]
related: [rfc-0141, phase-582, phase-557, issue-0758, issue-2400, issue-2897, issue-2963]
found: 2026-11-23
---

## What

Phase-582 W3 added the Zephyr SNTP re-sync thread
(`zephyr/nros_platform_zephyr_shims.c`, `nros_zephyr_sntp_resync_loop`,
started when `clock.sntp_resync_interval_ms` is above 0 and `sntp` is a source)
with a FIXED stack: `#define NROS_SNTP_RESYNC_STACK_SIZE 2048`. The thread runs
`sntp_simple()`, i.e. Zephyr's resolver (`getaddrinfo`), a socket and the
exchange, so its depth is Zephyr's, not nano-ros's, and no one measured it.
Zephyr guards no thread stack on most of our boards, so an overflow writes over
whatever sits below the stack with no message.

## Evidence

A consumer (ASI) ran an equivalent re-sync thread of its own around
`sntp_simple()` on `fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp` (Zephyr 3.7,
Thread Analyzer): **7,024 B** high-water; a 4,096 B stack overflowed. The
2,048 B we shipped is below both. 32-bit targets are unmeasured.

The acceptance cells (`zephyr_talker_clock_sntp_{resync,once}`) run on
`native_sim`, where a Zephyr thread runs on a host pthread's stack, so they
could not have caught it, and the stack-floor walk skips native_sim.

## Fixed so far (2026-11-23)

- The stack is a knob, `clock.sntp_resync_stack_bytes`
  (`CONFIG_NROS_CLOCK_SNTP_RESYNC_STACK_BYTES`), default **8,192 B** = the
  7,024 B peak + the table's 10 % margin (7,727 B), rounded up to a power of
  two. Range 1,024 .. 1,048,576.
- The thread joins the build's stack-floor walk as role `SNTP_RESYNC`, rooted
  at `nros_zephyr_sntp_resync_loop` and checked against the knob
  (`cmake/NanoRosStackFloor.cmake`, `--role-knob` in
  `scripts/nros-stack-floor.py`). With no `[[observed]]` row the role is
  UNMEASURED, so a stack below the walk warns and never refuses.
- RFC-0141 D5 now says the interval is the image's monotonic (on a simulator,
  guest) time.

## Still open

1. **A static walk of the role on real targets.** Needs a Zephyr build (not
   native_sim) of an image with `clock.source` naming `sntp` and
   `clock.sntp_resync_interval_ms > 0`, on one 64-bit board
   (`fvp-aemv8r-smp` or `qemu-cortex-a53`) and one 32-bit board
   (`mps2-an385-zephyr`). The build prints
   `nros-stack-floor: SNTP_RESYNC stack ... B holds the floor ...` (or the
   warning) and writes `roles.SNTP_RESYNC` to
   `<build>/nros/stack-floor/report.json`.
2. **A run-time peak on a 32-bit target** (thread analyzer), so the default can
   be argued per word size, or kept at 8 KiB for both.
   **Measured (host 142, 2026-11-24):**
   - `mps2-an385-zephyr`, static walk: OPEN's floor is 95,936 B, which fits
     131,072. SNTP_RESYNC's is 96,952 B against 8,192, an upper bound reached
     through bound indirect calls (`dns_resolve_name` -> `publish_trampoline`),
     which the re-sync thread does not take.
   - `qemu-cortex-a53`: the walk still refuses on the arm64 assembly
     (issue 2897's step 2), so it has no floor yet.
   - `mps2-an385-zephyr`, thread analyzer over a 91 s boot: SNTP re-sync
     thread peak **808 of 8,192 B**; main 13,832 of 131,072; `zpico_exec`
     1,384 of 16,384.
   - That peak is the TIMEOUT path only. Slirp has no NTP server at
     10.0.2.2, so all three queries failed (rc=-116), and the one re-sync at
     60 s timed out too. A successful exchange (the consumer's 7,024 B on a
     64-bit board) is still unmeasured on 32-bit.
   - Next: the same boot with an SNTP responder on the host, so the peak covers
     a real answer.
3. With both numbers, record them: a `[[drift]]` row per board
   (`role = "SNTP_RESYNC"`) so the nightly drift check holds the walk, and the
   knob's help updated with the measured values.

## 32-bit measured, floor enforced (2026-11-25)

Items 1–3 are done for the 32-bit board:

- **Image.** `examples/workspaces/rust` `pubsub_bringup`'s
  `zephyr_cortex_m_talker_clock_sntp_resync` (`mps2-an385-zephyr`, fixture row
  `workspace-rust-zephyr-mps2-talker-clock-sntp-resync`, a BuildOnly matrix
  cell). Its server is a hostname (`ntp.nros.test:12399`), so the thread runs
  Zephyr's DNS resolver too, the deeper path.
- **Walk.** `SNTP_RESYNC` walks 90,728 B from `nros_zephyr_sntp_resync_loop`,
  an upper bound through `z_impl_zsock_sendto -> _zp_unicast_lease_task_fn`
  (bound by C type): a socket edge into zenoh's lease task that this thread
  never takes.
- **Peak.** 1,160 B in the thread analyzer under QEMU with icount, the same
  in two runs (888 B with a numeric server). That is far below the
  consumer's 7,024 B on 64-bit Arm. The exact command, responders and
  analyzer counts are in the row's `method`.
- **Guard.** An `[[observed]]` row (`config/stack-floors.toml`, role
  `SNTP_RESYNC`, peak 1,160, static 90,728). With it the image is measured:
  its floor is the peak plus 10 %, 1,280 B, held against
  `CONFIG_NROS_CLOCK_SNTP_RESYNC_STACK_BYTES`. Negative control:
  `sntp_resync_stack_bytes = 1024` refuses the build ("below the measured
  floor 1280 B"), and 8,192 B holds. The nightly drift check re-walks the
  row (`just zephyr stack-floor-drift --only
  pubsub_bringup@zephyr_cortex_m_talker_clock_sntp_resync`: OK, as recorded).
  A walk that ever reads below the peak is refused as a walker defect.
- **Knob help** now gives both numbers. The default stays 8,192 B: it is
  what the 64-bit figure needs, and 64-bit is not measured in-tree yet.

The row was first taken by hand. The twin lane now re-measures it
([issue 2963](archived/2963-stack-floor-twin-measures-only-main-thread.md)):
`just zephyr stack-floor-twin --only
pubsub_bringup@zephyr_cortex_m_talker_clock_sntp_resync` starts the router
and the SNTP and DNS stand-ins itself, and read 1,160 B in two runs.

## Still open: 64-bit

Zephyr 3.7 builds no Rust for `qemu_cortex_a53` (`RUST_SUPPORTED` excludes
`TIMER_READS_ITS_FREQUENCY_AT_RUNTIME`), so the 64-bit image is a C one. The
re-sync thread is the shim's C either way. A C talker with the same clock
knobs on `qemu-cortex-a53` builds, but its walk is REFUSED: aarch64 libgcc
has no row in `config/stack-builtins.toml` (the `__aarch64_*` atomics), and
the arm64 assembly entry points (`__start`, `arch_cpu_idle`,
`arm_smccc_hvc`, `arm_smccc_smc`, `z_arm64_context_switch`,
`z_arm64_exit_exc`, `z_arm64_fpu_save`) have no frames.
[Issue 2897](archived/2897-a53-stack-walk-refuses-aarch64-builtins.md) owns that
groundwork, in three steps: (1) the aarch64 libgcc row for the atomics (draft
#4760); (2) frames for the arm64 assembly entry points above; (3) the
observed `SNTP_RESYNC` row on `qemu-cortex-a53`, from a boot's stack-peak
line. Step 3 is that issue's, so this one does not add a second row.

**After issue 2897's steps 1-2 land:** the a53 walk can run. The C image is
`examples/workspaces/c` `pubsub_bringup` with `board = "qemu-cortex-a53"` and
the 32-bit image's clock and sntp knobs, plus board fragments under
`boards/qemu_cortex_a53_qemu_cortex_a53_smp/`; this issue's 32-bit row's
`method` is how its peak was taken. **When 2897's step 3 records the row:**
decide whether the 8,192 B default is still right for 64-bit, update the
knob's help with the in-tree 64-bit figure, and close this issue.

## Progress: 64-bit walk completes (2026-11-27)

Issue 2897 closed the frame gap on aarch64, so the qemu_cortex_a53 image's
re-sync thread is walked: `[image.zephyr_smp_sntp_resync]` in realtime-c's
`smp_sntp_bringup` reports SNTP_RESYNC from `nros_zephyr_sntp_resync_loop`
against `CONFIG_NROS_CLOCK_SNTP_RESYNC_STACK_BYTES=8192`, floor 173,592 B.
That floor is an UPPER BOUND through a machine-bound edge
(`dns_resolve_name -> publish_trampoline`), not a size: do not size the
64-bit stack from it. Still open here: the 64-bit `[[observed]]` row (a
thread-analyzer peak under QEMU, as the 32-bit row was taken), and the
looseness, issue 2998 (aarch64 C untyped) then RFC-0143.
