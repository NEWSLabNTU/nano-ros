---
id: 2846
title: "Zephyr's SNTP re-sync thread had a fixed 2 KiB stack, below a measured 7 KiB peak"
status: open
type: bug
severity: medium
area: [zephyr, clock]
related: [rfc-0141, phase-582, issue-0758, issue-2400]
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
3. With both numbers, record them: a `[[drift]]` row per board
   (`role = "SNTP_RESYNC"`) so the nightly drift check holds the walk, and the
   knob's help updated with the measured values.
