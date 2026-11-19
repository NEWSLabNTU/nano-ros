---
id: 2691
title: "FreeRTOS answers libc's wall-clock calls with ticks since the scheduler started"
status: open
type: bug
severity: medium
area: [platform, boards, freertos]
related: [issue-0758, rfc-0141, phase-575, phase-582]
found: 2026-11-18
---

## Problem

`nros_platform_time_now_ns` on FreeRTOS returns 0, which `platform.h` defines
as "no wall clock here". But two libc-level shims on the same platform answer
WALL-clock questions with the tick count since the scheduler started:

1. **`_gettimeofday`**, weak, in
   `packages/boards/nros-board-freertos/c/freertos_c_entry.c`. It is newlib's
   backend for `gettimeofday`, so a ported C++ program's
   `std::chrono::system_clock::now()` reads boot-relative time as if it were
   the time of day.
2. **`clock_gettime`**, in
   `packages/platform/nros-platform-freertos/src/cyclonedds_compat.c`. It
   ignores `clock_id`, so `CLOCK_REALTIME` and `CLOCK_MONOTONIC` give the same
   tick-based answer. Cyclone DDS reads `CLOCK_REALTIME` for a sample's source
   timestamp, so a FreeRTOS Cyclone image stamps its samples with boot time.

Zenoh-pico's FreeRTOS port reaches the first through `gettimeofday`
(`z_time_*`, `_z_get_time_since_epoch`, `z_timestamp_new`).

So one platform gives two answers to "what time is it": "none" through the
nano-ros seam, and boot-relative time dressed as wall time through libc. The
second is the stale-stamp class issue 0758 records: a peer that validates
stamps (ASI's `vehicle_cmd_gate`) rejects a boot-relative stamp as stale, or
worse, a consumer computes a huge `now - stamp`.

## Why it is not a one-line fix

The `_gettimeofday` shim exists for a reason (phase-482 W3). The toolchain's
libstdc++ has no monotonic clock (`_GLIBCXX_USE_CLOCK_MONOTONIC` is unset), so
`std::chrono::steady_clock` IS `system_clock`, which calls `gettimeofday`.
Before the shim, libnosys's `_gettimeofday` failed without writing the struct,
and every `steady_clock` reading was stack garbage. A liveness monitor's
`now - last_seen` needs a steady clock, and ticks are one.

So a fix that routes `gettimeofday` to a real wall clock would make
`steady_clock` jump whenever that wall clock steps (an SNTP step, a gPTP
lock). The two uses need separating, not one answer exchanged for the other.

Measured on no image yet: this is from reading the code on main `82b70b4fd5`.

## Direction

Fixed in phase-575 W6, where the one clock seam lands (ruled by the manager,
2026-11-18):

- `clock_gettime(CLOCK_MONOTONIC)` → `nros_platform_clock_ns`.
- `clock_gettime(CLOCK_REALTIME)` → the image's SELECTED wall-clock source
  (`[knobs.clock] source`), answering per the `epoch_failure` policy when it
  has no time: never ticks.
- `_gettimeofday`: decide, measured, between
  - routing it to the selected wall source and giving ported C++ a real
    steady clock some other way (e.g. building libstdc++'s `steady_clock`
    over `clock_gettime(CLOCK_MONOTONIC)` where the toolchain allows it), or
  - keeping ticks for `steady_clock`'s sake, but only when the image selects
    NO wall source, and saying so in the board guidance.
  Either way, no image that SELECTS a wall source may answer `gettimeofday`
  with ticks.

## Done when

- On the FreeRTOS QEMU board:
  - with `source = "platform"`, `clock_gettime(CLOCK_REALTIME)` and
    `nros_platform_time_now_ns` agree ("none");
  - with a stub board wall clock selected, a Cyclone sample's source
    timestamp, a zenoh timestamp and `system_clock::now()` all carry the
    board clock.
- `steady_clock` in a ported C++ program stays monotonic across a wall-clock
  step.
