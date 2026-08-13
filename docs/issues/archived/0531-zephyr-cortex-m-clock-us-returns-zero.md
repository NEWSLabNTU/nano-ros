---
id: 531
title: "nros_platform_clock_us() returns 0 forever on Zephyr Cortex-M boards under 60 MHz, so the executor's timers never fire"
status: resolved
resolved_in: "fix landed 2026-08-13; verified on qemu_cortex_m3 2026-08-14"
type: bug
area: platform-zephyr
related: [issue-0502, issue-0529, issue-0532]
---

## Problem

The Zephyr port computes microseconds from the 64-bit cycle counter:

```c
/* packages/platform/nros-platform-zephyr/src/platform.c:49-51 */
uint64_t nros_platform_clock_us(void) {
    return (uint64_t) k_cyc_to_us_floor64(k_cycle_get_64());
}
```

`k_cycle_get_64()` is not unconditionally available:

```c
/* zephyr/include/zephyr/kernel.h:1838-1847 */
static inline uint64_t k_cycle_get_64(void) {
    if (!IS_ENABLED(CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER)) {
        __ASSERT(0, "64-bit cycle counter not enabled on this platform. ...");
        return 0;
    }
    return arch_k_cycle_get_64();
}
```

`__ASSERT` compiles out in a release build, so the failure mode is a
silent, permanent `0`.

On the Cortex-M SysTick driver, the symbol that selects it is
conditional on the CPU frequency:

```
# zephyr/drivers/timer/Kconfig.cortex_m_systick:30-34
config CORTEX_M_SYSTICK_64BIT_CYCLE_COUNTER
	depends on CORTEX_M_SYSTICK
	default y if (SYS_CLOCK_HW_CYCLES_PER_SEC > 60000000)
	select TIMER_HAS_64BIT_CYCLE_COUNTER
```

So any Zephyr Cortex-M board at or below 60 MHz gets a clock that
always reads zero unless something enables the symbol explicitly.
Nothing in this repo does: a git grep for
`CORTEX_M_SYSTICK_64BIT_CYCLE_COUNTER` / `TIMER_HAS_64BIT_CYCLE_COUNTER`
outside `zephyr-workspace/` returns nothing.

The tier-2 `ZephyrQemuCortexM` platform is exactly this case —
`qemu_cortex_m3_defconfig:4` sets `CONFIG_SYS_CLOCK_HW_CYCLES_PER_SEC=12000000`
(12 MHz).

## Why it matters

The executor defaults its timer accounting to this clock on
`no_std` + `rmw-cffi` builds (`nros-node/src/executor/spin.rs`,
`default_platform_clock_us`). With the clock stuck at 0 the per-spin
delta is `now - prev = 0` forever, so accumulated time never reaches
any period and **periodic callbacks never fire**. Subscriptions still
work, because they are data-driven — so the image looks alive while
every timer is dead, which is the same silent-failure shape as the
earlier spin-timeout accounting bug.

Boards NOT affected: `native_sim` / `native_posix`
(`Kconfig.native_posix:11` selects the symbol — which is why the
native_sim lane has always looked fine), ARM architected timer
(`Kconfig.arm_arch:11`, so the FVP board is fine), x86, ESP32,
nRF GRTC, cAVS, MTK ADSP. Cortex-M above 60 MHz picks up the default.

## Verification status

**Premise confirmed from the generated build config**, not just from
reading Kconfig defaults. Configuring `examples/zephyr/rust/talker` for
each board and reading `zephyr/include/generated/zephyr/autoconf.h`:

| board | `SYS_CLOCK_HW_CYCLES_PER_SEC` | `TIMER_HAS_64BIT_CYCLE_COUNTER` |
|---|---|---|
| `native_sim` | 1000000 | **defined 1** (why this lane always worked) |
| `qemu_cortex_m3` | 12000000 | **absent** (`CONFIG_CORTEX_M_SYSTICK 1`, no 64-bit variant) |

So on `qemu_cortex_m3` the port called `k_cycle_get_64()` in the exact
configuration where it returns 0.

### Confirmed on hardware-equivalent, 2026-08-14

`tests/zephyr-c-smoke` built for `qemu_cortex_m3` (12 MHz, no
`TIMER_HAS_64BIT_CYCLE_COUNTER`) and run under QEMU, as a same-binary A/B
with only `nros_platform_clock_ns` swapped:

| build | output |
|---|---|
| pre-fix (`k_cyc_to_ns_floor64(k_cycle_get_64())` unconditionally) | `clock_ms: 0 -> 0` → **FAIL: clock_ms did not advance** |
| fixed (tick fallback where the board has no 64-bit cycle counter) | `clock_ms: 0 -> 60` across a 50 ms sleep → pass |

