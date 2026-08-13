# Phase 352 — Platform clock: one nanosecond symbol, plus the resolution nobody could ask for

**Status (2026-08-13). W1–W5 LANDED.**

**Implements:** [RFC-0073](../design/0073-platform-clock-nanoseconds-and-resolution.md).

**The principle.** The ABI stops asserting a precision it cannot keep and starts
stating the precision it has. `nros_platform_clock_ms` + `nros_platform_clock_us`
(two fixed units, no way to ask what either is worth) become
`nros_platform_clock_ns` + `nros_platform_clock_resolution_ns`, with the unit
conversions demoted to `static inline` header wrappers that no port implements.

Why it is not a widening for its own sake: microseconds are a *lie* on
tick-sourced ports (ThreadX returns 10 ms steps under a µs signature) and a
*truncation* on fine ones (mps2 SysTick resolves 40 ns, STM32F4 DWT 6 ns).
Issues [#0502](../issues/archived/0502-freertos-threadx-clock-us-ms-quantized.md),
[#0515](../issues/archived/0515-period-not-multiple-of-spin-quantizes-silently.md)
and [#0531](../issues/0531-zephyr-cortex-m-clock-us-returns-zero.md) are three
symptoms of the same missing fact.

Measured, so the "ns costs more on a 32-bit MCU" objection does not have to be
argued: per read on mps2 under `-icount shift=0`, today's µs path is **91**
instructions, naive ns **94**, ns with an exact per-cycle multiplier **37**. The
unit is free; the runtime *division* is the cost; and ns-per-cycle is an integer
at 25/50/100/125/200/250 MHz where µs-per-cycle never is. The migration makes
the clock cheaper, not dearer.

---

## W1 — The ABI surface — **LANDED**

- [x] `<nros/platform.h>`: `clock_ns` + `clock_resolution_ns` replace the two
      unit symbols; `clock_us`/`clock_ms` become `static inline` wrappers behind
      `NROS_PLATFORM_LEGACY_CLOCK_UNITS`.
- [x] Rust mirror `PlatformClock`: two required methods, `clock_us`/`clock_ms`
      provided.
- [x] `nros_platform_export_clock!` emits only the two ABI symbols.
- [x] `generated.rs` regenerated; `check-platform-abi-mirror.sh` passes (it
      already skips `static inline` by design, so the wrappers carry no ABI
      obligation).

## W2 — The six C ports — **LANDED**

- [x] **posix** — `clock_gettime(CLOCK_MONOTONIC)` straight to ns, no division;
      resolution from `clock_getres`, floored at 1.
- [x] **freertos** — the #0502 SysTick interpolation, now in ns: sub-tick cycles
      scale by a compile-time `NS_PER_CYCLE` when `configCPU_CLOCK_HZ` divides
      1e9 (the fast path), else one divide. Resolution is the cycle step, or the
      tick when SysTick is unavailable.
- [x] **zephyr** — keeps the #0531 shape (cycle counter where the board has one,
      tick otherwise) and reports the matching resolution for whichever arm is
      live.
- [x] **threadx** — `tx_time_get() * NS_PER_TICK`, a constant multiply where it
      used to divide; resolution is the full tick, stated honestly.
- [x] **esp-idf** — `esp_timer_get_time()` is µs, so `* 1000` and resolution
      1000.
- [x] **cffi test stubs** — the stub port tracks the new signature.

## W3 — The three Rust ports — **LANDED**

- [x] **mps2-an385**, **stm32f4**, **esp32-qemu** implement `clock_ns` +
      `clock_resolution_ns`; the `clock_us` that was `clock_ms() * 1000` is gone
      rather than reproduced.

## W4 — Callers — **LANDED**

- [x] `nros-c`'s `get_time_ns` stops fabricating nanoseconds by multiplying
      microseconds and calls the clock directly.
- [x] Executor timer accounting, `nros::time`, `nros-log` timestamps: unchanged
      call sites, now reached through the header/trait wrappers.

## W5 — Conformance — **LANDED**

- [x] Three cases in the cffi port tests: **monotonic**, **advances**
      (two reads either side of a 10 ms sleep differ by ≥ 5 ms), **resolution is
      honest** (no non-zero delta smaller than the reported resolution).
- [x] The *advances* case is the one that would have caught #0531 — a clock
      stuck at zero passes every test in the tree today.

*Acceptance:* the ABI mirror gate passes, `nros-platform-cffi` port tests pass
including the three new cases, `nros-node` lib tests pass, and the FreeRTOS
mps2-an385 QEMU lane runs the 3-phase demo with cadence unchanged.

---

## W6 — Retire the legacy symbols — **OPEN**

Deliberately not part of the landing change. `NROS_PLATFORM_LEGACY_CLOCK_UNITS`
exists so an out-of-tree port that still *defines* `clock_ms`/`clock_us` keeps
working for one release; W6 deletes the escape hatch and the macro after that
window. In-tree there is nothing left to retire — no port defines the old
symbols as of W2/W3.
