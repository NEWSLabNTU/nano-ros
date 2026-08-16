---
id: 532
title: "The WALL clock still spreads one fact over three symbols — the monotonic half of this issue shipped as RFC-0073 / phase-352"
status: open
type: enhancement
area: embedded
related: [issue-0502, issue-0504, issue-0515, issue-0531, rfc-0073, phase-352, phase-354]
---

## Scope, restated 2026-08-16 (phase-354 W4)

**Most of this issue shipped.** W4 of phase-354 exists to check exactly that
before anyone plans work here, because phase-352 is titled "one nanosecond
symbol, plus the resolution nobody could ask for" — on its face this issue's
subject. Checked against the header rather than against the phase doc:

| this issue's proposed direction | status |
| --- | --- |
| 1. `nros_platform_clock_ns` as the one monotonic symbol | **DONE** (`platform.h:164`) |
| 2. `nros_platform_clock_resolution_ns` | **DONE** (`platform.h:178`) |
| 3. `clock_ms` / `clock_us` stop being per-port symbols | **DONE, and further** — phase-352 W6 retired them outright rather than keeping wrappers; `check-retired-platform-clock-symbols` gates their return |
| 4. keep a coarse path only if measurement justifies it | **DECIDED: deferred, not refused.** RFC-0073 names the trigger — "no caller has yet been shown to read the clock often enough to care. Add it when one is" |
| 5. wall clock collapses to one `time_now_ns` | **NOT COVERED — this is what remains** |
| open q: may `resolution_ns` change after init? | **ANSWERED** in the header: constant after platform init, and a port whose rate can change reports its COARSEST value. RFC-0073 keeps the liberalisation as an open question |

So this issue is not "already resolved and never closed", and it is also not
open as written. **What is left is item 5 alone**, and the rest of this document
is the monotonic argument that has already been acted on — kept because it is
the reasoning item 5 inherits.

## What remains: the wall clock

Unchanged since this was filed:

```c
uint64_t nros_platform_time_now_ms(void);         /* platform.h:286 */
uint32_t nros_platform_time_since_epoch_secs(void);
uint32_t nros_platform_time_since_epoch_nanos(void);
```

Three symbols for one fact, with the same defect the monotonic side had — and
the `secs`/`nanos` split additionally caps the seconds field at `uint32_t`,
which is a 2106 problem in a tree that just moved its monotonic clock to `u64`
ns for range. RFC-0073 mentions the wall clock only as EVIDENCE of the
inconsistency ("the wall clock advertises `time_since_epoch_nanos()` while
monotonic stops at µs"), never as scope, so nothing here has been designed.

The monotonic collapse is the worked example: one `u64` ns symbol, ports convert
from whatever they have, and the multi-symbol epoch-agreement question stops
existing. ~68 in-tree references across C and Rust, so a deprecation window is
needed as it was for `clock_ms`.

Not urgent, and deliberately not scheduled here — phase-354 W4's acceptance is
this restatement, not the work.

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

## Measured: the cost axis is DIVISION, not the unit (2026-08-13)

The objection to nanoseconds is "finer unit, more expensive on a 32-bit
MCU". Measured on the mps2-an385 lane, that is false — and the real
cost driver turns out to argue *for* nanoseconds.

Method: each candidate is an `#[inline(never)]` function reading
SysTick `LOAD`/`VAL` plus the FreeRTOS tick, called 1000 times under
`qemu -icount shift=0`, where one guest instruction is 1 ns of guest
time, so elapsed µs × 1000 is instructions retired. All four share the
same register-sampling prologue.

| candidate | per-read instructions |
|---|---|
| `us`, as the port does today (`cycles * 1_000 / (load+1)`) | **91** |
| `ns`, naive (`cycles * 1_000_000_000 / (load+1)`) | **94** |
| `ns`, exact multiplier (`cycles * 40`, 25 MHz ⇒ 40 ns/cycle) | **37** |
| raw counter + frequency, no conversion (the NuttX `perf_gettime` shape) | **34** |

Four conclusions, three of them surprising:

1. **The unit is not the cost axis.** ns costs the same as µs (94 vs
   91). The wider intermediate does not push the arithmetic onto a slow
   64-bit path; the runtime division dominates either way.
2. **Division is the cost axis** — 91 → 37 instructions, 2.5× cheaper,
   purely from replacing a divide with a multiply.
3. **Nanoseconds make that multiply possible MORE often than
   microseconds do.** ns-per-cycle is an integer for 25, 50, 100, 125,
   200 and 250 MHz (40, 20, 10, 8, 5, 4); µs-per-cycle is never an
   integer above 1 MHz, so a µs ABI is *forced* into a division on
   exactly the boards where an ns ABI is free. Normalizing to ns is
   cheaper than normalizing to µs.
4. **Raw counter + frequency is not worth its complexity.** It saves 3
   instructions over exact-ns (34 vs 37, ~8%) while pushing wrap
   handling, frequency plumbing and conversion onto every caller. The
   NuttX shape is the right answer for a profiling counter with
   genuinely unknown units; it is the wrong trade here.

Note that today's µs path is the most expensive option measured — ~91
instructions, ~3.6 µs at 25 MHz — and the executor reads it every spin.
So this is not only an accuracy change.

### Revised proposal

Unchanged: `clock_ns` as the single monotonic symbol, plus
`clock_resolution_ns`, with `clock_ms`/`clock_us` as header wrappers.
Rejected on evidence: the raw-counter-plus-frequency alternative.

Added, and this is the part the measurement earns:

- **The ABI should document the exact-multiplier path as the expected
  implementation**, not leave it to each port to rediscover. Where the
  counter frequency divides 1e9, a port converts with a compile-time
  `NS_PER_CYCLE` multiply; the ABI text should say so and the reference
  ports should demonstrate it.
- Where it does not divide evenly (12 MHz on `qemu_cortex_m3`,
  168 MHz on STM32F4), a fixed-point reciprocal — multiply by
  `2^32 / freq`, then shift — keeps it division-free at the cost of a
  wider multiply. Worth measuring before mandating.
- `clock_resolution_ns` then has a second job beyond diagnostics: it is
  the number a port must be honest about *because* the fast path
  depends on it. A port that returns 40 is claiming a 25 MHz counter.

### Still open

- Whether a separate cheap/coarse entry point survives. The gap between
  a bare tick read (~7 instructions, measured earlier) and exact-ns
  (~37) is real but small in absolute terms; the interesting question
  is whether any caller reads the clock often enough to care once the
  division is gone. The executor's per-spin read is the candidate.
- The same measurement on silicon. QEMU's `icount` counts instructions,
  not cycles, so it does not model memory-access latency for the
  register reads — the instruction ratios should hold, the absolute
  microseconds should not be quoted.
