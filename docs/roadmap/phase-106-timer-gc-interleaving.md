# Phase 106 — Timer / Guard-condition interleaving inside `drive_io`

**Goal:** unify the dispatch path for all callback sources (subs,
services, clients, timers, guard conditions) inside the backend's
`drive_io` loop so the per-call `max_callbacks` cap from Phase 105
applies uniformly. Today timers and GCs are dispatched *between*
`drive_io` calls; this phase moves them *into* `drive_io` so the cap
covers them too.

**Status:** Not Started.
**Priority:** Low — only useful once Phase 105's
`max_callbacks_per_spin = 1` mode has shipped and apps report that
timer-callback latency is still bottlenecked by sub callbacks
running first inside `drive_io`.
**Depends on:** Phase 105 (`max_callbacks` cap).

## Background

Phase 105's `max_callbacks` cap bounds how many user callbacks
`drive_io` fires per call. The runtime spin loop is:

```text
spin_once(user_timeout):
    drive_io(effective_timeout, cap)    ← fires sub / service callbacks
    process_timers()                     ← fires timer callbacks
    process_guard_conditions()           ← fires GC callbacks
```

The cap covers `drive_io`-dispatched callbacks (subs, services,
clients). It does *not* cover timers or GCs because those are
dispatched outside `drive_io`. With `cap = 1` the worst-case sequence
becomes:

```text
spin_once(100):
    drive_io(100, 1):
        callback_for_sub_a()    ← 8 ms
    process_timers():
        timer_a_callback()       ← 4 ms
        timer_b_callback()       ← 4 ms      ← cap doesn't apply!
        timer_c_callback()       ← 4 ms
        ...
    process_guard_conditions():
        gc_callback()            ← 5 ms
```

If many timers expired during the 8 ms of `callback_for_sub_a`, all
of them fire back-to-back in `process_timers()` regardless of `cap`.
Same for GCs. The cap is non-uniform across callback sources.

## Design

Move timer / GC dispatch *into* `drive_io`'s loop. Backend gets
references to the timer scheduler and guard-condition scheduler;
between I/O slices it consults them and dispatches their callbacks
under the same cap.

Trait change: `drive_io` accepts schedulers as parameters.

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
    /// Time until the next timer expires. `None` if no timers pending.
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
        // 1. Drain any ready I/O, fire one sub/service callback if any.
        if self.poll_one_io_callback()? {
            fired += 1;
            continue;
        }
        // 2. Fire one timer if expired.
        if timers.fire_one() {
            fired += 1;
            continue;
        }
        // 3. Fire one GC if triggered.
        if gcs.fire_one() {
            fired += 1;
            continue;
        }
        // 4. No ready work. Block on backend wait primitive until
        //    one of: (a) I/O arrives, (b) timer's next_deadline_ms
        //    elapses, (c) GC fires (via wake mechanism if backend
        //    has one), (d) overall timeout fires.
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

Now `cap = 1` fires exactly one callback regardless of source — sub,
service, timer, or GC.

## Work Items

- [ ] **106.1 — Define `TimerScheduler` + `GuardConditionScheduler`
      traits.** Object-safe, dyn-friendly, no_std.
      **Files:** `packages/core/nros-rmw/src/traits.rs`.

- [ ] **106.2 — Refactor executor's timer + GC state into
      scheduler-trait impls.**
      **Files:** `packages/core/nros-node/src/executor/timers.rs`,
      `packages/core/nros-node/src/executor/guards.rs`.

- [ ] **106.3 — Update `Session::drive_io` signature to accept
      schedulers.** cffi vtable adds two scheduler-trait function
      pointers (cfi-side: function pointer triples for
      `next_deadline_ms`, `fire_one_timer`, `fire_one_gc`).
      **Files:** `packages/core/nros-rmw/src/traits.rs`,
      `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`.

- [ ] **106.4 — Per-backend rewrite of `drive_io` to interleave.**
      Each backend's drive_io loop gets the alternating
      I/O/timer/GC pattern. Backends without internal scheduling
      can default-dispatch timers and GCs in-loop and only block on
      backend wait when nothing's ready.
      **Files:** `packages/zpico/nros-rmw-zenoh/src/`,
      `packages/dds/nros-rmw-dds/src/`,
      `packages/xrce/nros-rmw-xrce/src/`,
      `packages/px4/nros-rmw-uorb/src/`.

- [ ] **106.5 — Test coverage.**
      `cap = 1` with mixed sources: register one sub + one timer +
      one GC. Trigger them simultaneously. Assert that three
      consecutive `spin_once` calls each fire exactly one callback,
      one of each type.
      **Files:** `packages/testing/nros-tests/tests/`.

- [ ] **106.6 — Book updates.**
      `book/src/design/rmw-vs-upstream.md` Section 4 callback-dispatch
      sub-section: explain the unified-cap behaviour.
      `book/src/concepts/rtos-cooperation.md` updated with the new
      uniformity guarantee.

## Acceptance Criteria

- [ ] `cap = 1` fires exactly one callback per `spin_once` regardless
      of source.
- [ ] No regression on default `cap = usize::MAX` behaviour.
- [ ] No regression on Phase 105 backend `next_deadline_ms` impls.

## Notes

- **Trade-off:** more code per backend. Each backend duplicates the
  alternating-poll loop. Could be factored into a default
  `Session::drive_io_with_schedulers` method that backends call from
  their concrete impl, but factoring tends to reintroduce the
  performance regressions the per-backend impl avoids.
- **Why a separate phase from 105?** 105 alone covers
  preemptive-priority RTOS apps where timer-callback latency is
  bounded by `cap × max_callback_duration`. 106 tightens it further
  to "one callback per spin_once, period." Most apps won't need 106;
  defer until requested.
- **Async / Embassy path unaffected.** `spin_async` doesn't go
  through `drive_io`; it drives futures via wakers. The schedulers
  are spin-loop machinery only.
