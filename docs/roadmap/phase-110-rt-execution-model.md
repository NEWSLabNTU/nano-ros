# Phase 110 — RT Execution Model (Intra-Executor Scheduling)

**Goal:** Refactor the nano-ros executor to support mixed-criticality callbacks (Critical / Normal / BestEffort) on one executor with pluggable scheduling policies (FIFO / EDF / Sporadic), keeping the user-facing ROS-style API stable across scheduler swaps. Default uses **1 OS priority slot per executor thread** — scheduling decisions live in user space, not in OS priority slots (avoids PiCAS-style slot starvation on platforms like Cortex-M0+ which only has 4 NVIC levels).

**Status:** v1 in progress — 110.0 / A / B / C / D-foundation landed (POSIX/NuttX/FreeRTOS/Zephyr/ThreadX/cffi/Orin SPE PlatformScheduler impls + multi-exec lifecycle). Drone S1 / watchdog S3 timing acceptance + 110.E–G post-v1 deferred.

**Priority:** High

**Depends on:** Phase 79 (PlatformYield trait), Phase 76 (config.toml plumbing), Phase 77 (`zpico_spin_once` wake primitive). Coordinates with Phase 94 (RTOS orchestration — emits per-callback `SchedContext` bindings from launch manifests in future) and Phase 88 (nros-log — uses `BestEffort` SC for log sinks). **Absorbs former Phase 105** — the RMW-side `Session::next_deadline_ms` work lives here as 110.0; former 105's `max_callbacks` + timer/GC interleaving + wall-clock budget are all subsumed by 110.A's Activator + ReadySet design (cap and budget enforcement live in the executor's dispatch loop, not in `drive_io`). **Absorbs Phase 104's E-thread tail** — the cross-backend bridge follow-ups (former 104.E.2 PiCAS bridge test, 104.E.4 ARINC TT bridge example) migrated to 110.F.bridge + 110.G.bridge respectively since their runtime gates live here, not in the bridge-design phase.

**Design:** [docs/design/rt-execution-model.md](../design/rt-execution-model.md)

---

## Overview

Today's `Executor::spin_once` (`packages/core/nros-node/src/executor/spin.rs:1498..1731`):

1. `drive_io(timeout_ms)` pumps transport.
2. Bitmap scan over entries in **registration order**.
3. Trigger evaluation (`Any/All/AnyOf/...`).
4. Dispatch loop in **registration order**, run-to-completion.

No criticality, no priority, no deadline. Critical callbacks can be delayed by registration-order predecessors. No way to bind an OS scheduling policy (e.g. NuttX `SCHED_SPORADIC`) at the executor level.

Phase 110 splits the executor into four pluggable abstractions:

- **`JobDescriptor`** — static per-callback metadata (period, WCET hint, SC binding).
- **`SchedContext`** — first-class scheduling capability (period, budget, deadline, class). Inspired by **seL4 MCS**. Multiple callbacks share one SC; one OS pri slot per Executor regardless of callback count.
- **`ReadySet`** — queue + selection abstraction (FIFO bitmap, EDF heap, bucketed). Idempotent `insert`. One ready bit per callback regardless of message count.
- **`Activator + Dispatcher`** — replace the bitmap scan + `try_process` call sites. `Trigger` evaluation moves into `Activator::scan`.

**Key correctness boundary:** single-thread, non-preemptive execution **cannot guarantee hard deadlines across SchedClasses** (a 50 ms `Fifo` callback blocks a 1 ms `Edf` deadline). For hard-RT (drone, watchdog, kHz control), Phase 110.D's multi-executor is **mandatory**. For soft-RT mixed-criticality (mobile robot pipelines, sensor aggregation), single-thread `BucketedEdfSet` (110.C) suffices.

**ABI compatibility:** time fields use sentinel `0` for "absent" in C/C++ headers; Rust API uses `Option<NonZeroU32>` via `#[repr(transparent)]` newtype `OptUs(u32)` so cbindgen emits plain `uint32_t`. No alloc dependency, no `dyn Transport` trait — `Activator::scan` reads handle metadata only after `drive_io` returns.

See [design doc](../design/rt-execution-model.md) for full per-RTOS fit checks, scenario catalogue (S1–S12), and references to PiCAS / CIL-EDF / HSE / Budget-Micro-ROS / seL4 MCS.

---

## Work Items

### v1 (Phases 110.0–110.D — required)

- [x] 110.0 — `Session::next_deadline_ms()` RMW trait method + per-backend impls (RMW-side, lands first or in parallel)
- [x] 110.A — Refactor: `Activator + ReadySet + Dispatcher` + ISR SPSC ring (behavioural no-op)
- [x] 110.B — `SchedContext` API + `OptUs` newtype + `EdfReadySet` (incl. C/C++ wrappers)
- [x] 110.C — `BucketedFifoSet<N>` + `BucketedEdfSet<N>` (HSE-style criticality split)
- [~] 110.D — `Executor::open_threaded` + `PlatformScheduler` trait per RTOS
      Trait + POSIX/NuttX/FreeRTOS/Zephyr/ThreadX/cffi/Orin SPE
      impls landed; `open_threaded` + `ThreadHandle` lifecycle
      landed; multi-exec smoke test green. Drone S1 / watchdog S3
      timing acceptance defers to a privileged-scheduling
      integration harness (CAP_SYS_NICE + multi-core wall-clock
      measurement).

### Post-v1 (Phases 110.E–110.G)

- [~] 110.E — `SchedClass::Sporadic` + budget refill timer (NuttX-native + user-space fallback)
      v1 landed:
      * Linux `SCHED_DEADLINE` via direct `sched_setattr` syscall
        (x86_64 / aarch64 / riscv64).
      * NuttX `SCHED_SPORADIC` via `sched_setscheduler` + augmented
        `sched_param`.
      * User-space `SporadicState` w/ polled-clock refill — std-only.
      Both syscall paths need privileged execution + matching kernel
      config to actually take effect; per-platform integration tests
      follow once the privileged-scheduling harness ships.

      **110.E.b in progress** — ISR-driven refill on no-std platforms
      (FreeRTOS / Zephyr / ThreadX / bare-metal). Requires
      `PlatformTimer` trait + `AtomicSporadicState` rewrite — design
      locked in
      [`docs/design/phase-110-e-platform-timer.md`](../design/phase-110-e-platform-timer.md).
      ~620 LOC across 5 crates, ~3-5 dedicated sessions. Executor
      stays platform-agnostic via opaque-handle pattern (mirrors
      `Executor::open_threaded`'s `apply_policy: fn(...)` shape).
      Per-callback runtime measurement + `cancel` / `restart_oneshot`
      land in a follow-up to 110.E.b once `PlatformTimer` is in
      place.

      Landed pieces:
      * Trait `PlatformTimer` + `TimerError` in `nros-platform-api`
        (default `Unsupported` for `create_periodic` /
        `create_oneshot`, no-op `destroy`, false `cancel`).
      * `AtomicSporadicState` + `atomic_sporadic_refill_thunk` in
        `nros-node::executor::sched_context` (atomic budget + Release
        store refill thunk safe in any thread / ISR context).
      * `OpaqueTimerHandle` + `Executor::register_sporadic_timer` in
        `nros-node::executor::spin` (Drop walks `sporadic_atomic_states`
        and frees timers via the per-platform destroy thunk).
      * Canonical C ABI in `<nros/platform_timer.h>` +
        `nros_platform_timer_create_periodic` /
        `nros_platform_timer_create_oneshot` /
        `nros_platform_timer_destroy` /
        `nros_platform_timer_cancel`.
      * **POSIX C port** (`nros-platform-posix/src/timer.c`) backed by
        `timer_create(CLOCK_MONOTONIC, SIGEV_THREAD)` + trampoline.
      * **`impl PlatformTimer for CffiPlatform`** in
        `nros-platform-cffi` — dispatches the trait through the C ABI.
        `CffiTimerHandle` newtype carries the opaque `*mut c_void`
        with `Send + Sync` impls.
      * End-to-end test `c_port_posix_timer::rust_trait_atomic_sporadic_refill_round_trip`
        drains an `AtomicSporadicState`, attaches the periodic timer
        via the Rust trait + `atomic_sporadic_refill_thunk`, and
        asserts the budget refilled back to its declared capacity
        from the POSIX timer callback (6/6 tests in the file PASS).
      * **Per-RTOS native C ports** (Phase 121.6) already provide
        `nros_platform_timer_*` symbols for POSIX / FreeRTOS / Zephyr
        / ThreadX / ESP-IDF; NuttX shares POSIX's `timer.c` via
        `add_library` (its CMakeLists pulls
        `${NROS_PLATFORM_POSIX_C_SRC_DIR}/timer.c` directly). Every
        hosted-RTOS target therefore exposes the canonical timer
        symbols through the same link path as the POSIX reference.
      * **Bare-metal stub exports** (Phase 110.E.b) — Mps2An385 /
        Stm32f4 / Esp32QemuPlatform / Esp32Platform impl
        `PlatformTimer` with the trait's default
        `TimerError::Unsupported` and emit the C symbols via
        `nros_platform_export_timer!` so the bare-metal link line
        always resolves cleanly. Calls degrade to a NULL handle /
        `KernelError`; ISR-driven refill on bare-metal still needs
        the per-board `SysTickHook` work flagged in
        `docs/design/phase-110-e-platform-timer.md` step 9 (separate
        per-board investment, design-deferred).

      v1 trait surface + every supported hosted RTOS now has a
      functional `PlatformTimer` port.

      **Per-callback runtime accounting (Phase 110.E.b follow-up step
      10) landed.** `spin_once`'s per-bucket dispatch loop now
      wall-clock-times each invocation of `dispatch_one` and feeds
      the elapsed microseconds into the bound SC's
      `AtomicSporadicState::consume` (atomic-path Sporadic SCs
      only). Replaces the cycle-level `delta_us` over-attribution
      that previously charged every Sporadic SC the full spin
      duration regardless of which callbacks fired. Verified by the
      new `test_atomic_sporadic_per_callback_runtime_consumed`
      test: a 10 ms sleeping callback consumes ≥10 000 us / <500 000
      us from a 1 s budget (proves dispatch-local measurement, not
      cycle-level over-attribution). Existing
      `test_sporadic_budget_exhaustion_suppresses_dispatch` still
      passes — the polled `SporadicState` path is unchanged for
      no_std consumers.

      **Per-board MPS2-AN385 ISR refill landed.**
      `nros-platform-mps2-an385::sporadic_timer` drives CMSDK
      Timer1 with `#[interrupt] fn TIMER1()` calling the registered
      `atomic_sporadic_refill_thunk` from ISR context. Single
      periodic-callback slot (v1) backs
      `<Mps2An385Platform as PlatformTimer>::create_periodic`.
      Build verified: `nros-platform-mps2-an385` compiles for
      thumbv7m-none-eabi; `qemu-rtic-talker` example still builds
      + `test_qemu_rtic_pubsub_e2e` PASSES (talker doesn't bind a
      Sporadic SC so the Timer1 path stays dormant). Bare-metal
      callers needing sporadic refill can now register an
      `AtomicSporadicState` via `Executor::register_sporadic_timer`
      and receive ISR-driven budget refresh without a polled
      fallback.

      **stm32f4 / esp32 / esp32-qemu hook surface landed.** These
      three platforms own peripherals via their board crates'
      `Peripherals` split (stm32f4xx-hal handles, esp-hal v1's
      ownership model), so the platform crate can't drive a fixed
      timer directly the way mps2-an385 does with CMSDK Timer1.
      Each platform now exposes
      `sporadic_timer::install_periodic_timer_hook(register,
      destroy)`: a board crate (or user-init) calls it once during
      boot with function pointers that arrange a periodic IRQ on
      whichever timer the board has reserved (TIM2/TIM3/TIM5 on
      stm32f4, `esp_hal::timer::PeriodicTimer` /
      `SystemTimer` alarms on ESP32-C3). The platform's
      `PlatformTimer::create_periodic` dispatches through the hook;
      `nros_platform_timer_*` returns NULL when no hook is
      installed so cross-platform code degrades gracefully. mps2
      remains the canonical "drive the timer directly" reference
      for the simpler / fully-owning case.

      **Callback overrun detection landed.** Cooperative
      single-thread dispatch can't preempt a runaway callback, so
      post-dispatch wall-clock comparison delivers the same
      observable signal as the design's oneshot-IRQ-and-cancel
      pattern without needing a separate timer per SC.
      `AtomicSporadicState` gained `overrun_count: AtomicU32` +
      `last_overrun_us: AtomicU32` (set when `elapsed_us >
      budget_capacity_us` inside the per-callback runtime closure)
      + `record_overrun(us)` / `clear_overrun_stats()` helpers.
      Verified by `test_atomic_sporadic_overrun_recorded_when_callback_exceeds_budget`
      — a 25 ms sleeping callback against a 5 ms budget records
      one overrun with `last_overrun_us ≥ 20_000`. The trait's
      `create_oneshot` + `cancel` stays available for callers that
      want an actual IRQ-driven oneshot (e.g. for waking a watchdog
      on a separate executor thread); the spin-loop dispatch path
      itself uses the cheaper wall-clock signal.

      v1 Phase 110.E.b acceptance criteria fully met. Remaining
      per-board work (stm32f4 / esp32 / esp32-qemu actual timer
      driver populating the hook surface) is downstream consumer
      investment.
- [~] 110.F — `OsPrioritySet` per-priority OS-thread dispatch.
      **Reframed:** Cargo feature `scheduler-os-priority` + stub
      `OsPrioritySet<N>` shipped to lock the namespace; real
      dispatch model intentionally **deferred to a future
      node-orchestration phase** that will pick the canonical
      callback-to-priority mapping. nano-ros may not adopt PiCAS as
      written. Trait surface (`ReadySet`) stays compatible; whatever
      orchestration model lands plugs into the existing
      Activator/Dispatcher seams from 110.A.
- [x] 110.G — `SchedClass::TimeTriggered` (ARINC-653-style cyclic
      executive). Runtime gate (`spin_once`'s per-cycle phase check
      against `tt_window_offset_us` / `tt_window_duration_us`)
      landed pre-session via `Executor::register_time_triggered_dispatcher`.
      Schedule-table declarative API + validator + builder now land
      this session:

      * `TimeTriggeredWindow { offset_us, duration_us, name }` —
        one cyclic slot, static-lifetime `name` for diagnostics.
      * `TimeTriggeredSchedule<const N>` — fixed-size array of
        windows + `major_frame_us` + `window_count` for active
        prefix; `new_full` constructor + `validate()` that rejects
        `ZeroMajorFrame`, `ZeroWindowDuration`,
        `WindowExceedsMajorFrame`, `WindowsOverlap` (O(N²)
        sliding-window comparison — TT schedules are small).
      * `Executor::apply_time_triggered_schedule(&schedule)` —
        one-shot builder: validates, sets `major_frame_us`, creates
        one `SchedContext` per window with the right
        `tt_window_*` fields, returns `[SchedContextId; N]` for
        callers to `bind_handle_to_sched_context` against.

      Verified by two tests in `executor::tests`:
      * `test_apply_time_triggered_schedule_dispatches_only_active_window`
        — two subscriptions bound to windows `[0..1s)` + `[1s..2s)`
        within a 2 s major frame; one `spin_once` early in the
        cycle dispatches only the window-0 entry, window-1 stays
        suppressed.
      * `test_time_triggered_schedule_rejects_overlapping_windows`
        — overlapping or oversize windows surface structured
        `TimeTriggeredScheduleError` rather than silent dispatcher
        precedence.

      Deferred to a future session if a real consumer surfaces: a
      `nano-ros.toml` schedule-table parser + a load-from-TOML
      example. The Rust struct API is sufficient for embedded
      consumers that declare their schedule at compile time.

---

### 110.0 — `Session::next_deadline_ms()` RMW trait method (RMW-side prerequisite)

The backend internally schedules events: zenoh-pico's lease keepalive, XRCE-DDS's session ping, dust-DDS's heartbeats. The executor's `spin_once` should cap its `drive_io` timeout against the soonest of (user_timeout, timer_deadline, this) — otherwise on quiet links the backend wakes early, sees no user-visible work, calls `drive_io` again. Wasted round-trips.

```rust
pub trait Session {
    /// Backend's next internal-event deadline (keepalive, heartbeat,
    /// lease expiry). The runtime caps its `drive_io` timeout against
    /// `min(user_timeout, timer_deadline, this)`. Returns `None` if
    /// the backend has no internal deadlines or chooses not to expose
    /// them.
    fn next_deadline_ms(&self) -> Option<u32> { None }
}
```

C side — optional vtable function pointer (NULL = no deadline):

```c
typedef struct nros_rmw_vtable_t {
    /* … */
    int32_t (*next_deadline_ms)(const nros_rmw_session_t *session);
} nros_rmw_vtable_t;
```

**Backend opt-in matrix:**

| Backend | Deadline source | Plan |
|---------|-----------------|------|
| zenoh-pico | Lease keepalive interval | Track `last_keepalive_sent + LEASE_INTERVAL` in shim. ~15 LOC. |
| dust-DDS | Per-writer heartbeat period; per-reader ACK-NACK timeout; participant liveliness lease | Implement `DdsRuntime::next_event_time`. ~30 LOC. |
| XRCE-DDS | Heartbeat to agent; session ping | Mirror in shim: `last_run + heartbeat_period - now()`. ~15 LOC. |
| uORB | None — intra-process, no keepalives | Keep default `None`. 0 LOC. |

110.A's refactored `spin_once` consumes this trait method when computing `effective_timeout`:

```rust
let next_timer = self.timers.next_deadline_ms();
let next_session = self.session.next_deadline_ms();
let effective_timeout = [Some(user_timeout), next_timer, next_session]
    .into_iter().flatten().min().unwrap();
```

**Files:**

- `packages/core/nros-rmw/src/traits.rs` — `Session::next_deadline_ms` default trait method.
- `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h` — optional vtable function pointer.
- `packages/core/nros-rmw-cffi/src/lib.rs` — Rust mirror.
- `packages/zpico/nros-rmw-zenoh/src/` — zenoh lease deadline tracking.
- `packages/dds/nros-rmw-dds/src/` — dust-DDS min-over-entities.
- `packages/xrce/nros-rmw-xrce/src/` — XRCE heartbeat schedule mirror.
- Tests: per-backend test that verifies `next_deadline_ms` caps the wait to the keepalive interval on a quiet link.

**Acceptance:** zenoh-pico `next_deadline_ms` keeps drive_io from waking sooner than the lease deadline + tolerance band on a quiet link (verified by counting drive_io return events in a 30 s window). Independent of 110.A — can land first or in parallel.

---

### 110.A — Refactor: `Activator + ReadySet + Dispatcher` + ISR SPSC ring

Behavioural no-op refactor of `spin_once`. Default `R = FifoReadySet` reproduces today's registration-order dispatch bit-for-bit. Adds the `ReadySet` trait + `Activator + Dispatcher` traits + lock-free SPSC ring between ISR and executor (reuses existing `executor/spsc_ring.rs`).

**Decisions to lock:**

- `ReadySet` stores `(SortKey, DescIdx)` pairs only (never full `ActiveJob`). Full job info reconstructed from `JobDescriptor[idx]` lookup at dispatch.
- `insert` is **idempotent** — second insert for already-ready `desc_idx` is no-op. Matches default ROS 2: one ready bit per callback regardless of message count; callback drains rmw queue per QoS.
- `Activator::scan(&ctx, &mut ready)` runs *after* `session.drive_io(timeout)`. `Trigger::{Any | All | AnyOf | AllOf | One | Predicate | RawPredicate}` evaluated inside `scan`.
- `Dispatcher` binds arena pointer at construction; no raw `*mut u8` per call.
- `DrainMode::{Latched (default), Greedy}`. Latched preserves current snapshot semantics.

**Files:**

- `packages/core/nros-node/src/executor/ready_set.rs` — new module: `ReadySet` trait, `FifoReadySet<const N>`, `Overflow` error.
- `packages/core/nros-node/src/executor/activator.rs` — new module: `Activator` trait, `ActivatorCtx`, default impl reproducing current bitmap scan.
- `packages/core/nros-node/src/executor/dispatcher.rs` — new module: `Dispatcher` trait, default impl wrapping current `try_process`.
- `packages/core/nros-node/src/executor/spin.rs` — refactor `spin_once` to compose `Activator + ReadySet + Dispatcher`. SPSC ring drain at top of cycle.
- `packages/core/nros-node/src/executor/spsc_ring.rs` — already exists; expose `try_pop` to `spin_once`.
- `packages/core/nros-node/src/executor/types.rs` — add `DescIdx`, `SortKey`, `ActiveJob`, `DrainMode` types.
- Tests: `nros-tests` parity oracle — every existing executor test must pass unchanged.

**Acceptance:** `just test-all` green; behavior bit-identical to current. No new public API surface yet.

---

### 110.B — `SchedContext` API + `OptUs` newtype + `EdfReadySet`

User-facing API. Adds `SchedContext { class, period, budget, deadline, deadline_policy, os_thread }`, builder methods on `Executor`, `EdfReadySet<N>`, and the `OptUs(u32)` `#[repr(transparent)]` newtype for ABI-stable optional time fields.

**Decisions to lock:**

- Default `SchedContext` auto-created at executor startup w/ `class = Fifo`; `add_subscription` (no `_in`) binds to it. Existing examples unchanged.
- Const-generic `Executor<const MAX_HANDLES, const MAX_SC = 8>`.
- `DeadlinePolicy::{Released, Activated, Inherited}` — released for timers, activated for event-triggered subs, inherited carries deadline in message header (latency-aware).
- `OptUs(u32)` w/ `0` sentinel; cbindgen emits `uint32_t`; Rust API exposes `Option<NonZeroU32>` via `OptUs::get()` + `Option<Duration>` ergonomics on the builder.

**Files:**

- `packages/core/nros-node/src/executor/sched_context.rs` — new module: `SchedContext`, `SchedClass`, `DeadlinePolicy`, `SchedContextId`, `OptUs`.
- `packages/core/nros-node/src/executor/ready_set/edf.rs` — `EdfReadySet<N>` (presence bitmap + `heapless::BinaryHeap<(deadline, idx)>`).
- `packages/core/nros-node/src/executor/spin.rs` — `create_sched_context`, `add_*_in(sc_id, ...)` builder methods alongside existing `add_*`.
- `packages/core/nros-c/src/sched_context.rs` — C wrappers: `nros_executor_create_sched_context`, `nros_executor_add_subscription_in`. Cbindgen exports.
- `packages/core/nros-c/include/nano_ros/sched_context.h` — hand-written stub including `nros_generated.h`.
- `packages/core/nros-cpp/include/nano_ros/sched_context.hpp` — C++ wrapper (header-only).
- Tests: per-platform integration tests for EDF dispatch order under contention.

**Acceptance:** EDF callback dispatched before lower-deadline callback when both ready; existing FIFO behavior preserved when `class = Fifo`. `cargo nano-ros generate-cpp` exposes new types correctly.

---

### 110.C — `BucketedFifoSet<N>` + `BucketedEdfSet<N>` (HSE-style)

Adds bucketed ready sets for criticality split on a single executor. Each bucket holds a FIFO bitmap or EDF heap. Selection: bucket asc → within-bucket FIFO/EDF.

**Decisions to lock:**

- Default bucket count = 3 (Critical / Normal / BestEffort) matching scenario S1–S12 vocabulary.
- `BucketedEdfSet<N>` uses N independent EDF heaps (not one heap w/ bucket key) to keep insert/pop O(log n_per_bucket) bounded.
- Single-thread blocking semantics documented as soft-RT only (per § 4.6); 110.D required for hard-RT.

**Files:**

- `packages/core/nros-node/src/executor/ready_set/bucketed.rs` — `BucketedFifoSet<N>`, `BucketedEdfSet<N>`.
- `packages/core/nros-node/src/executor/spin.rs` — `Scheduler::Hybrid { rt_class, be_class }` selector wired to `ExecutorConfig::scheduler`.
- Tests: nros-tests for criticality ordering across buckets w/ overload (BE callback running when Critical activates — verify next-cycle dispatch order, not preemption).

**Acceptance:** Critical-bucket callback runs before BE-bucket callback when both ready. Document non-preemption limitation explicitly in builder docs.

---

### 110.D — `Executor::open_threaded` + `PlatformScheduler` trait

**Mandatory for hard-RT.** Multi-executor: one OS thread per Executor instance, each at its own OS priority. OS preemption handles cross-Executor isolation. Default model still single-thread; multi-thread is opt-in via `Executor::open_threaded`.

`PlatformScheduler` trait extends `nros-platform`:

```rust
pub trait PlatformScheduler {
    fn set_current_thread_policy(p: SchedPolicy) -> Result<(), SchedError>;
    fn yield_now();
    fn set_affinity(cpu_mask: u32) -> Result<(), SchedError>;
}

pub enum SchedPolicy {
    Fifo { os_pri: u8 },
    RoundRobin { os_pri: u8, quantum_ms: u32 },
    Deadline { runtime_ns: u64, period_ns: u64, dl_ns: u64 },  // Linux SCHED_DEADLINE
    Sporadic { budget_us: u32, period_us: u32, hi_pri: u8, lo_pri: u8 },  // NuttX SCHED_SPORADIC
    Platform(PlatformOpaque),  // ThreadX preempt-threshold etc.
}
```

**Per-platform impls:**

- `nros-platform-posix` — `pthread_setschedparam` w/ `SCHED_FIFO/RR/DEADLINE` (Linux) or `SCHED_FIFO/RR/SPORADIC` (NuttX).
- `nros-platform-zephyr` — `k_thread_priority_set`, `k_thread_cpu_pin`. Direction-flipped (lower = higher).
- `nros-platform-freertos` — `vTaskPrioritySet`, `vTaskCoreAffinitySet` (V11+).
- `nros-platform-threadx` — `tx_thread_priority_change`, `tx_thread_preemption_threshold_change`, `tx_thread_smp_core_exclude`. Direction-flipped.
- `nros-platform-bare-metal` — no-op (single-thread); future RTIC integration via NVIC tier promotion.

Direction-flipped priority handled internally; user-facing API uses abstract `Priority::{Critical, Normal, BestEffort}` enum, platform crate translates.

**Files:**

- `packages/core/nros-platform/src/scheduler.rs` — new module: `PlatformScheduler` trait, `SchedPolicy`, `SchedError`.
- `packages/zpico/platform-posix/src/scheduler.rs` — Linux + NuttX impls (NuttX shares POSIX path).
- `packages/zpico/platform-zephyr/src/scheduler.rs`
- `packages/zpico/platform-freertos/src/scheduler.rs`
- `packages/zpico/platform-threadx/src/scheduler.rs`
- `packages/core/nros-node/src/executor/threaded.rs` — new module: `Executor::open_threaded`, `ThreadHandle`, multi-executor lifecycle.
- `packages/core/nros-c/src/threaded.rs` — C wrappers.
- Tests: per-platform integration tests for cross-Executor preemption (drone-style scenario S1).

**Acceptance:** Watchdog scenario S3 passes — Critical Executor preempts BestEffort Executor mid-callback. Drone scenario S1 meets 1 ms deadline under sustained 5 ms BE load.

---

### 110.E — `SchedClass::Sporadic` + budget refill (NuttX-native + user-space)

Server-style budget+period replenishment. NuttX uses kernel `SCHED_SPORADIC` directly (Budget-Micro-ROS RTSS '21 already proved this); other platforms emulate via atomic budget counter + refill timer.

**Decisions to lock:**

- NuttX path: `SCHED_SPORADIC` per executor thread. Set `CONFIG_SCHED_SPORADIC=y`, `CONFIG_SCHED_SPORADIC_MAXREPL=16` (bumped from default 8) in nano-ros NuttX defconfigs.
- Linux path: opt-in `SCHED_DEADLINE` via `ExecutorConfig::os_policy = OsPolicy::Deadline { ... }`. `sched_setattr` syscall (no glibc wrapper).
- Other platforms: user-space refill timer (`xTimerCreate` / `tx_timer_create` / `k_timer` / SysTick ISR) + atomic budget counter. Cortex-M0/M0+ falls back to `critical_section`.

**Files:**

- `packages/core/nros-node/src/executor/sched_context/sporadic.rs` — user-space sporadic server impl (refill heap, budget counter).
- `packages/zpico/platform-posix/src/scheduler.rs` — NuttX `SCHED_SPORADIC` + Linux `SCHED_DEADLINE` syscalls.
- `packages/zpico/platform-zephyr/src/scheduler.rs` — `k_timer`-driven refill.
- `packages/zpico/platform-freertos/src/scheduler.rs` — `xTimerCreate` + `xTimerPendFunctionCall` refill.
- `packages/zpico/platform-threadx/src/scheduler.rs` — `tx_timer_create` + `tx_event_flags_set` refill.
- `packages/zpico/platform-bare-metal/src/scheduler.rs` — SysTick / TIMx ISR refill.
- Tests: budget-overrun integration test (Critical SC w/ 5 ms budget, 10 ms period; verify dispatch suppression after budget exhausted, restoration on refill).

**Acceptance:** S4 Autoware perception scenario meets WCET budget under sustained planner overrun; bandwidth not stolen by misbehaving callback.

---

### 110.F — `OsPrioritySet` (PiCAS-style, opt-in stretch)

Opt-in for users wanting native OS-level callback priorities. Useful only on platforms w/ enough OS pri slots (Linux, NuttX 1..255). Disqualified on Cortex-M0+ (4 NVIC levels) — guarded by feature flag.

**Files:**

- `packages/core/nros-node/src/executor/ready_set/os_priority.rs` — `OsPrioritySet` impl.
- `packages/core/nros-node/Cargo.toml` — `feature = "scheduler-os-priority"`.

**Acceptance:** PiCAS interop reproducible on Linux (Xavier-style platform) per RTAS '21 paper baseline.

#### 110.F.bridge — PiCAS + bridge interaction test (migrated from 104.E.2)

- [ ] `packages/testing/nros-tests/tests/bridge_picas_priority.rs`:
      high-priority sub on backend A + low-priority pub on
      backend B; measure end-to-end priority inheritance
      under the PiCAS dispatcher. Asserts no priority
      inversion. **Blocked on 110.F runtime apply path** —
      `scheduler-os-priority` Cargo feature already gates
      the OsPrioritySet skeleton; needs the per-callback
      OS-priority dispatch wired through the bridge's
      multi-Session Executor (Phase 104.C.3) so each Node's
      callbacks land on their own OS thread.
      **Files:**
      `packages/testing/nros-tests/tests/bridge_picas_priority.rs`.

---

### 110.G — `SchedClass::TimeTriggered` (cyclic executive, stretch)

ARINC-653-style outer time-triggered + inner priority. Major-frame schedule table. For safety-cert paths.

**Files:**

- `packages/core/nros-node/src/executor/sched_context/tt.rs` — TT class impl + schedule table parsing.
- `packages/core/nros-node/src/executor/spin.rs` — TT mode selector.

**Acceptance:** ARINC-653 schedule table demo (one major frame, three slots, deterministic dispatch order across runs).

#### 110.G.bridge — ARINC TT bridge example (migrated from 104.E.4)

- [ ] `examples/native/rust/bridge/tt-zenoh-to-xrce/`:
      time-triggered cyclic bridge with non-overlapping
      ingress/egress windows in a 10 ms major frame.
      Demonstrates `tt_window_offset_us` +
      `tt_window_duration_us` per Node default
      SchedContext. **Blocked on 110.G runtime** —
      `scheduler-time-triggered` Cargo feature already
      gates the TT skeleton; needs major-frame dispatch +
      per-handle TT window enforcement before the bridge
      example can exercise the deterministic
      ingress/egress slot pattern.
      **Files:**
      `examples/native/rust/bridge/tt-zenoh-to-xrce/{Cargo.toml,src/main.rs,README.md}`.

---

## Acceptance Criteria

### v1 (110.0–110.D)

- [ ] `just test-all` green w/ `Scheduler::Fifo` (default) — bit-identical to today.
- [ ] zenoh-pico `next_deadline_ms` keeps drive_io from waking sooner than the lease deadline + tolerance band on a quiet link, verified by counting drive_io return events in a 30 s window (110.0).
- [ ] EDF dispatch order verified under contention (110.B).
- [ ] Bucketed criticality dispatch order verified (110.C).
- [ ] **Drone scenario S1 meets 1 ms deadline** under sustained 5 ms BE-load on Linux + NuttX (110.D required).
- [ ] **Watchdog scenario S3 passes** — multi-executor preemption verified (110.D).
- [ ] No alloc dependency added (no_std + heapless preserved).
- [ ] Cbindgen export builds for all platforms.
- [ ] `nros-cpp` wrappers compile freestanding + std modes.
- [ ] All existing examples build + run unchanged (default `SchedContext` binding).
- [ ] Documentation: book chapter on RT execution model + scenario catalogue.

### Post-v1 (110.E–110.G)

- [ ] `SchedClass::Sporadic` budget enforcement verified on NuttX (native) + Linux (`SCHED_DEADLINE`) + user-space fallback.
- [ ] PiCAS interop reproduced (110.F).
- [ ] PiCAS + cross-backend bridge priority-inheritance test green (110.F.bridge, migrated from 104.E.2).
- [ ] ARINC-653 schedule-table demo (110.G).
- [ ] ARINC TT bridge example deterministic across runs (110.G.bridge, migrated from 104.E.4).

---

## Notes

### Why not adopt PiCAS as the foundation

PiCAS Algorithm 1 burns one OS priority slot per (callback × chain). FreeRTOS = 32 slots, NuttX = 256, ThreadX = 32, **Cortex-M0+ NVIC = 4**. With even moderate chain count × callback count, exhausts the priority space. EDF / CIL-EDF / HSE all avoid this — deadline ordering happens in user-space queue, occupying just **1 OS priority slot per executor thread**.

### Why drop `dyn Transport`

Different transports (zenoh-pico C library, xrce-dds C library, dust-dds Rust crate, uorb byte-shaped session, ros2-rmw vendor lookup) have wildly different shapes. Forcing a common `dyn Trait` adds vtable + alloc-style indirection (we want alloc-free) without buying anything. Instead, `Activator::scan` runs *after* `session.drive_io(timeout)` returns, consulting handle metadata only. Transport stays platform-native; scheduling stays portable.

### Why sentinel-backed `Option` for ABI

`Option<NonZeroU32>` is layout-optimized to `u32` by recent rustc (niche optimization) but **not guaranteed by `#[repr(C)]`** — explicit C repr disables niche opt. So `Option<NonZeroU32>` in a `#[repr(C)]` struct is **not** ABI-compatible with C `uint32_t`.

Solution: `#[repr(transparent)]` newtype `OptUs(u32)` w/ documented `0` sentinel. Cbindgen emits inner `uint32_t`. Rust API exposes `Option<NonZeroU32>` via `OptUs::get()` getter. Both worlds happy. Sentinel value `0` is physically meaningful for time fields (a 0-period would mean infinite frequency; 0-budget means unbounded; 0-deadline means no deadline).

### Single-thread non-preemption is the elephant

Non-preemptive single-thread CANNOT guarantee hard deadlines across SchedClasses. A 50 ms `Fifo` callback WILL block a 1 ms `Edf` deadline. **For hard-RT, 110.D multi-executor is mandatory.** Document loudly. See design doc § 4.6.

### Why split 110.D out from 110.A–C

110.A–C deliver soft-RT EDF + bucketed criticality on single thread. Useful for mobile robots, sensor aggregators, soft-RT pipelines. Many users don't need hard-RT. Splitting 110.D out lets soft-RT users land 110.A–C without paying multi-executor's complexity cost. Hard-RT users layer 110.D on top.

### Coordination with Phase 94 (RTOS Orchestration)

Phase 94's launch-tree codegen will eventually emit per-callback `SchedContext` bindings derived from launch-file hints + `nros.toml` manifests. Today users hand-bind via `add_subscription_in(sc, ...)`; Phase 94 codegen will set this from manifest. No API change needed — Phase 94 just calls the same `_in` methods.

### Coordination with `play_launch` (external, ~/repos/play_launch)

Causal chain discovery happens externally. `play_launch` reads launch files + callback source and emits per-callback `(period, deadline, budget)` hints to `nros.toml`. nano-ros consumes those hints via `SchedContext` bindings. **nano-ros does NOT model chains.**

### Direction-flipped priority handling

ThreadX (0=high), Zephyr-preempt (low=high), Cortex-M NVIC (0=high) vs POSIX/NuttX/FreeRTOS (high=high). User-facing API uses abstract `Priority::{Critical, Normal, BestEffort}` enum; `PlatformScheduler` impl translates to platform-native numeric direction. Already documented in `rtos-orchestration.md` § 10.3.

### Heapless capacity sizing

`Executor<const MAX_HANDLES = 64, const MAX_SC = 8>`. ReadySet capacity = `MAX_HANDLES`. EDF heap capacity = `MAX_HANDLES`. SPSC ring capacity = configurable via `ExecutorConfig::isr_ring_capacity` (default 16). All static; no alloc.

### Compile-time scheduler selection

OSEK lesson: pick at config / build time so MCU builds drop unused scheduler code. Cargo feature gates:

- `feature = "scheduler-fifo"` (default)
- `feature = "scheduler-edf"`
- `feature = "scheduler-bucketed"`
- `feature = "scheduler-sporadic"`
- `feature = "scheduler-os-priority"` (110.F)
- `feature = "scheduler-time-triggered"` (110.G)

Runtime selection only when multiple are compiled in.

### Absorbed from former Phase 105

Former Phase 105 was an earlier attempt at the same problem at the RMW / `drive_io` layer. Phase 110's executor refactor obsoletes most of it:

| Former Phase 105 item | Outcome under Phase 110 |
|-----------------------|---------------------------|
| 105.A `Session::next_deadline_ms` | **Absorbed as 110.0** — RMW trait method that 110.A's `spin_once` consumes when computing `effective_timeout`. |
| 105.A `max_callbacks` cap on `drive_io` | **Subsumed by 110.A** — cap lives in the executor's dispatch loop (`while ready.pop_next()`) via `DrainMode::Latched` (default; drains snapshot only) or an optional `MaxCount(usize)` variant. Doesn't need a `drive_io` parameter. |
| 105.B Timer/GC interleaving INTO `drive_io` | **Obsoleted by 110.A** — 110's `Activator::scan` runs *after* `drive_io` returns and unifies all callback sources (subs, services, timers, GCs) under one ready-set scan. No need to push timer/GC schedulers into the backend. |
| 105.C Wall-clock time budget per `drive_io` | **Subsumed by 110.A** — `ExecutorConfig::cycle_budget_us` enforces wall-clock budget at the executor's dispatch loop, not at `drive_io`. Same primitive, cleaner layering. |

The earlier 105 design was correct in identifying the problems but wrong about which layer should own the solution. Pushing scheduling concerns into the backend's `drive_io` couples every backend to scheduler internals; pulling them up to the executor (110) keeps backends transport-only.
