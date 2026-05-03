# Phase 105 — `drive_io` Cooperation Surface

**Goal:** Three additions to `Session::drive_io` that let real-time / cyclic apps cooperate with the executor's spin loop, without changing the model. All three target the same backend signature, so they ship as a single phase with sub-phases A/B/C.

- **105.A — `next_deadline_ms()` + per-call `max_callbacks` cap** — backend exposes its next internal-event deadline (lease keepalive, heartbeat, ACK-NACK retransmit) and accepts an upper bound on the number of user callbacks fired per call. Matches upstream `rclcpp`'s "one callback per `spin_once`" pattern.
- **105.B — Timer / Guard-condition interleaving inside `drive_io`** — moves timer + GC dispatch *into* `drive_io`'s loop so the `max_callbacks` cap applies uniformly across all callback sources (subs, services, clients, timers, GCs).
- **105.C — Wall-clock time budget per `drive_io`** — optional `time_budget_ms` so time-triggered cyclic apps (avionics / functional-safety) can budget a fixed CPU slice for ROS work per schedule cycle.

All three default to current behaviour. Apps opt in per RTOS profile. No ABI break for backends; opt-in for the runtime.

**Status:** Not Started

**Priority:** Medium (105.A) → Low (105.B, 105.C)

**Depends on:** Phase 102 (typed entity structs) for sub-phase A. Sub-phase B depends on A; sub-phase C depends on B. Coordinates with Phase 110.A (executor refactor — Phase 110's `Activator + ReadySet + Dispatcher` consumes this surface).

---

## Background

The current `Session::drive_io(timeout_ms)` drains all ready I/O and fires every queued user callback before returning. This works for:

- **Cooperative single-task** apps (one task does ROS, no priority competition).
- **Asynchronous Embassy / tokio** apps (callbacks are wakers; no spin loop in the traditional sense).

It is a footgun for:

- **Preemptive priority RTOS** apps (FreeRTOS / ThreadX / Zephyr) where the ROS task runs at a fixed priority and ROS-internal entities (timers, subs, services, GCs) share that priority. A long subscriber callback delays a timer expiry that should have fired earlier — they can't preempt one another.
- **Time-triggered cyclic** apps (avionics / functional-safety) that budget a fixed CPU slice for ROS per cycle. `drive_io` cannot respect that budget today.

Three small tweaks fix the gap without rewriting the model.

---

## Design

### 105.A — `next_deadline_ms` + `max_callbacks`

#### `Session::next_deadline_ms`

The backend internally schedules events: zenoh-pico's lease keepalive, XRCE-DDS's session ping, dust-DDS's heartbeats. Today the executor calls `drive_io(user_timeout)` blind to these — the backend may return *much* sooner than `user_timeout` because of an internal deadline. Wasted round-trip on quiet links.

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
| dust-DDS | Per-writer heartbeat period; per-reader ACK-NACK timeout; participant liveliness lease | Implement `DdsRuntime::next_event_time` returning min over entities. ~30 LOC. |
| XRCE-DDS | Heartbeat to agent; session ping | Mirror in shim: `last_run + heartbeat_period - now()`. ~15 LOC. |
| uORB | None — intra-process, no keepalives | Keep default `None`. 0 LOC. |

#### `Session::drive_io` cap

Today `drive_io` fires every queued callback before returning. For preemptive-priority RTOS this means a 64 ms drive_io call can leave a timer 60 ms late.

`rclcpp`'s single-threaded executor runs **exactly one callback per `spin_once`** (`Executor::execute_any_executable` in `rclcpp/src/rclcpp/executor.cpp`). Per-callback scheduling opportunity is the unit; the spin loop is the rate.

```rust
pub trait Session {
    /// Drive backend I/O for up to `timeout_ms`. Fires at most
    /// `max_callbacks` user callbacks before returning. Returns
    /// `DriveStats` describing what happened.
    ///
    /// `max_callbacks = usize::MAX` is the historical "drain all"
    /// behaviour. `max_callbacks = 1` matches `rclcpp`'s
    /// per-`spin_once` dispatch pattern.
    fn drive_io(
        &mut self,
        timeout_ms: u32,
        max_callbacks: usize,
    ) -> Result<DriveStats, Self::Error>;
}

pub struct DriveStats {
    pub callbacks_fired: usize,
    /// Backend has more ready data that wasn't dispatched (cap
    /// exceeded). Caller should call `drive_io` again with
    /// `timeout_ms = 0` to drain.
    pub more_pending: bool,
}
```

C vtable:

```c
typedef struct nros_rmw_drive_stats_t {
    uint32_t callbacks_fired;
    bool     more_pending;
    uint8_t  _reserved[3];
} nros_rmw_drive_stats_t;

typedef struct nros_rmw_vtable_t {
    /* … */
    nros_rmw_ret_t (*drive_io)(
        nros_rmw_session_t *session,
        int32_t  timeout_ms,
        size_t   max_callbacks,
        nros_rmw_drive_stats_t *stats_out);
} nros_rmw_vtable_t;
```

#### Executor configuration

```rust
pub struct ExecutorConfig {
    /* existing fields … */
    /// Maximum user callbacks per `spin_once` iteration. Defaults
    /// to `usize::MAX` (drain all per call). Set to `1` for
    /// upstream-style per-callback scheduling on preemptive-priority
    /// RTOS targets.
    pub max_callbacks_per_spin: usize,
}
```

`spin_once`:

```rust
pub fn spin_once(&mut self, user_timeout: u32) -> Result<()> {
    let next_timer = self.timers.next_deadline_ms();
    let next_session = self.session.next_deadline_ms();
    let effective_timeout = [Some(user_timeout), next_timer, next_session]
        .into_iter().flatten().min().unwrap();

    let stats = self.session.drive_io(
        effective_timeout,
        self.config.max_callbacks_per_spin,
    )?;

    self.process_timers();
    self.process_guard_conditions();
    Ok(())
}
```

### 105.B — Timer / Guard-condition interleaving inside `drive_io`

Phase 105.A's `max_callbacks` cap covers `drive_io`-dispatched callbacks (subs, services, clients). It does *not* cover timers or GCs because those are dispatched outside `drive_io`. With `cap = 1` the worst-case sequence is:

```text
spin_once(100):
    drive_io(100, 1):
        callback_for_sub_a()    ← 8 ms
    process_timers():
        timer_a_callback()       ← 4 ms
        timer_b_callback()       ← 4 ms      ← cap doesn't apply!
        timer_c_callback()       ← 4 ms
    process_guard_conditions():
        gc_callback()            ← 5 ms
```

Move timer / GC dispatch *into* `drive_io`'s loop. Backend gets references to the timer scheduler and guard-condition scheduler; between I/O slices it consults them and dispatches their callbacks under the same cap.

```rust
pub trait Session {
    fn drive_io(
        &mut self,
        timeout_ms: u32,
        max_callbacks: usize,
        timers: &mut dyn TimerScheduler,
        gcs: &mut dyn GuardConditionScheduler,
    ) -> Result<DriveStats, Self::Error>;
}

pub trait TimerScheduler {
    fn next_deadline_ms(&self) -> Option<u32>;
    /// Fire one ready timer's callback. Returns `true` if a timer fired.
    fn fire_one(&mut self) -> bool;
}

pub trait GuardConditionScheduler {
    /// Drain one ready GC. Returns `true` if a GC fired.
    fn fire_one(&mut self) -> bool;
}
```

Backend's drive_io loop:

```rust
fn drive_io(&mut self, timeout, cap, timers, gcs) -> Result<DriveStats> {
    let mut fired = 0;
    let deadline = now() + timeout;

    while fired < cap && now() < deadline {
        if self.poll_one_io_callback()? { fired += 1; continue; }
        if timers.fire_one()             { fired += 1; continue; }
        if gcs.fire_one()                { fired += 1; continue; }

        let block_for = (deadline - now()).min(
            timers.next_deadline_ms().unwrap_or(u32::MAX)
        );
        self.block_for(block_for)?;
    }

    Ok(DriveStats {
        callbacks_fired: fired,
        more_pending: self.has_pending_work(),
    })
}
```

`cap = 1` now fires exactly one callback regardless of source — sub, service, timer, or GC.

### 105.C — Wall-clock time budget

Phase 105.A's `max_callbacks` cap bounds *count*; per-callback WCET varies. Time-triggered cyclic apps need a wall-clock budget instead.

```rust
pub trait Session {
    fn drive_io(
        &mut self,
        timeout_ms: u32,
        max_callbacks: usize,
        time_budget_ms: Option<u32>,         // 105.C addition
        timers: &mut dyn TimerScheduler,
        gcs: &mut dyn GuardConditionScheduler,
    ) -> Result<DriveStats, Self::Error>;
}
```

`None` = no budget check, current behaviour. `Some(ms)` = check elapsed wall-clock after each fired callback; return when exceeded even if `max_callbacks` not reached.

Backend's drive_io loop adds the check:

```rust
let start = clock_ms();
while fired < cap && now < deadline {
    if let Some(budget) = time_budget_ms {
        if clock_ms() - start >= budget { break; }
    }
    /* fire one callback (sub / service / timer / gc) */
    fired += 1;
}
```

Cost: one clock read per iteration. ARM has DWT cycle counter (constant-time). ESP32 has `esp_timer_get_time`. POSIX has `clock_gettime(CLOCK_MONOTONIC)`. All fast enough.

C vtable: `time_budget_ms = 0` sentinel for "no budget" (consistent with Phase 110's `OptUs` ABI pattern).

`ExecutorConfig::time_budget_per_spin_ms: Option<u32>` — default `None`.

---

## Work Items

### v1 — 105.A (`next_deadline_ms` + `max_callbacks`)

- [ ] **105.A.1 — Add `Session::next_deadline_ms` trait method.** Default impl returns `None`. `nros-rmw-cffi` adds the optional C vtable function pointer (NULL = use full timeout).
  **Files:** `packages/core/nros-rmw/src/traits.rs`, `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`, `packages/core/nros-rmw-cffi/src/lib.rs`.
- [ ] **105.A.2 — Backend overrides for `next_deadline_ms`.** zenoh-pico tracks lease deadline; dust-DDS computes min over writer heartbeat / liveliness; XRCE mirrors heartbeat schedule. uORB stays at default `None`.
  **Files:** `packages/zpico/nros-rmw-zenoh/src/`, `packages/dds/nros-rmw-dds/src/`, `packages/xrce/nros-rmw-xrce/src/`.
- [ ] **105.A.3 — Add `max_callbacks` parameter to `drive_io`.** Trait signature change + new `DriveStats` return type. cffi vtable signature change. Backend impls thread the count through their RX loop and short-circuit when cap is reached.
  **Files:** `packages/core/nros-rmw/src/traits.rs`, `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`, `packages/core/nros-rmw-cffi/src/lib.rs`, every backend.
- [ ] **105.A.4 — Executor wires it through.** `ExecutorConfig::max_callbacks_per_spin` default `usize::MAX`. `spin_once` reads `next_deadline_ms` to cap timeout, threads `max_callbacks_per_spin` into `drive_io`, loops on `more_pending` for the same iteration before checking timers.
  **Files:** `packages/core/nros-node/src/executor/mod.rs`, `packages/core/nros-node/src/executor/config.rs`.
- [ ] **105.A.5 — Test coverage.** Per-backend test: `max_callbacks_per_spin = 1`, publish 10 messages, spin in loop until all delivered; assert each `spin_once` fired exactly one callback. Per-backend test: subscribe to nothing, drive_io with long timeout, assert `next_deadline_ms` caps the wait to keepalive interval.
  **Files:** `packages/testing/nros-tests/tests/`.
- [ ] **105.A.6 — Book + Doxygen.** `book/src/design/rmw-vs-upstream.md` Section 4 elaborated with callback-dispatch timing model and the `max_callbacks` knob. New page `book/src/concepts/rtos-cooperation.md`. Doxygen on the new vtable fields.

### Post-v1 — 105.B (Timer/GC interleaving)

- [ ] **105.B.1 — Define `TimerScheduler` + `GuardConditionScheduler` traits.** Object-safe, dyn-friendly, no_std.
  **Files:** `packages/core/nros-rmw/src/traits.rs`.
- [ ] **105.B.2 — Refactor executor's timer + GC state into scheduler-trait impls.**
  **Files:** `packages/core/nros-node/src/executor/timers.rs`, `packages/core/nros-node/src/executor/guards.rs`.
- [ ] **105.B.3 — Update `Session::drive_io` signature to accept schedulers.** cffi vtable adds two scheduler-trait function pointers (cffi-side: function pointer triples for `next_deadline_ms`, `fire_one_timer`, `fire_one_gc`).
  **Files:** `packages/core/nros-rmw/src/traits.rs`, `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`, `packages/core/nros-rmw-cffi/src/lib.rs`.
- [ ] **105.B.4 — Per-backend rewrite of `drive_io` to interleave.** Each backend's drive_io loop gets the alternating I/O/timer/GC pattern.
  **Files:** every backend.
- [ ] **105.B.5 — Test coverage.** `cap = 1` with mixed sources (one sub + one timer + one GC, all triggered simultaneously). Three consecutive `spin_once` calls each fire exactly one callback, one of each type.
  **Files:** `packages/testing/nros-tests/tests/`.
- [ ] **105.B.6 — Book updates.** `book/src/concepts/rtos-cooperation.md` updated with the unified-cap behaviour.

### Post-v1 — 105.C (Wall-clock budget)

- [ ] **105.C.1 — Add `time_budget_ms` parameter to `Session::drive_io`.** Optional in Rust trait (`Option<u32>`), use `0` sentinel in C vtable for "no budget" (matches Phase 110 `OptUs` convention).
  **Files:** `packages/core/nros-rmw/src/traits.rs`, `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`, `packages/core/nros-rmw-cffi/src/lib.rs`.
- [ ] **105.C.2 — Backend impls of the time check.** Per-backend `clock_ms()` source via `nros-platform-api::PlatformClock`.
  **Files:** all four backends.
- [ ] **105.C.3 — `ExecutorConfig::time_budget_per_spin_ms`.** Optional. Default `None`.
  **Files:** `packages/core/nros-node/src/executor/config.rs`.
- [ ] **105.C.4 — Test coverage.** `time_budget_per_spin_ms = 5`, slow-callback subscription (artificial 3 ms work), publish 10 messages. Two `spin_once` calls each fire one callback (1×3 ms < 5 ms; 2×3 ms > 5 ms). Drains over multiple iterations.
  **Files:** `packages/testing/nros-tests/tests/`.
- [ ] **105.C.5 — Book updates.** `book/src/concepts/rtos-cooperation.md` time-triggered cyclic profile + time-budget configuration.

---

## Acceptance Criteria

### v1 (105.A)

- [ ] `cargo build -p nros-rmw -p nros-rmw-cffi -p nros-node -p nros` clean.
- [ ] Default behaviour (`max_callbacks_per_spin = usize::MAX`) identical to current — no regression on any existing test.
- [ ] zenoh-pico `next_deadline_ms` keeps drive_io from waking sooner than the lease deadline + tolerance band on a quiet link (counted drive_io return events in 30 s window).
- [ ] `max_callbacks_per_spin = 1` test fires exactly one callback per `spin_once`.
- [ ] Book + Doxygen build clean.

### Post-v1 (105.B + 105.C)

- [ ] `cap = 1` fires exactly one callback per `spin_once` regardless of source (sub / service / timer / GC).
- [ ] No regression on default `cap = usize::MAX` behaviour.
- [ ] `time_budget = Some(5)` test demonstrates `spin_once` respects the budget across multiple iterations.
- [ ] No measurable overhead on the `time_budget = None` path.

---

## Notes

- **Why bundle three sub-phases?** All three modify `drive_io`'s call site (the executor's spin loop) and all three touch the cffi vtable. Bundling amortises the ABI surface change. Phase 102.7 was skipped for the same reason — cffi is pre-publish, no version bump needed.
- **Why is 105.B post-v1?** 105.A alone covers preemptive-priority RTOS apps where timer-callback latency is bounded by `cap × max_callback_duration`. 105.B tightens to "one callback per spin_once, period." Most apps won't need it; defer until requested.
- **Why is 105.C post-v1?** Driven only by use cases like DO-178C-flavoured deployments that allocate a fixed time slice per cycle. None currently in tree.
- **Default behaviour is "drain all."** Existing apps see no change. RTOS apps that need per-callback scheduling set `max_callbacks_per_spin = 1`. Time-triggered apps set `time_budget_per_spin_ms = Some(N)`.
- **Async / Embassy path unaffected.** `spin_async` doesn't go through `drive_io`; it drives futures via wakers.
- **Coordination with Phase 110.** Phase 110.A's `Activator + ReadySet + Dispatcher` refactor consumes the `drive_io` surface defined here. Phase 110.A SHOULD land *after* 105.A (so executor refactor is built atop the cooperation surface), but 105.B + 105.C can land independently of 110.
- **`time_budget = Some(0)` means no budget** (sentinel), to match Phase 110's `OptUs` ABI convention. Wall-clock budget = 0 is meaningless physically (would cap at zero callbacks); using 0 as "absent" sentinel is consistent.
- **What if a single callback exceeds the budget?** Budget is checked *after* the callback. A 10 ms callback on a 5 ms budget still runs to completion; drive_io returns afterwards. Apps that need preempt-during-callback need a different mechanism (Phase 110.D multi-executor).
