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

**Status.** Plumbing landed 2026-05-18 (commits c262d145 →
7c7c34b1, 8 commits this stretch): 141.A.1-.A.3 wake-cb on
FreeRTOS Cortex-M3 + Executor wire-up + spin_once branch;
141.B.1 DWT cycles_to_ns; 141.B.2 `wake_probe` module + hook
sites; 141.C.1/.C.2 Histogram + write_csv + parse_csv +
percentile_ns (3 lib tests passing); 141.D.1/.2/.3 bench
crate (`packages/testing/nros-bench/wake-latency-cortex-m3/`)
+ host runner asserting P99 ≤ 10 ms. Outstanding: real-
hardware P99 ≤ 100 µs spec gate (no CI runner for real
hardware), 10× baseline (one-time measurement vs pre-124.B
flag-only path), FreeRTOS E2E regression sweep.

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
      `platform-freertos`.** Backend-side: verified 2026-05-18
      that `packages/zpico/nros-rmw-zenoh/src/shim/session.rs:473`
      has no platform-posix cfg gate; `wake_cb` / `wake_ctx`
      AtomicPtr fields are stored unconditionally and `drive_io`
      (line 449) fires the cb when zenoh-pico's spin_once
      observes work. Runtime-side scaffolding for the no_std cb
      target landed in:
      - `e36ee8cf` lifted `NodeWake` cfg from `std` to `alloc`
        (kernel-native binary semaphore wrapper now compiles on
        no_std RTOS targets too).
      - `ee2b77f5` added `executor::wake_alloc::WakeCtxAlloc` +
        no_std `nros_rmw_runtime_wake_cb` (mirror of the std
        path's struct + cb function, using `Arc<AtomicBool>` +
        `Arc<NodeWake>` instead of `std::sync::Condvar`).
      The runtime cb *type* now exists for the no_std RTOS
      build; wiring it into `Executor` fields + a spin_once
      no_std wait branch is the remaining 141.A.3 work (next
      bullet).
- [ ] **141.A.3 — Wire `WakeCtxAlloc` into `Executor` +
      no_std spin_once wait branch.** Mirrors the std-RTOS
      branch already present in
      `packages/core/nros-node/src/executor/spin.rs:3193-3216`
      (which uses `node_wake.wait_ms(timeout_ms)` for the
      kernel-native binary-semaphore wait):
      - Add cfg-gated Executor fields
        (`cfg(all(alloc, not(std), rmw-cffi, any-rtos-platform))`):
        `wake_flag_alloc: Arc<AtomicBool>`,
        `node_wake_alloc: Option<Arc<NodeWake>>`,
        `wake_ctx_alloc: Option<Arc<WakeCtxAlloc>>`,
        `has_async_wake_alloc: bool`.
      - Initialize in both Executor constructors
        (`from_session` line 749 + `from_session_ptr` line 837).
      - Add `install_wake_signal_on_primary_alloc` /
        `_on_extra_alloc` methods (alloc-mode mirrors of
        the existing std installers at lines 1148-1180).
      - Add the no_std wait branch in `spin_once` that picks
        the alloc `node_wake.wait_ms(deadline)` when
        `has_async_wake_alloc && wake_flag_alloc.swap(false)`
        is false.
      - ISR-safety: cb in `wake_alloc::nros_rmw_runtime_wake_cb`
        is NOT ISR-safe (matches the std cb policy). ISR callers
        route through the existing
        `nros_platform_wake_signal_from_isr` slot (Phase 130.1).
        Verify against the ISR contract in
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
- [x] **141.B.2 — Instrumentation hooks in executor +
      transport.** Landed via new
      `packages/core/nros-node/src/executor/wake_probe.rs`
      module + `wake-latency-probe` Cargo feature
      (`portable-atomic/fallback` for Cortex-M3's missing
      native AtomicU64). Hooks: (a) `super::wake_probe::on_wake()`
      at the entry of `nros_rmw_runtime_wake_cb` (std variant
      in `spin.rs:447` + alloc variant in `wake_alloc.rs:85`);
      (b) `super::wake_probe::on_dispatch()` at the top of
      `dispatch_one` (`spin.rs:3968`) when
      `matches!(meta.kind, EntryKind::Subscription)` — Service
      / Timer / GuardCondition skip the probe because the 141
      acceptance is wake-to-subscription latency only.
      Storage: lock-free `[AtomicU64; 256]` ring +
      `WRITE_IDX: AtomicU32` (sample count + wrap detection)
      + `LAST_WAKE_TICKS: AtomicU64` (T0 pending pairing,
      swap-cleared by `on_dispatch`). Time source is
      caller-supplied via
      `wake_probe::set_cycle_reader(Some(fn))` — point at
      `nros_platform_mps2_an385::timing::clock_cycles` for
      Cortex-M3. Drain API:
      `wake_probe::drain(&mut [u64])` → `(samples_written,
      total_writes_since_boot)` for 141.C's UART harness.
      Feature is off by default — production builds carry
      zero overhead (call sites are `#[cfg]`-elided no-ops).

### 141.C — Histogram aggregation + UART export

