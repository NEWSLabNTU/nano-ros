# Phase 110 — RT Execution Model (Intra-Executor Scheduling)

**Goal:** Refactor the nano-ros executor to support mixed-criticality callbacks (Critical / Normal / BestEffort) on one executor with pluggable scheduling policies (FIFO / EDF / Sporadic), keeping the user-facing ROS-style API stable across scheduler swaps. Default uses **1 OS priority slot per executor thread** — scheduling decisions live in user space, not in OS priority slots (avoids PiCAS-style slot starvation on platforms like Cortex-M0+ which only has 4 NVIC levels).

**Status:** Not Started

**Priority:** High

**Depends on:** Phase 79 (PlatformYield trait), Phase 76 (config.toml plumbing), Phase 77 (`zpico_spin_once` wake primitive). Coordinates with Phase 94 (RTOS orchestration — emits per-callback `SchedContext` bindings from launch manifests in future) and Phase 88 (nros-log — uses `BestEffort` SC for log sinks).

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

### v1 (Phases 110.A–110.D — required)

- [ ] 110.A — Refactor: `Activator + ReadySet + Dispatcher` + ISR SPSC ring (behavioural no-op)
- [ ] 110.B — `SchedContext` API + `OptUs` newtype + `EdfReadySet`
- [ ] 110.C — `BucketedFifoSet<N>` + `BucketedEdfSet<N>` (HSE-style criticality split)
- [ ] 110.D — `Executor::open_threaded` + `PlatformScheduler` trait per RTOS

### Post-v1 (Phases 110.E–110.G)

- [ ] 110.E — `SchedClass::Sporadic` + budget refill timer (NuttX-native + user-space fallback)
- [ ] 110.F — `OsPrioritySet` (PiCAS-style, opt-in)
- [ ] 110.G — `SchedClass::TimeTriggered` (ARINC-653-style cyclic executive)

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

---

### 110.G — `SchedClass::TimeTriggered` (cyclic executive, stretch)

ARINC-653-style outer time-triggered + inner priority. Major-frame schedule table. For safety-cert paths.

**Files:**

- `packages/core/nros-node/src/executor/sched_context/tt.rs` — TT class impl + schedule table parsing.
- `packages/core/nros-node/src/executor/spin.rs` — TT mode selector.

**Acceptance:** ARINC-653 schedule table demo (one major frame, three slots, deterministic dispatch order across runs).

---

## Acceptance Criteria

### v1 (110.A–110.D)

- [ ] `just test-all` green w/ `Scheduler::Fifo` (default) — bit-identical to today.
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
- [ ] ARINC-653 schedule-table demo (110.G).

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
