---
id: 532
title: "The platform clock ABI fixes a unit but cannot express resolution, so every port either lies or truncates"
status: open
type: enhancement
area: embedded
related: [issue-0502, issue-0504, issue-0515, issue-0531]
---

## Problem

The monotonic surface is two fixed-unit entry points with no way to ask
what they are worth:

```c
/* packages/platform/nros-platform-api/include/nros/platform.h:112,116 */
uint64_t nros_platform_clock_ms(void);
uint64_t nros_platform_clock_us(void);   /* "same epoch as clock_ms" */
```

Every port implements both independently, and the fixed unit is wrong
in both directions depending on the board:

- **Microseconds are a lie** where the source is a tick. ThreadX returns
  `tx_time_get() * MS_PER_TICK * 1000` at a 100 Hz default tick — 10 ms
  steps under a microsecond signature (`nros-platform-threadx/src/platform.c`).
  The MPS2-AN385 bare-metal port is `clock_ms() * 1000`
  (`nros-platform-mps2-an385/src/lib.rs:140`). FreeRTOS was the same
  until issue 0502 added SysTick interpolation, and only on Cortex-M.
- **Nanoseconds are discarded** where the source is finer. POSIX gets a
  `timespec` and divides ns away (`nros-platform-posix/src/platform.c:48`).
  On the mps2 board, SysTick resolves 40 ns at 25 MHz (measured:
  consecutive `SYST_CVR` reads step by 0-1 cycle) while the ABI reports
  1 µs. STM32F4 has DWT at 168 MHz — 6 ns — and its portable
  `MonotonicClock` is documented as "Resolution is 1 ms".
- **The same header is already inconsistent with itself**: the wall
  clock advertises `nros_platform_time_since_epoch_nanos()`
  (`platform.h:224`) while monotonic stops at µs — and that ns value is
  itself derived from a millisecond source on most ports.

Nothing in the ABI answers "how much of this number is real". Issues
0502 (ms-quantized µs), 0515 (a period the spin cadence cannot express)
and 0531 (a Zephyr clock that returns 0) are three symptoms of the same
missing fact.

## What the backends can actually do

Surveyed across the supported RTOSes (defaults as configured in-tree):

| RTOS | Best monotonic API | Native granularity | Finer counter reachable? | Clock ids | Resolution query |
|---|---|---|---|---|---|
| FreeRTOS | `xTaskGetTickCount` | tick (`configTICK_RATE_HZ`; 1 kHz here, 100 Hz on ESP-IDF) | only by reading SysTick directly (what 0502 does), port-specific | no | no |
| ThreadX | `tx_time_get` | tick, 100 Hz default = 10 ms | no public API (`TX_TRACE_TIME_SOURCE` is port-private DWT) | no | no |
| Zephyr | `k_uptime_ticks` / `k_cycle_get_32/64` | tick (1 kHz as configured); cycles for the cycle API | yes, but `k_cycle_get_64` needs `TIMER_HAS_64BIT_CYCLE_COUNTER` (issue 0531), and the cycle rate can be a RUNTIME value (`CONFIG_TIMER_READS_ITS_FREQUENCY_AT_RUNTIME`) | POSIX layer: MONOTONIC + REALTIME, both tick-resolution | POSIX `clock_getres` (tick constant) |
| NuttX | `clock_gettime` | tick (`CONFIG_USEC_PER_TICK`, 10 ms default) | yes: `perf_gettime` + `perf_getfreq`, explicitly "unknown units" | 5 ids + PTP | yes, but returns `NSEC_PER_TICK` for every id |
| ESP-IDF | `esp_timer_get_time` | **1 µs, free-running 64-bit hardware counter** | it is one | no | no |
| POSIX | `clock_gettime` | ns-capable | n/a | full set | yes |

Two facts fall out:

1. **A Linux-style clock-id interface cannot be implemented with
   fidelity on 4 of 6 backends.** FreeRTOS, ThreadX and ESP-IDF have no
   clock ids at all; NuttX has five but `clock_getres` returns the same
   tick-derived number for all of them. Copying `clockid_t` would mostly
   produce aliases of one clock — the multiplexing exists in Linux for
   namespaces, vDSO variants and per-process CPU time, none of which
   apply here.
2. **Resolution is a per-board, sometimes per-boot fact.** Zephyr's
   cycle rate may only be known at runtime; ESP-IDF stitches its
   timebase across sleep and can rescale on APB frequency change; a
   tick rate is a Kconfig. A compile-time constant cannot carry it.

## Proposed direction

**Nanoseconds as the single canonical unit, plus a resolution query,
and no clock ids.**

1. `uint64_t nros_platform_clock_ns(void)` becomes the one monotonic
   symbol a port implements. u64 ns is ~584 years of range, and ns is
   the only unit that does not truncate the hardware the tier-1/2 boards
   actually have (40 ns on mps2, 6 ns on STM32F4, 1 µs on ESP-IDF).
   Conversion from a tick source is one multiply, so tick-based ports
   lose nothing.
2. `uint64_t nros_platform_clock_resolution_ns(void)` — the missing
   fact. Ports return their real step (1e6 for a 1 kHz tick, 40 for
   25 MHz SysTick, 1000 for ESP-IDF). This is what lets the executor
   warn instead of guess: 0515's spin-cadence audit becomes exact, 0502
   would have been detectable in a test rather than by measuring
   cadence on target, and 0531 could be a startup check ("resolution
   claims 40 ns but the clock never advances").
3. `clock_ms` / `clock_us` become inline wrappers in the header, not
   per-port symbols. One implementation per port, no epoch-agreement
   question, and the 39-symbol ABI shrinks.
4. Keep the coarse path only if measurement justifies it. The argument
   for a separate cheap clock is real on tick RTOSes — an interpolated
   read is a retry loop plus register reads versus a single load — but
   the axis is cost, not unit. Measured on mps2/QEMU the interpolated
   read is ~156 ns amortised (1000 reads in 156 µs), which is not
   obviously worth a second symbol; the same measurement on silicon
   should decide it. If it is kept, name it for what it is
   (`clock_coarse_ns`), not for its unit.
5. Wall clock stays a separate concept (it already is) but should
   collapse the same way: one `time_now_ns` rather than
   `time_now_ms` + `time_since_epoch_secs` + `time_since_epoch_nanos`.

Migration: `clock_ms` has three in-tree consumers (two in the
CycloneDDS shim, one XRCE session-timeout loop) and `clock_us` has the
load-bearing ones; both keep working as header wrappers. Out-of-tree
ports implementing the old symbols need a deprecation window — accept a
port-provided `clock_us` if present, prefer `clock_ns`, drop a release
later.

Open question worth deciding explicitly: whether `clock_resolution_ns`
is allowed to change after boot (Zephyr runtime cycle rate, ESP-IDF APB
rescale). Simplest contract is "constant after platform init, ports
that cannot promise that must report their worst case".
