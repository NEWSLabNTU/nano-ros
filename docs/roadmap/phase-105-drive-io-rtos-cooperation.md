# Phase 105 — `drive_io` RTOS cooperation: deadline visibility + per-call callback cap

**Goal:** two small additions to the `Session::drive_io` interface that
let real-time RTOS apps cooperate with the executor:

1. **`next_deadline_ms()`** — backend exposes its next internal-event
   deadline (lease keepalive, heartbeat, ACK-NACK retransmit) so the
   executor caps `drive_io`'s timeout against it.
2. **`max_callbacks`** — `drive_io` accepts an upper bound on the
   number of user callbacks fired per call, giving the executor
   per-callback scheduling control matching upstream `rclcpp`'s
   "one callback per `spin_once`" pattern.

Both default to current behaviour. Apps opt in per their RTOS
profile. No ABI break for backends; opt-in for the runtime.

**Status:** Not Started.
**Priority:** Medium. Closes a real footgun for preemptive-priority
RTOS deployments where ROS-internal entities all share a task
priority and timer dispatch can be delayed by long subscriber-callback
chains.
**Depends on:** Phase 102 (typed entity structs).

## Background

The current `Session::drive_io(timeout_ms)` drains all ready I/O and
fires every queued user callback before returning. This works for:

- **Cooperative single-task** apps (one task does ROS, no priority
  competition).
- **Asynchronous Embassy / tokio** apps (callbacks are wakers; no
  spin loop in the traditional sense).

It is a footgun for:

- **Preemptive priority RTOS** apps (FreeRTOS / ThreadX / Zephyr) where
  the ROS task runs at a fixed priority and ROS-internal entities
  (timers, subs, services, GCs) share that priority. A long subscriber
  callback delays a timer expiry that should have fired earlier — they
  can't preempt one another.
- **Time-triggered cyclic** apps (avionics / functional-safety) that
  budget a fixed CPU slice for ROS per cycle. `drive_io` cannot
  respect that budget today.

Two independent tweaks fix the gap without rewriting the model.

### Tweak 1 — `next_deadline_ms()`

The backend internally schedules events: zenoh-pico's lease keepalive,
XRCE-DDS's session ping, dust-DDS's heartbeats. Today the executor
calls `drive_io(user_timeout)` blind to these — the backend may
return *much* sooner than `user_timeout` because of an internal
deadline. The executor wakes, sees no user-visible work, calls
`drive_io` again with a fresh timeout. Wasted round-trip on quiet
links.

The fix: a trait getter the backend optionally implements. The
executor caps the `drive_io` timeout against it.

### Tweak 2 — `max_callbacks`

Today `drive_io` fires every queued callback before returning. For
preemptive-priority RTOS this means a 64 ms drive_io call can leave a
timer 60 ms late.

`rclcpp`'s single-threaded executor runs **exactly one callback per
`spin_once`** — see `Executor::execute_any_executable` in
`rclcpp/src/rclcpp/executor.cpp`. Per-callback scheduling
opportunity is the unit; the spin loop is the rate.

The fix: `drive_io` accepts a cap on callbacks fired per call. The
backend respects it. The runtime spin loop calls `drive_io` again to
process the rest.

## Design

### `Session::next_deadline_ms`

```rust
pub trait Session {
    /* existing methods … */

    /// Backend's next internal-event deadline (keepalive, heartbeat,
    /// lease expiry). The runtime caps its `drive_io` timeout
    /// against `min(user_timeout, timer_deadline, this)`. Returns
    /// `None` if the backend has no internal deadlines or chooses
    /// not to expose them.
    fn next_deadline_ms(&self) -> Option<u32> { None }
}
```

C side — optional vtable function pointer:

```c
typedef struct nros_rmw_vtable_t {
    /* … */
    /** Optional. NULL = backend has no internal deadlines. Returns
     *  ms-until-next-event; 0 means the backend wants to be driven
     *  immediately. */
    int32_t (*next_deadline_ms)(const nros_rmw_session_t *session);
} nros_rmw_vtable_t;
```

NULL function pointer = "no deadline." The runtime treats it as
`None`.

#### Backend opt-in matrix

| Backend | Deadline source | Plan |
|---------|-----------------|------|
| zenoh-pico | Lease keepalive interval | Track `last_keepalive_sent + LEASE_INTERVAL` in shim. ~15 LOC. |
| dust-DDS | Per-writer heartbeat period; per-reader ACK-NACK timeout; participant liveliness lease | Implement `DdsRuntime::next_event_time` returning min over entities. ~30 LOC. |
| XRCE-DDS | Heartbeat to agent; session ping | Mirror in shim: `last_run + heartbeat_period - now()`. ~15 LOC. |
| uORB | None — intra-process, no keepalives | Keep default `None`. 0 LOC. |

