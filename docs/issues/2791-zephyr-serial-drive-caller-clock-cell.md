---
id: 2791
title: "No booting cell proves a serial (ZPICO_DRIVE_CALLER) Zephyr image latches zenoh-hlc through the clock pump"
status: open
type: test-gap
severity: medium
area: [zephyr, zenoh, clock, serial, testing]
related: [2078, rfc-0141, phase-582]
found: 2026-11-21
---

## What

phase-582 W1c-2 resolves the wall clock before the first spin. In a build
whose zenoh-pico executor runs only inside a spin (`ZPICO_DRIVE_CALLER`:
serial, and any single-thread build), nothing reads the link while boot waits,
so zpico hands the clock gate a pump (`zpico_clock_pump`, registered at
session open): `zpico_spin_once(s, 0)` per open session, driven by the
`zenoh-hlc` source's wait in `zephyr/nros_platform_zephyr_shims.c`.

No cell boots such an image. The only serial Zephyr row,
`zephyr-cortex-m-rust-talker-serial-zenoh` (issue 2078), is a LINK witness:
`tests/zephyr_link_layout.rs` reads its ELF and nothing boots it. So the pump
is compiled, and never observed running.

The configured cells that answer W1c-2 (`(ZephyrNativeSim, Rust, Zenoh,
Pubsub, Example) + [ClockHlc]`, `tests/zephyr_clock_resolve.rs`) are IP
native_sim images, which are `ZPICO_DRIVE_THREAD`: their read thread delivers
the sample, and the pump is NULL there.

## Why it was acceptable to ship without it

A pump that never runs leaves the `zenoh-hlc` source unanswered, so the
resolver falls back to the next declared source after the settle time (the
documented failure mode, the same one the fallback cell asserts). It does not
produce a wrong stamp.

## Acceptance

A booting Zephyr serial image (zenoh over a UART to a host router, e.g.
mps2/an385 under QEMU with the UART on a host pty or socket) with
`clock.source = "zenoh-hlc,platform"` and a host publisher: it latches
`zenoh-hlc` before its settle time (`nros: wall clock from zenoh-hlc`, the
resolution duration under `clock.fallback_settle_ms`), and then dispatches.
A negative control with the pump unregistered must fall back instead.
