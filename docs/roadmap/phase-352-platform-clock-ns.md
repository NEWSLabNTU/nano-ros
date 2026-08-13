# Phase 352 — Platform clock: one nanosecond symbol, plus the resolution nobody could ask for

**Status (2026-08-13). COMPLETE — W1–W6 LANDED.**

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

## W6 — Retire the legacy symbols — **LANDED**

- [x] The `static inline` wrappers and `NROS_PLATFORM_LEGACY_CLOCK_UNITS` are
      gone from `<nros/platform.h>`. `nros_platform_clock_ms` and
      `nros_platform_clock_us` no longer exist in any form.
- [x] The Rust trait's provided `clock_ms`/`clock_us` conveniences go with
      them — `PlatformClock` is now exactly the two methods the ABI requires.
- [x] 30 call sites across 13 C/C++ files divide `clock_ns()` themselves.
- [x] `nros-cpp`'s no_std clock stopped scaling microseconds up by 1000 (the
      extra zeros were never real precision) and reads `clock_ns` directly.
- [x] **`nros new-platform`'s scaffold** emitted `clock_us`/`clock_ms` for
      every newly scaffolded port — it now emits `clock_ns` +
      `clock_resolution_ns`, so a new port is born on the current ABI rather
      than on one that was retired before it was written.

*Acceptance, met:* both gates pass (`check-platform-abi-mirror`,
`check-retired-platform-clock-symbols` over 576 tracked sources), core crate
tests pass (nros-node 261, nros-platform-cffi 2 + 11 port conformance,
nros-log 40, nros 8), and the FreeRTOS mps2-an385 QEMU lane runs the 3-phase
demo unchanged — ctrl 10.000 ms, steer 32.999, wd 100.024 against their
declarations, chain at its 29.5% ceiling, 95 emergency gate ticks through the
outage.

Not covered by this repo's checkout: the XRCE lane, whose
`micro-xrce-dds-client` submodule is not initialised here, so
`cargo build --workspace` stops at its build script. Its clock call sites were
converted with the rest and the retired-symbol gate covers its sources.
