---
id: 2760
title: "Elapsed time is measured on the WALL clock, and two boards hand out uptime as wall time to keep it working"
status: resolved
resolved_in: "fix(platform): durations read the monotonic clock; no-RTC boards read 0 (issue 2760)"
type: bug
severity: high
area: [platform, rmw, zenoh, smoltcp, clock]
found: 2026-11-20
related: [phase-582]
---

**Home:** [phase-582](../../roadmap/phase-582-wall-clock-source.md) (found
while landing W1b; fixed before W1c). Owner: nros 139.

## Two defects, one cause

`nros_platform_time_now_ns()` is the WALL clock: UNIX time, or 0 when the
port has none (`nros/platform.h`). `nros_platform_clock_ns()` is the
MONOTONIC clock. Several readers measure elapsed time on the first:

| Site | What it times |
| --- | --- |
| `nros-smoltcp/src/bridge.rs` `poll` | smoltcp's `Instant` (retransmit, ARP, TCP timers) |
| `nros-smoltcp/src/bridge.rs` `clock_now_ms` | the bridge's own timeouts |
| `nros-smoltcp/src/deadline.rs` | the stack's next deadline |
| `nros-smoltcp/src/wait_idle.rs` | how long the idle wait sleeps |
| `zpico-sys/c/zpico/platform_aliases.c` `smoltcp_clock_now_ms` | the same, for C boards |
| `zpico-sys/c/zpico/platform_aliases.c` `z_time_elapsed_{us,ms,s}` | zenoh-pico's elapsed-time helpers |
| `nros-rmw-zenoh/src/shim/{publisher,subscriber}.rs` `now_ms` | deadline QoS, publish/match timing |

So:
- **On FreeRTOS and ThreadX** the wall clock is 0. Every one of these
  timers reads a clock that never moves.
- **On POSIX** the wall clock steps when the host's clock is set (NTP, a
  user), and a step moves every deadline with it.
- **On mps2-an385 and stm32f4** (the bare-metal smoltcp boards),
  `PlatformTime::time_now_ns` returns UPTIME, `clock_ms() * 1_000_000`. That
  keeps their smoltcp stack alive, and it hands out boot-relative time as if
  it were UNIX time. That is a stamp that looks real and is not, against the
  user's ruling that an unsynced wall clock reads 0 (RFC-0141 D4,
  "unstamped"). Their `nros_platform_wall_clock_state()` already says
  unsynced (phase-582 W1b), which disagrees with the non-zero clock.

## Fix direction

1. Move every elapsed-time reader in the table to
   `nros_platform_clock_ns()`. Readers that STAMP (a message's received
   time, a reply attachment, `_z_get_time_since_epoch`) stay on the wall
   clock.
2. Then make mps2-an385 and stm32f4 answer 0 from `time_now_ns`, as every
   port with no real-time clock does.
3. A gate, or a test, that keeps elapsed-time arithmetic off the wall clock.

## Done when

- A test reads `time_now_ns` on mps2-an385 and stm32f4 while unsynced and
  gets 0.
- The smoltcp bridge still advances its `Instant` when the wall clock is 0
  (a test with a frozen-at-zero wall clock and a moving monotonic one).
- No reader in the table derives a duration from the wall clock.

## Resolved

1. Every reader in the table except zenoh-pico's `z_time_elapsed_*` reads
   `nros_platform_clock_ns()` now. `z_time_*` is zenoh-pico's WALL-time API
   by its upstream contract (`gettimeofday`); its only duration use is the
   emscripten websocket link, which no image builds, and its timeouts use
   `z_clock_*`. The board's idle sleep compares `deadline_ns` to the
   monotonic clock, so the deadline smoltcp hands it now agrees with it.
2. mps2-an385 and stm32f4 answer 0 from `time_now_ns`. Tests:
   `just check bare-metal-wall-clock` (build tier; both crates sit outside the
   host workspace). stm32f4's advances its uptime counter first and fails on
   the old answer (5 000 000 000 against 0).
3. Gate `check wall-clock-readers` (fast): every file that reads the wall
   clock is named with why (a stamp, or a wall-time API), and a new reader
   fails until its reason is written. Run against main's five pre-fix files,
   it refuses each. It works per FILE, so a new duration in a file already
   named for a stamp is not caught; `platform_aliases.c` is the one such file.

The "frozen wall clock, moving monotonic" smoltcp test is replaced by the
gate: the bridge no longer reads the wall clock at all.

