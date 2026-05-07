# Phase 110.E.b — `PlatformTimer` + `AtomicSporadicState` design

**Status:** Design locked, impl deferred.
**Predecessors:** Phase 110.E v1 commits `91a94cf2` (kernel syscall paths) +
`50d2e7d3` (polled-clock SporadicState).
**Goal:** Replace the polled-clock `SporadicState` with an ISR-driven
refill model that works on every platform, not just `feature = "std"`.

---

## Problem

The Phase 110.E v1 `SporadicState` (commit `50d2e7d3`) refills budget
by polling `Instant::now()` from `Executor::spin_once`. Two limits:

1. **`feature = "std"` only.** No-std builds (FreeRTOS / Zephyr /
   ThreadX / bare-metal) get no refill — Sporadic-class SCs there
   never recover budget after exhaustion.
2. **Polled, not ISR-driven.** Cycle-level `delta_us` attribution is
   coarse — a cycle that runs 5 ms of FIFO work charges that 5 ms
   against every Sporadic SC even if none of them dispatched.
   Acceptable as a worst-case bandwidth limiter; not acceptable for
   per-callback runtime accounting (the actual phase-doc 110.E
   acceptance shape).

Direct ISR access from Rust safe code is impossible without atomics +
a callback-thunking trait surface — exactly what `PlatformTimer`
provides.

## Design

### `PlatformTimer` trait (lives in `nros-platform-api`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    Unsupported,
    OutOfRange,
    KernelError,
}

pub trait PlatformTimer {
    /// Opaque per-platform handle (FreeRTOS `TimerHandle_t`,
    /// Zephyr `*mut k_timer`, ThreadX `*mut TX_TIMER`,
    /// POSIX `timer_t`).
    type TimerHandle: Send + Sync + 'static;

    /// Register a periodic timer that fires `callback(user_data)`
    /// every `period_us` microseconds. Returns the platform-native
    /// handle for later destruction.
    ///
    /// Callback executes in the platform's "timer context" — direct
    /// ISR on Zephyr / bare-metal, deferred via
    /// `xTimerPendFunctionCall` on FreeRTOS, signal handler on POSIX.
    /// The callback contract is the lowest common denominator: a
    /// `extern "C" fn(*mut c_void)` so platforms with different
    /// native callback signatures can wrap behind a per-platform
    /// thunk.
    fn create_periodic(
        period_us: u32,
        callback: extern "C" fn(*mut c_void),
        user_data: *mut c_void,
    ) -> Result<Self::TimerHandle, TimerError>;

