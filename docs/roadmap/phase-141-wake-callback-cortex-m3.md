# Phase 141 — Wake-Callback Wiring on Embedded Backends + Cortex-M3 P99 Microbench

> **Renumbered 132 → 141 on 2026-05-17** to resolve the duplicate
> Phase 132 number. The other Phase 132 (`phase-132-cmsdk-uart-irq-
> driven.md`) landed first and keeps the 132 slot; sequential
> 133–140 were already taken.

**Goal.** Close Phase 124.B.2 acceptance: wake-latency P99
(subscriber-receive → callback-run) ≤ 100 µs on Cortex-M3 QEMU +
zenoh-pico, demonstrating ≥ 10× improvement over the pre-124.B
flag-only path. Requires shipping a wake-driven backend on
Cortex-M3 + an embedded latency-measurement harness that can
produce a P99 distribution.

**Status.** Not started.

**Priority.** P2. Phase 124's executor-side wake plumbing
(`wake_cv` / `NodeWake` / `wake_flag` + cv-wait spin) is
production-ready on POSIX and RTOS std (verified by Phase 124.B.8
microbench: 0 ms trigger-to-spin-exit, 100 ms-honoured negative
control). The Cortex-M3 acceptance was the only Phase 124 item
that needed a separate phase because it depends on backend +
embedded test infra work that isn't on Phase 124's critical path.

**Depends on.** Phase 124.B (executor wake mechanism — done),
Phase 130 (platform wake primitive — done).

## Overview

Phase 124.B specced an opt-in wake-callback path:

```
backend's transport-notify path  →  cb(ctx)         (124.B.1 slot)
                                       │
                                       v
                       runtime cb writes wake_flag
                       and signals wake_cv / NodeWake (124.B.2)
                                       │
                                       v
                  spin_once unblocks sub-poll-period
```

