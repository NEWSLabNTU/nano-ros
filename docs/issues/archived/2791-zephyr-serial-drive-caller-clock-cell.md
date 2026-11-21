---
id: 2791
title: "No booting cell proves a serial (ZPICO_DRIVE_CALLER) Zephyr image latches zenoh-hlc through the clock pump"
status: resolved
type: test-gap
severity: medium
area: [zephyr, zenoh, clock, testing]
related: [2078, rfc-0141, phase-582]
found: 2026-11-21
resolved: 2026-11-21
---

## What

phase-582 W1c-2 resolves the wall clock before the first spin. For a build
whose zenoh-pico executor runs only inside a spin (`ZPICO_DRIVE_CALLER`), it
shipped a pump: `zpico_clock_pump`, registered at session open, which ran
`zpico_spin_once(s, 0)` per open session from the `zenoh-hlc` source's wait in
`zephyr/nros_platform_zephyr_shims.c`. No cell booted such an image, so the
pump was compiled and never observed running. This issue asked for a booting
serial cell that latched the HLC through it, with a negative control.

## What was found: no accepted Zephyr image reaches the pump

**The serial premise was wrong.** `zpico.c` selects `ZPICO_DRIVE_CALLER` only
when `Z_FEATURE_MULTI_THREAD` is 0, or for ThreadX, `ZPICO_SMOLTCP` or
`ZPICO_SERIAL`. The Zephyr port defines only `ZENOH_GENERIC` and
`ZENOH_ZEPHYR`, never `ZPICO_SERIAL`, and `CONFIG_NROS_ZENOH_MULTI_THREAD`
defaults to `y`. So a serial Zephyr image keeps its read thread like an IP
one, and its pump was NULL.

**The single-thread build boots into a refusal.** That leaves
`CONFIG_NROS_ZENOH_MULTI_THREAD=n`. Such an image BUILDS (a native_sim Rust
listener links, carrying the pump), but on 3.7 (2026-11-21, host 241) both
cells written for it failed before any clock logic ran:

    the session states no readiness form: its backend neither calls a wake
    callback on an arrival nor reports a handle to park on, so an executor
    over it could only block inside its receive. Refused.
    RMW session open failed — Unsupported
    executor open failed: Transport(Unsupported)

followed by a kernel panic at 2.0 s. A caller-driven zenoh session states
`Notify` only when a board's receive interrupt wakes the park
(`set_caller_rx_wakes_park`) or the link's stack calls the wake callback on
a receive. No Zephyr board does either, and zsock has no arrival callback. So
the session states `None`, and the runtime refuses it at open (RFC-0113 D5).

## Resolution

The pump is unreachable on every Zephyr image the runtime accepts, so it is
deleted (`zpico.c`'s `zpico_clock_pump` and its registration, and the shim's
`nros_clock_set_pump`). The build-accepted, boot-refused gap closes with it:
`zephyr/cmake/nros_rmw_zenoh.cmake` now refuses
`CONFIG_NROS_ZENOH_MULTI_THREAD=n` at configure time, naming the reason,
instead of an image that panics at boot. RFC-0141 D1 is amended to match.

If a Zephyr single-thread zenoh build ever gets a readiness form (a receive
wake into the executor's park), the clock gate will need the link driven
during the `zenoh-hlc` wait again. That belongs with whichever change makes
the build boot, not here.