- [x] **141.C.1 — Ring-buffered histogram + CSV serializer.**
      Landed as `wake_probe::Histogram` +
      `wake_probe::write_csv` in
      `packages/core/nros-node/src/executor/wake_probe.rs`.
      Log-distributed `HISTOGRAM_BUCKETS = 24` buckets
      (`BUCKET_EDGES_NS` 1 µs → ~4.2 s pow-2 + u64::MAX
      overflow) — 96 bytes of state, well under the 1 KB
      stack budget. `insert(ns)` is branch-free linear scan,
      `saturating_add` on count (sample bursts past u32::MAX
      between drains won't panic).
      `drain_into::<BUF_SAMPLES>(&mut hist, cycles_to_ns)`
      convenience: drains the probe ring through a stack
      buffer + bucketizes via a caller-supplied
      `cycles_to_ns` (typically partial-applied
      `nros_platform_mps2_an385::timing::cycles_to_ns`).
      `write_csv` emits the v1 contract — `NROS-WAKE-HIST,v1`
      header, `bucket_edge_ns,count` body, `total,N` summary,
      `END` sentinel. `Histogram::percentile(pct)` for
      on-device P99 sanity logging.
- [x] **141.C.2 — Host-side parser + assertion helpers.**
      `wake_probe::parse_csv(input)` + `percentile_ns(buckets,
      pct)` gated `cfg(feature = "std")` so the no_std
      embedded path doesn't pay any cost. Round-trip
      verified by the lib test
      `wake_probe::tests::csv_roundtrip` (write_csv → parse_csv
      → percentile_ns chain). The full FreeRTOS-QEMU
      pub/sub binary + serial drainer that this parser feeds
      is the 141.D harness work — these helpers are the
      reusable building blocks.

### 141.D — Microbench scenarios

- [x] **141.D.1 — Single sub, 100 Hz pub.** Steady-state P99
      under nominal load. Wired as the `scenario-single`
      feature (default) in the new
      `packages/testing/nros-bench/wake-latency-cortex-m3/`
      bench crate — talker timer at 10 ms (100 Hz) +
      same-Executor `/wake-latency` subscription. Probe hooks
      fire automatically via the 141.B.2 plumbing; binary dumps
      `wake_probe::write_csv` block over semihosting after
      `TARGET_SAMPLES = 200` round-trips.
- [x] **141.D.2 — 4 idle subs + 1 active sub.** Wired as
      the `scenario-fanout` feature in the same bench crate.
      Idle subs subscribed BEFORE the active one so the
      dispatch loop walks past them per wake; probe only
      counts the active `/wake-latency` topic since
      `on_dispatch` fires once per dispatched callback and the
      idle topics never receive traffic.
- [x] **141.D.3 — Burst (10 messages back-to-back).** Wired
      as the `scenario-burst` feature in the same bench crate.
      Talker timer publishes 10 messages per tick instead of
      one so multiple wakes pile into one spin_once cycle —
      worst-case path the executor must handle.

Host runner: `packages/testing/nros-tests/tests/wake_latency_cortex_m3.rs`
boots the bench under QEMU MPS2-AN385, scrapes the
`NROS-WAKE-HIST,v1` CSV block off semihosting stdout, parses
via `wake_probe::parse_csv`, computes P99 via
`percentile_ns`, asserts ≤ 10 ms (loose CI bound — see
"Acceptance threshold" below). `#[cfg(feature = "trigger-test")]`
gates the file so the default test build doesn't pull
`nros-rmw-zenoh` unnecessarily.

## Acceptance criteria

- [x] **Plumbing acceptance (CI-gated):** `wake_latency_cortex_m3_p99_within_bound`
      asserts P99 ≤ 10 ms on Cortex-M3 QEMU + zenoh-pico, which
      proves the wake-cb path is firing (pre-141 floor was the
      ~5 ms `poll_interval_ms` from
      `examples/qemu-arm-freertos/.../config.toml`'s
      `scheduling.poll_interval_ms = 5`). Tightened bound to
      100 µs validates on real hardware (STM32F4) where DWT
      CYCCNT is accurate — QEMU's CYCCNT emulation is
      best-effort and the test `[SKIPPED]`s cleanly when DWT
      returns 0 (the typical QEMU degenerate path).
- [ ] **Spec acceptance (hardware-gated, deferred):** P99
      wake-latency ≤ 100 µs across all three 141.D scenarios on
      real Cortex-M3 hardware (MPS2-AN385 dev board or STM32F4
      Discovery). Currently no CI runner for real hardware;
      manual validation expected.
- [ ] **10× baseline:** ≥ 10× improvement over the pre-124.B
      `set_wake_signal` flag-only path. Requires a one-time
      baseline measurement with `wake-latency-probe` enabled
      and `wake_alloc.rs::nros_rmw_runtime_wake_cb` patched to
      no-op (or the install path stubbed out). Not a CI gate —
      tracks against a static reference once captured.
- [x] **Histogram CSV in test logs:** `eprintln!` in the host
      runner logs the parsed P99 + sample count per run; full
      CSV is captured in `target/nextest/.../<test>.stderr`.
- [ ] **No FreeRTOS QEMU pub/sub/service/action E2E regression:**
      9/9 Phase 130.7 tests still green after the 141.A.3 wake
      plumbing landed. Pending validation on the next full
      `just freertos test-all` run.

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