Every shipping cffi backend today leaves the
`set_wake_callback` slot NULL: XRCE-DDS-Client (no async notify —
poll-driven), Cyclone DDS / dust-DDS (listener threads exist but
not wired to the slot), zenoh-pico-cffi (the Rust shim has it on
POSIX std per `shim/session.rs:473`, but the embedded cffi build
path doesn't link a wake-firing thread). Result: on Cortex-M3 the
spin loop falls back to `drive_io(timeout_ms)`, the transport's
blocking `recv` yields up to `timeout_ms`, and effective wake
latency ≈ recv-poll period (typically 1–10 ms on smoltcp). 10–100×
the 100 µs target.

Closing 124.B.2 means:

1. **A wake-firing backend on Cortex-M3.** zenoh-pico is the only
   realistic candidate (no DDS on M3-class hardware). Needs an
   RX task / interrupt-driven hook on the embedded build that
   invokes the executor's `nros_rmw_runtime_wake_cb` on socket-
   readable.
2. **A µs-grain latency probe on Cortex-M3.** DWT CYCCNT cycle
   counter on entry to the transport notify path + exit on
   callback dispatch, with cycles-to-ns conversion at
   `SystemCoreClock`.
3. **Histogram aggregation + UART export.** Run N pub/sub cycles
   in QEMU, accumulate latency samples into a ring-buffered
   histogram, dump on UART. Host-side harness parses, computes
   P99, asserts the acceptance bounds.

## Work items

### 141.A — Wake-callback on embedded zenoh-pico

- [x] **141.A.1 — Decide RX driver shape.** **Picked option a
      (dedicated FreeRTOS RX task)** 2026-05-18 for the
      following reasons:
  - **Portability.** Option (a) matches the POSIX worker-thread
    shape (`shim/session.rs` already drives `zp_read` from the
    runtime); the same `nros_rmw_runtime_wake_cb` plumbing
    extends to FreeRTOS by swapping the storage primitive
    (`Wake<P>` instead of `std::sync::Condvar`). Works for any
    embedded transport (smoltcp, raw socket, UART), not just
    smoltcp.
  - **No transport carve-out.** Option (b) requires a callback
    slot inside smoltcp's `Interface::poll` path — couples the
    wake-fire to one driver. Phase 80 deliberately keeps the
    transport / driver split orthogonal; (b) would re-couple
    them.
  - **Existing infra.** `Wake<P>` from
    `packages/core/nros-platform-api/src/wake.rs` already
    provides a no_std-safe wait-signal primitive backed by the
    platform-cffi `nros_platform_wake_*` C ABI. FreeRTOS's impl
    in `packages/core/nros-platform-freertos/src/platform.c`
    routes through `xSemaphoreCreateBinary` /
    `xSemaphoreTake(timeout_ms)` / `xSemaphoreGive` /
    `xSemaphoreGiveFromISR` — exactly the contract 141.A needs.
  - **Bare-metal carve-out.** Pure bare-metal Cortex-M3 (no
    RTOS — `platform-bare-metal`) has NO wake primitive: single
    thread, no scheduler to wake. The spin-loop falls back to
    `cortex_m::asm::wfi()` / busy-spin. Phase 141 acceptance is
    explicitly scoped to **FreeRTOS on Cortex-M3** (the
    `freertos_armcm3` platform value, MPS2-AN385 board), NOT
    `platform-bare-metal`. Updated wording elsewhere in this
    phase doc reflects that scoping.

      **Files (forthcoming under 141.A.2 / .A.3).**
      - `packages/core/nros-node/src/executor/spin.rs` — extend
        `WakeCtx` + `nros_rmw_runtime_wake_cb` cfg from
        `feature = "std"` to `feature = "alloc"` + present-wake-
        primitive; replace `std::sync::Condvar`/`Mutex` with
        `Wake<P>` on no-std branch. Existing std path stays for
        POSIX.
      - `packages/zpico/nros-rmw-zenoh/src/shim/session.rs` —
        spawn a dedicated `zenoh-rx` task on FreeRTOS that owns
        the inner `zp_read` loop and fires `wake_cb` on
        data-arrival (mirrors the POSIX worker pattern at
        `shim/session.rs:449`).
- [x] **141.A.2 — `set_wake_callback` impl in
      `nros-rmw-zenoh` on `platform-bare-metal` /
      `platform-freertos`.** *(landed — verified 2026-05-18:
      `packages/zpico/nros-rmw-zenoh/src/shim/session.rs:473`
      has no platform-posix cfg gate; `wake_cb` /`wake_ctx`
      AtomicPtr fields are stored unconditionally and
      `drive_io` (line 449) fires the cb when zenoh-pico's
      spin_once observes work. The doc statement that the
      shim was POSIX-only was stale.)*
- [ ] **141.A.3 — ISR-safety contract verification.** If
      141.A.1 goes interrupt-driven, the cb must use the
      ISR-safe wake primitive (`nros_platform_wake_signal_from_isr`
      from Phase 130.1, k_sem-equivalent on FreeRTOS). Verify
      against Phase 130's ISR contract documented in
      `docs/reference/platform-sync-abi.md`.

### 141.B — µs-grain latency probe on Cortex-M3

- [x] **141.B.1 — DWT CYCCNT helper in
      `nros-platform-mps2-an385`.** Cortex-M3 has DWT
      (`0xE0001000`) and CYCCNT (`+0x4`); init via DEMCR
      `TRCENA` + DWT_CTRL `CYCCNTENA`. Expose
      `clock_cycles() -> u32` and
      `cycles_to_ns(cycles, system_core_clock_hz) -> u64`. Read
      on transport-notify entry + executor-dispatch exit; diff
      gives sub-µs wake latency. *(landed 2026-05-18 —
      `CycleCounter::{enable, read, measure, cycles_to_ns}` +
      free-fn aliases `clock_cycles()` / `cycles_to_ns()` in
      `packages/platforms/nros-platform-mps2-an385/src/timing.rs`.)*
- [ ] **141.B.2 — Instrumentation hooks in executor +
      transport.** Two probe points: (a) inside
      `nros_rmw_runtime_wake_cb` (entry — "transport notified
      executor at T₀"), (b) at the top of arena dispatch when
      the bit for the matching subscription fires
      ("subscription callback ran at T₁"). Gate behind
      `feature = "wake-latency-probe"` so production builds
      stay clean.

### 141.C — Histogram aggregation + UART export

- [ ] **141.C.1 — Ring-buffered histogram in `nros-tests`
      embedded harness.** Logarithmic bucket distribution (1 µs
      → 100 ms) sized to ~1 KB stack budget. Sample push from
      the instrumentation hooks (141.B.2). Dump format: CSV
      bucket-edge,count over UART, terminated by a sentinel
      line.
- [ ] **141.C.2 — Host-side parser + assertion.** Test bin
      runs FreeRTOS QEMU + zenoh-pico talker/listener pair,
      drains UART, parses histogram, computes P99, compares to
      pre-124.B baseline (captured once with
      `set_wake_callback = NULL` to establish the ≥ 10× claim).

### 141.D — Microbench scenarios

- [ ] **141.D.1 — Single sub, 100 Hz pub.** Steady-state P99
      under nominal load.
- [ ] **141.D.2 — 4 idle subs + 1 active sub.** Mirrors Phase
      124.G.1 4-sub-idle topology so the wake fan-out cost on
      embedded is visible.
- [ ] **141.D.3 — Burst (10 messages back-to-back).** Worst-case
      P99 when several wakes pile up inside one cv-wait cycle.

## Acceptance criteria

- [ ] P99 wake-latency ≤ 100 µs on Cortex-M3 QEMU (MPS2-AN385) +
      zenoh-pico across the 141.D scenarios.
- [ ] ≥ 10× improvement over the same scenarios with the
      pre-124.B `set_wake_signal` flag-only path (captured as a
      one-time baseline so future regressions show up against a
      stable reference).
- [ ] Histogram CSV + analysis logged to `test-logs/` per CI run
      so latency regressions are visible without re-running the
      microbench.
- [ ] No regression in existing FreeRTOS QEMU pub/sub/service/
      action E2E (Phase 130.7 sweep: 9/9 green) under the new
      RX task / wake wiring.

## Notes

- The "10× improvement" baseline must be captured BEFORE
  enabling the RX task wake-callback so the comparison is
  apples-to-apples. Land the harness + baseline measurement
  before flipping the cb-install switch.
- Phase 130.8 removed the legacy blocking `call_raw` fallback in
  CFFI; any backend reaching Cortex-M3 today already provides
  the non-blocking `send_request_raw` / `try_recv_reply_raw`
  slots. Wake-callback is a separate opt-in (still NULL today)
  that this phase fills in for zenoh-pico embedded.
- DDS backends (Cyclone, dust-DDS) on Cortex-M3 are out of scope
  — neither runs on M3-class hardware. The Cortex-M3 acceptance
  is specifically for zenoh-pico. A separate phase could
  later add wake-cb wiring for Cyclone/dust-DDS on RTOS std
  (Zephyr, FreeRTOS Linux SITL) using the same executor-side
  plumbing.
- The Phase 124.B.7 ISR-safe contract
  (`nros_platform_condvar_signal_from_isr`) + Phase 130.1
  `nros_platform_wake_signal_from_isr` are the per-platform
  primitives the wake-callback fires through if 141.A.1 picks
  the interrupt-driven path. No new platform ABI needed.

## Cross-references

- `docs/roadmap/phase-124-rmw-zero-copy-dispatch.md` — 124.B
  executor wake mechanism + the 124.B.2 acceptance criterion
  this phase closes.
- `docs/roadmap/archived/phase-130-platform-wake-primitive.md`
  — platform wake primitive that fires this phase's RX-task
  signals.
- `docs/reference/platform-sync-abi.md` — per-platform wake
  primitive contract + ISR-safety rules.
- `packages/zpico/nros-rmw-zenoh/src/shim/session.rs:473` —
  existing POSIX-std `set_wake_callback` impl to extend.