    /// Cancel + free the timer. Idempotent on already-destroyed
    /// handles.
    fn destroy(handle: Self::TimerHandle);
}
```

Default impl on the trait returns `Unsupported` for `create_periodic`
and is a no-op for `destroy` so platforms without a timer surface
inherit safe behavior.

### `AtomicSporadicState`

Replaces the current plain-struct `SporadicState`:

```rust
pub struct AtomicSporadicState {
    pub budget_remaining_us: AtomicU32,
    pub last_refill_ms: AtomicU64,
    pub budget_capacity_us: u32,  // immutable post-init
    pub period_us: u32,           // immutable post-init
}
```

**Refill thunk** (the callback `PlatformTimer::create_periodic`
invokes):

```rust
extern "C" fn refill_thunk(user_data: *mut c_void) {
    // SAFETY: the caller of `create_sched_context` registers `state`
    // and keeps it alive for the lifetime of the timer.
    let state = unsafe { &*(user_data as *const AtomicSporadicState) };
    state.budget_remaining_us.store(
        state.budget_capacity_us,
        Ordering::Release,
    );
}
```

**Dispatch read** (called from `spin_once`, no `&mut self` needed):

```rust
fn has_budget(&self) -> bool {
    self.budget_remaining_us.load(Ordering::Acquire) > 0
}
```

**Per-callback consumption** (post-dispatch in `spin_once`):

```rust
let consumed = (callback_end - callback_start).as_micros() as u32;
state.budget_remaining_us.fetch_sub(consumed, Ordering::Release);
```

(`fetch_sub` saturates with `compare_exchange_loop` to avoid
underflow.)

### Executor side

```rust
pub struct Executor {
    // ... existing fields ...
    pub(crate) sporadic_states:
        [Option<(Arc<AtomicSporadicState>, OpaqueTimerHandle)>; MAX_SC],
}
```

`create_sched_context` for a `Sporadic`-class SC:

```rust
let state = Arc::new(AtomicSporadicState::new(budget_us, period_us));
let handle = P::create_periodic(
    period_us,
    refill_thunk,
    Arc::as_ptr(&state) as *mut c_void,
)?;
self.sporadic_states[i] = Some((state, handle));
```

`Drop for Executor` walks `sporadic_states` and calls
`P::destroy(handle)`.

### `Executor` stays non-generic

Phase 110.D.a's decision (don't make `Executor` generic over Platform)
is preserved by **opaque-boxing** the `TimerHandle`:

```rust
pub struct OpaqueTimerHandle {
    handle: NonNull<c_void>,
    destroy_fn: extern "C" fn(*mut c_void),
}
```

`PlatformTimer::create_periodic` returns the platform-specific handle;
nros-node wraps it in `OpaqueTimerHandle` with the platform's
`destroy` thunk. Drop calls `(self.destroy_fn)(self.handle.as_ptr())`.

The `apply_policy: fn(SchedPolicy)` pattern from `open_threaded` is
the model — caller provides the platform glue, executor stays
platform-agnostic.

## Per-platform implementations

| Platform | Native API | Thunk pattern |
|---|---|---|
| **POSIX** | `timer_create(CLOCK_MONOTONIC, &sigevent, &timer)` + `timer_settime` | `sigevent.sigev_notify = SIGEV_THREAD`; `sigev_notify_function = thunk`; `sigev_value.sival_ptr = user_data` |
| **FreeRTOS** | `xTimerCreate(name, period_ticks, pdTRUE, id=user_data, callback)` | `pvTimerGetTimerID(timer)` → `user_data` inside callback. `xTimerPendFunctionCall` only needed when callback work is heavy; refill is one atomic store, fits in timer-service-task |
| **Zephyr** | `k_timer_init(timer, expiry_fn, NULL)` + `k_timer_user_data_set` + `k_timer_start(timer, period, period)` | `k_timer_user_data_get(timer)` → `user_data` inside `expiry_fn`. Direct ISR call; thunk does atomic store + returns |
| **ThreadX** | `tx_timer_create(timer, name, expiration_fn, expiration_input, init_ticks, reschedule_ticks, TX_AUTO_ACTIVATE)` | `expiration_input` carries `user_data`. ThreadX expiration runs in timer-thread context |
| **Bare-metal** | Board-specific (SysTick / TIMx ISR) | Board crate exposes `SysTickHook::register(cb, user_data)` — wired alongside per-board PlatformScheduler stub work |
| **NuttX** | `timer_create(CLOCK_MONOTONIC, ...)` POSIX | Same shape as POSIX impl |
| **cffi** | C vtable extension | Add `vtable.create_periodic_timer` + `vtable.destroy_timer` |

## Cancel / restart (deferred)

Per-callback runtime accounting needs:
- Oneshot timer fires `budget_us` after dispatch start
- If callback returns first → cancel + arm refill timer for next period
- If oneshot fires first → callback ran past budget → suppress + log

That requires `cancel(handle)` + `restart_oneshot(handle, us)` on
`PlatformTimer`. Out of scope for the v1 trait; lands with the per-
callback measurement work.

## Migration path

1. **Land trait** (`PlatformTimer` + `TimerError` in
   nros-platform-api). Default impls all `Unsupported`. ~50 LOC.
2. **Atomic-fy `SporadicState`** → `AtomicSporadicState`. Update
   `tick`-based polling path to use atomic ops; keeps backward compat
   on std builds. ~80 LOC.
3. **POSIX `PlatformTimer` impl** as the reference. ~120 LOC. Linux-
   first since `timer_create` is portable across glibc / musl.
4. **Wire `create_sched_context`** to call
   `P::create_periodic` on Sporadic-class SCs; thread `OpaqueTimerHandle`
   storage onto `Executor`. ~80 LOC.
5. **FreeRTOS impl** — first RTOS, exercises `pvTimerGetTimerID`
   id-trick. ~120 LOC.
6. **Zephyr impl** — direct ISR thunk. ~80 LOC.
7. **ThreadX impl** — `tx_timer_create` + expiration_input. ~80 LOC.
8. **NuttX impl** — POSIX-share path. ~10 LOC.
9. **Bare-metal** — opens up `SysTickHook` board-crate work. Per-board.
10. **Per-callback runtime** — adds `cancel` / `restart_oneshot`
    methods on `PlatformTimer`. Plumbs measurement into `spin_once`
    around the per-callback `try_process` calls.

Total without bare-metal + per-callback runtime: ~620 LOC across
5 crates. Estimate 3-5 dedicated sessions.

## Open questions

1. **Send-ness of `TimerHandle`.** POSIX `timer_t` is `Send`. FreeRTOS
   `TimerHandle_t` is `Send` when `INCLUDE_xTimerGetTimerDaemonTaskHandle`
   is on. Zephyr `*mut k_timer` requires the storage to outlive the
   timer — need static or `Arc<UnsafeCell<k_timer>>` pattern.
2. **Drop ordering.** `Executor::Drop` must destroy timers *before*
   the `Arc<AtomicSporadicState>`s that they reference are dropped,
   else the timer's last refill could fire after the state is freed.
   Trait `destroy` should be synchronous with timer-callback drain.
3. **`Arc` on no-std.** Requires `alloc`. nros-node already feature-
   gates `alloc`; carry that forward. Bare-metal w/o alloc would need
   a `'static AtomicSporadicState` lifetime contract instead of
   `Arc` — extends the API but doesn't change the trait shape.
4. **Multiple SCs sharing one timer.** Each Sporadic SC currently gets
   its own `PlatformTimer`; if MAX_SC × Sporadic count is large
   (rare), share one tick timer + per-SC accounting. Optimization,
   not v1.

## Acceptance for the full landing

- All 5 RTOSes' `PlatformTimer` impls build clean
- Sporadic-class SC budget refill works without `feature = "std"`
- Existing `test_sporadic_budget_exhaustion_suppresses_dispatch`
  passes with ISR-driven refill (replaces polled-clock path on std
  too — atomic state is a strict superset)
- Per-platform integration test exercises actual ISR refill on at
  least one no-std target (FreeRTOS QEMU recommended)