That is the defect and the fix, end to end, on the board class the issue
names. Two things the run needed, neither related to the clock: the board
has no entropy device (`cmake/zephyr/qemu-cortex-m3.conf` enables Zephyr's
explicitly-not-random generator), and the smoke app's native_sim-sized
heap and stack overflow 64 KB of RAM.

An unrelated `FAIL: mutex_init` follows the clock line on this board and
is NOT investigated here — the clock assertions precede it and both
report cleanly.

**Superseded note** — the earlier claim that this was: both Zephyr lanes currently fail
to build before reaching this file, in `zpico-sys`'
`zenoh-pico/system/platform/zephyr.h:18` with `fatal error: version.h:
No such file or directory` — which is issue **0529** (the zpico platform
resolver never selects `zephyr`, so the Zephyr include paths are never
applied). A runtime confirmation, and a regression test asserting that
timers actually fire on a sub-60 MHz Cortex-M board, both wait on 0529.

## Fix (landed)

The port no longer depends on a Kconfig it does not control:

```c
uint64_t nros_platform_clock_us(void) {
    if (IS_ENABLED(CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER)) {
        return (uint64_t) k_cyc_to_us_floor64(k_cycle_get_64());
    }
    return (uint64_t) k_ticks_to_us_floor64(k_uptime_ticks());
}
```

The cycle counter is still preferred where the board provides one;
everywhere else the tick clock (always available, resolution
`CONFIG_SYS_CLOCK_TICKS_PER_SEC` = 1 kHz in this project's configs)
gives a clock that advances instead of one that reads zero.
`IS_ENABLED` rather than `#ifdef` so both arms keep compiling on every
board.

Deliberately NOT done: selecting `CORTEX_M_SYSTICK_64BIT_CYCLE_COUNTER`
from nano-ros' Zephyr Kconfig. That would force a 64-bit software cycle
count into every Cortex-M user's tick ISR to buy resolution most of
them are not asking for; a board that wants it can still enable it and
the first arm picks it up.

Remaining options if better resolution is wanted by default:
- Prefer a build-time assertion over a runtime zero: if the port keeps
  using `k_cycle_get_64`, a `BUILD_ASSERT(IS_ENABLED(
  CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER))` turns a silent dead-timer
  image into a compile error naming the missing symbol.
- `k_cycle_get_32()` is not a drop-in alternative: it wraps in ~25 s at
  168 MHz and ~171 s at 12-25 MHz, so it needs the same software
  wrap-extension the FreeRTOS and STM32F4 ports already carry.
- The general shape of this — a platform clock whose resolution and
  availability are per-board facts the ABI cannot express — is issue
  0532.


## Status 2026-08-13 — the FIX is in; the confirmation is BLOCKED

**Fixed by RFC-0073 / phase-352.** `nros-platform-zephyr/src/platform.c` no
longer calls `k_cycle_get_64()` unconditionally:

```c
uint64_t nros_platform_clock_ns(void) {
    if (IS_ENABLED(CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER)) {
        return (uint64_t) k_cyc_to_ns_floor64(k_cycle_get_64());
    }
    return (uint64_t) k_ticks_to_ns_floor64(k_uptime_ticks());
}
```

The cycle counter is used only where the board provides one; every other board
falls back to the tick clock. The comment there cites this issue by number. The
symbol itself is gone — RFC-0073 replaced `clock_{ms,us}` with `clock_ns`.

**The regression test now exists.** This issue asked for one ("a `qemu_cortex_m3`
run is both the confirmation and the regression test"), and the right witness is
the Rust cell of `mps2_an385` — a 12 MHz board, comfortably under the 60 MHz
threshold that decides `CORTEX_M_SYSTICK_64BIT_CYCLE_COUNTER`. Its talker
publishes from a 500 ms `nros_cpp_timer_create`, so `assert_talker` IS the clock
assertion: a dead clock produces no publishes.

`matrix::CELLS` had declared that cell `Runtime` since phase-346 W3 with nothing
running it; `zephyr_cortex_m_rust_zenoh_pubsub_e2e` now does.

**Confirmation is blocked by issue 0552, which is board-wide.** Every image on
this board — C, C++ AND Rust — branches to `PC = 0` shortly after net init when
a zenoh router is reachable (measured 2/2 with a router, 0/2 without). No image
survives long enough for a timer to matter, so the clock fix cannot be exercised
here yet. 0552's claim that "Rust on the same board passes" is refuted there with
the measurements.

So: keep this issue OPEN, but not as unfinished work — the fix is in and the test
is written. What remains is one green run of
`zephyr_cortex_m_rust_zenoh_pubsub_e2e`, available the moment 0552 is resolved.