### `Session::drive_io` cap

Replace the existing single-arg form:

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
    /// Number of user callbacks fired this call.
    pub callbacks_fired: usize,
    /// Backend has more ready data that wasn't dispatched (cap
    /// exceeded). Caller should call `drive_io` again with
    /// `timeout_ms = 0` to drain.
    pub more_pending: bool,
}
```

C vtable side:

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

`max_callbacks = SIZE_MAX` is current behaviour. Backends count
fired callbacks, return when cap is reached even if more is ready.
The `more_pending` flag tells the executor to spin back without
sleeping.

### Executor configuration

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

Executor's `spin_once`:

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

`spin_period(period)` loops `spin_once` until period elapses. With
`max_callbacks_per_spin = 1`, ten ready messages cause ten loop
iterations — one callback each, with timer/GC checks between.

## Work Items

- [ ] **105.1 — Add `Session::next_deadline_ms` trait method.**
      Default impl returns `None`. `nros-rmw-cffi` adds the optional
      C vtable function pointer (NULL = use full timeout).
      **Files:** `packages/core/nros-rmw/src/traits.rs`,
      `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`.

- [ ] **105.2 — Backend overrides for `next_deadline_ms`.**
      zenoh-pico tracks lease deadline; dust-DDS computes min over
      writer heartbeat / liveliness; XRCE mirrors heartbeat schedule.
      uORB stays at default `None`.
      **Files:** `packages/zpico/nros-rmw-zenoh/src/`,
      `packages/dds/nros-rmw-dds/src/`,
      `packages/xrce/nros-rmw-xrce/src/`.

- [ ] **105.3 — Add `max_callbacks` parameter to `drive_io`.**
      Trait signature change + new `DriveStats` return type. cffi
      vtable signature change. Backend impls thread the count
      through their RX loop and short-circuit when cap is reached.
      **Files:** `packages/core/nros-rmw/src/traits.rs`,
      `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      every backend.

- [ ] **105.4 — Executor wires it through.**
      `ExecutorConfig::max_callbacks_per_spin` default `usize::MAX`.
      `spin_once` reads `next_deadline_ms` to cap timeout, threads
      `max_callbacks_per_spin` into `drive_io`, loops on
      `more_pending` for the same iteration before checking timers.
      **Files:** `packages/core/nros-node/src/executor/mod.rs`,
      `packages/core/nros-node/src/executor/config.rs`.

- [ ] **105.5 — Test coverage.**
      Per-backend test: configure `max_callbacks_per_spin = 1`,
      publish 10 messages, spin in a loop until all 10 delivered;
      assert each `spin_once` fired exactly one callback. Per-backend
      test: subscribe to nothing, drive_io with long timeout, assert
      `next_deadline_ms` caps the wait to the keepalive interval.
      **Files:** `packages/testing/nros-tests/tests/`.

- [ ] **105.6 — Book + Doxygen updates.**
      `book/src/design/rmw-vs-upstream.md` Section 4 elaborated with
      callback-dispatch timing model and the `max_callbacks` knob.
      New page `book/src/concepts/rtos-cooperation.md` matching
      drive_io configuration to RTOS execution profiles. Doxygen on
      the new vtable fields.

## Acceptance Criteria

- [ ] `cargo build -p nros-rmw -p nros-rmw-cffi -p nros-node -p nros`
      clean.
- [ ] Default behaviour (`max_callbacks_per_spin = usize::MAX`)
      identical to current — no regression on any existing test.
- [ ] zenoh-pico `next_deadline_ms` keeps drive_io from waking sooner
      than the lease deadline + tolerance band on a quiet link
      (verified by counting drive_io return events in a 30 s window).
- [ ] `max_callbacks_per_spin = 1` test fires exactly one callback per
      spin_once.
- [ ] Book + Doxygen build clean.

## Notes

- **Why bundle the two tweaks?** Both modify `drive_io`'s call site
  (the executor's spin loop) and both touch the cffi vtable. Bundling
  amortises the ABI surface change. Phase 102.7 was skipped because
  cffi is pre-publish; same applies here — no version bump needed.
- **Why not move timer / GC dispatch into `drive_io`?** That's a
  bigger architectural change tracked separately as Phase 106
  (timer / GC interleaving). It's only worth doing once 105's
  `max_callbacks` knob shows up insufficient in practice.
- **`time_budget_ms` deferred to Phase 107.** Wall-clock budgeting per
  `drive_io` call needs fast clock reads; only useful for
  time-triggered apps that haven't shown up yet.
- **Default behaviour is "drain all."** Existing apps see no change.
  RTOS apps that need per-callback scheduling set `1`. Async / Embassy
  apps using `spin_async` aren't affected (they go through Future /
  Waker, not the cap).
