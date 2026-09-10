---
id: 1242
title: "`park_granularity_us()` is a hardcoded 1 ms — wrong high on ThreadX, wrong low on POSIX, and never asks the primitive"
status: resolved
type: bug
area: executor
resolved: 2026-09-10
resolved_in: "78569eb66 fix(#1242): the park granularity comes from the primitive, not a constant"
related: [issue-1193, issue-1194, phase-436]
---

## Problem

phase-436 W2 introduced `park_granularity_us()` so the executor could state
what park it can actually express, and W3 built the jitter-granularity report
on top of it. It is a constant:

```rust
pub fn park_granularity_us(&self) -> u64 {
    // 1 ms — the granularity of `nros_platform_wake_wait_ms`.
    1_000
}
```
(`packages/core/nros-node/src/executor/spin.rs`)

The justification given was that every `nros_platform_wake_*` port takes
`uint32_t timeout_ms`, so one millisecond is the floor everywhere. **That
reasoning is wrong in both directions, and the constant never consults the
primitive actually installed.**

## Wrong high — ThreadX

The ABI signature is not the only limit; the RTOS tick is a second, coarser
one. ThreadX converts to ticks:

```c
ULONG tps = TX_TIMER_TICKS_PER_SECOND;
ticks = (ULONG)(((uint64_t) timeout_ms * tps + 999u) / 1000u);
```
(`packages/platform/nros-platform-threadx/src/platform.c:729-746`)

and both shipped board configs pin `TX_TIMER_TICKS_PER_SECOND = 100`
(`nros-board-threadx-linux/config/tx_user.h:11`,
`nros-board-threadx-qemu-riscv64/config/tx_user.h:11`) — a **10 ms** tick.

So on ThreadX the executor reports a 1 ms park as achievable when the platform
can only deliver 10 ms, and `release_jitter_granularity_us()` inherits that
over-claim. This is exactly the failure W3 exists to prevent — *a measurement
claiming precision its mechanism cannot deliver* — reproduced one layer below
W3 by W3's own dependency.

FreeRTOS by contrast pins `configTICK_RATE_HZ = 1000`, so 1 ms is correct
there. The constant is right for one RTOS by coincidence.

## Wrong low — POSIX and NuttX

The POSIX primitive is already nanosecond-native: `pthread_cond_timedwait`
against a `struct timespec` on Apple, `sem_timedwait` against a `struct
timespec` elsewhere — and the `#else` branch is also the NuttX port, since
`nros-platform-nuttx/CMakeLists.txt` compiles
`../nros-platform-posix/src/platform.c` verbatim.

The millisecond floor there is **purely the exported signature**
(`uint32_t timeout_ms`), not the primitive. A microsecond park needs a unit
change, not a new mechanism.

## The mechanism defect underneath

`park_granularity_us()` does not look at `park_primitive` at all. So a port
that installs a genuinely microsecond-capable `ParkUntilFn` through the
phase-436 W6 seam **still cannot produce a sub-millisecond park**: the
requested bound is rounded up to 1 ms by `round_park_up_us` before the
primitive is ever called.

The seam was built to let a platform contribute its own waiting, and this
constant silently overrides what the platform can do — in both directions.

## Fix direction

1. Make the granularity a property of the installed primitive, not a constant:
   either a third field alongside `set_park_primitive(park, ctx)` or a probe
   the port supplies. With no primitive installed, fall back to the current
   1 ms — that is the honest floor for the `wake_wait_ms` ABI.
2. ThreadX must report its tick, not the ABI unit. `TX_TIMER_TICKS_PER_SECOND`
   is a compile-time constant, so the port can state it exactly.
3. Only then is issue 1193's µs path meaningful end to end: today µs is carried
   through the core (W2) and thrown away at the boundary.

Do **not** fix this by lowering the constant. A constant cannot be right for a
1 ms FreeRTOS tick, a 10 ms ThreadX tick and a nanosecond POSIX timespec at the
same time, which is the whole point.

## Evidence

Found by parallel port surveys during phase-436 W6, verified against the
sources cited above rather than inferred from the ABI.

## Resolution

Resolved by `78569eb66` (2026-09-10); verified against `main` on 2026-09-11.
Fix-direction items 1 and 2 hold; item 3 was never this issue's to close.

1. **The granularity is the primitive's.** `set_park_primitive(park, ctx,
   granularity_us)` stores what the port declares
   (`packages/core/nros-node/src/executor/spin.rs:2546-2554`), and
   `park_granularity_us()` returns it, falling back to 1 ms only when nothing
   is installed (`spin.rs:2685-2691`) — the honest floor of the `wake_wait_ms`
   ABI. No hardcoded `1_000` remains outside that fallback.
2. **Each port states its tick.** ThreadX derives it from
   `TX_TIMER_TICKS_PER_SECOND` — 10 ms on both shipped boards
   (`packages/platform/nros-platform-threadx/src/platform.c:811-815`); Zephyr
   from `CONFIG_SYS_CLOCK_TICKS_PER_SEC` (`nros-platform-zephyr/src/platform.c:1096`).
   The board entries hand it to the executor
   (`nros-board-threadx/src/entry.rs:115`, `nros-board-zephyr/src/entry_tiers.rs:111`,
   `nros-board-zephyr/c/zephyr_run_tiers.c:199`).
3. **POSIX / NuttX / FreeRTOS** install no `ParkUntilFn`, so they report the
   1 ms fallback, which is exactly what their `uint32_t timeout_ms` primitive
   delivers — no longer an over- or under-claim. Their sub-millisecond park
   needs the ABI unit change this issue named as a precondition, and that is
   issue 1193's `wake_wait_us` slot (still open), not a residue here. The cffi
   shim deliberately forwards neither optional symbol
   (`nros-platform-cffi/src/lib.rs:416-427`).

Tests from the fix commit, in `executor/tests.rs`: fallback with no primitive
(`:2085`), a coarser 10 ms port honoured (`:2099`), a finer 1 µs port honoured
(`:2118`), and the jitter granularity following the port rather than the
constant (`:2130`).
