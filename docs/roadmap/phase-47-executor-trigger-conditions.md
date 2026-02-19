# Phase 47 — Executor Trigger Conditions

## Status: Not Started

## Background

nano-ros's `Executor::spin_once()` currently processes **every** registered
callback unconditionally on each iteration. It iterates `entries`, calls each
`try_process` function pointer, and tallies results. There is no mechanism to
gate execution based on data readiness.

In contrast, rclc (the ROS 2 C client for micro-ROS) provides **trigger
conditions** — executor-level gate functions that control whether callbacks
fire during a given spin iteration. This enables sensor-fusion patterns
(barrier synchronization), event-driven architectures (single-handle wakeup),
and custom scheduling policies.

### rclc Trigger Conditions

rclc's executor accepts a trigger function with signature:

```c
typedef bool (*rclc_executor_trigger_t)(
    rclc_executor_handle_t *handles,
    unsigned int size,
    void *obj
);
```

The executor evaluates this function **after** checking data availability (via
`rcl_wait()`) but **before** dispatching any callbacks. If the trigger returns
`false`, the entire iteration is skipped — no callbacks fire. Four built-in
triggers are provided:

| Trigger                        | Behavior                                                  |
|--------------------------------|-----------------------------------------------------------|
| `rclc_executor_trigger_any`    | Fire if **any** handle has new data (default)             |
| `rclc_executor_trigger_all`    | Fire only if **all** handles have new data (barrier sync) |
| `rclc_executor_trigger_one`    | Fire only if a **specific** handle has new data           |
| `rclc_executor_trigger_always` | Always fire, regardless of data availability              |

Custom triggers receive the full handle array and an opaque context pointer,
allowing arbitrary readiness predicates.

Additionally, each rclc handle has a per-handle **invocation mode**:

| Mode          | Behavior                                                 |
|---------------|----------------------------------------------------------|
| `ON_NEW_DATA` | Callback fires only when new data is available (default) |
| `ALWAYS`      | Callback fires every iteration, regardless of data       |

The trigger condition and per-handle invocation mode are orthogonal — the
trigger gates the entire iteration, while the invocation mode gates individual
callbacks within an iteration that passes the trigger.

### rclrs (ROS 2 Rust)

rclrs does not implement trigger conditions. It uses a `WaitSet` model
wrapping `rcl_wait()` which blocks until any entity has data, then dispatches
all ready callbacks. There is no trigger gate or per-callback invocation mode.

### nano-ros Current State

The executor already has the building blocks for trigger conditions:

1. **`Subscriber::has_data(&self) -> bool`** (in `nros-rmw/src/traits.rs:692`):
   Non-destructive readiness check. Zenoh backend checks an `AtomicBool` flag
   on the static subscriber buffer. XRCE backend conservatively returns `true`.

2. **`ServiceServerTrait::has_request(&self) -> bool`** (in `nros-rmw/src/traits.rs:798`):
   Same pattern for service servers.

3. **`CallbackMeta`** (in `nros-node/src/executor/arena.rs:31`): Type-erased
   metadata with `try_process` fn pointer. Currently has `offset`, `kind`,
   `try_process`, and `drop_fn` — but no `has_data` fn pointer.

4. **`spin_once()`** (in `nros-node/src/executor/spin.rs:590`): Iterates
   `entries`, calls `try_process` unconditionally. Does not pre-scan readiness.

### Goals

1. Add a `has_data` fn pointer to `CallbackMeta` for type-erased readiness queries
2. Add a `Trigger` enum to the executor for iteration-level gating
3. Add per-callback `InvocationMode` (ON_NEW_DATA / ALWAYS)
4. Modify `spin_once()` to evaluate the trigger before dispatching
5. Expose the API without requiring `alloc` — works on `no_std` targets
6. Add C API support for trigger conditions

### Non-Goals

- Wait-set based blocking (like rclrs) — nano-ros uses polling, not blocking
- Dynamic trigger changes during spin — trigger is set before spinning
- Custom triggers with heap-allocated closures — use fn pointer + context

## Design

### 47.1 — `has_data` Function Pointer on CallbackMeta

Add a monomorphized `has_data` fn pointer to `CallbackMeta`:

```rust
pub(crate) struct CallbackMeta {
    pub(crate) offset: usize,
    pub(crate) kind: EntryKind,
    pub(crate) try_process: unsafe fn(*mut u8, u64) -> Result<bool, TransportError>,
    pub(crate) drop_fn: unsafe fn(*mut u8),
    pub(crate) has_data: unsafe fn(*mut u8) -> bool,        // NEW
    pub(crate) invocation: InvocationMode,                   // NEW
}
```

For each concrete entry type, a monomorphized `has_data` function extracts the
subscriber/service handle and calls `has_data()` / `has_request()`:

```rust
pub(crate) unsafe fn sub_has_data<M, Sub, F, const RX_BUF: usize>(
    ptr: *mut u8,
) -> bool
where
    Sub: Subscriber,
{
    let entry = unsafe { &*(ptr as *const SubEntry<M, Sub, F, RX_BUF>) };
    entry.handle.has_data()
}

pub(crate) unsafe fn srv_has_data<Svc, Srv, F, const RQ: usize, const RP: usize>(
    ptr: *mut u8,
) -> bool
where
    Svc: RosService,
    Srv: ServiceServerTrait,
{
    let entry = unsafe { &*(ptr as *const SrvEntry<Svc, Srv, F, RQ, RP>) };
    entry.handle.has_request()
}

// Timers always report ready (readiness is time-based, handled in try_process)
pub(crate) unsafe fn timer_has_data<F>(_ptr: *mut u8) -> bool {
    true
}

// Action servers: check all sub-services
pub(crate) unsafe fn action_server_has_data<...>(ptr: *mut u8) -> bool {
    // Conservative: true (multiple internal handles)
    true
}

// Action clients: check feedback subscription
pub(crate) unsafe fn action_client_has_data<...>(ptr: *mut u8) -> bool {
    // Conservative: true (feedback sub readiness)
    true
}
```

**Registration changes:** Each `add_subscription*()`, `add_service*()`,
`add_timer*()`, and `add_action_*()` method sets the `has_data` fn pointer
in the `CallbackMeta` it creates. Default `invocation` is `OnNewData`.

### 47.2 — InvocationMode Enum

```rust
/// Per-callback invocation mode.
///
/// Controls whether a callback fires only when new data is available
/// or on every spin iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvocationMode {
    /// Fire only when `has_data()` returns true (default).
    #[default]
    OnNewData,
    /// Fire on every spin iteration, regardless of data availability.
    Always,
}
```

This is a public type in `nros-node::executor::types`.

### 47.3 — Trigger Enum

```rust
/// Executor-level trigger condition.
///
/// Controls when the executor dispatches callbacks during `spin_once()`.
/// The trigger is evaluated after polling the transport but before any
/// callback dispatch.
#[derive(Debug, Clone, Copy, Default)]
pub enum Trigger {
    /// Fire if any handle has new data (default behavior).
    #[default]
    Any,
    /// Fire only if all non-timer handles have new data (barrier sync).
    All,
    /// Fire only if the handle at the given entry index has new data.
    One(usize),
    /// Always fire, regardless of data availability.
    Always,
}
```

The `Trigger` enum is stored on the `Executor` struct:

```rust
pub struct Executor<S, const MAX_CBS: usize = 0, const CB_ARENA: usize = 0> {
    pub(crate) session: S,
    pub(crate) arena: [MaybeUninit<u8>; CB_ARENA],
    pub(crate) arena_used: usize,
    pub(crate) entries: [Option<CallbackMeta>; MAX_CBS],
    pub(crate) trigger: Trigger,                            // NEW
    // ... existing fields ...
}
```

Default is `Trigger::Any`, preserving current behavior.

### 47.4 — Modified `spin_once()` Flow

The updated `spin_once()` implements a two-phase dispatch:

```
Phase 1: Readiness scan
  - drive_io() to pump the transport
  - For each entry, call has_data() → build readiness bitmap

Phase 2: Trigger evaluation
  - Evaluate trigger condition against readiness bitmap
  - If trigger returns false → return SpinOnceResult::new() (no work)

Phase 3: Dispatch
  - For each entry:
    - If invocation == Always → call try_process
    - If invocation == OnNewData && readiness[i] → call try_process
    - Otherwise → skip
```

```rust
pub fn spin_once(&mut self, timeout_ms: i32) -> SpinOnceResult {
    let _ = self.session.drive_io(timeout_ms);

    let delta_ms = timeout_ms.max(0) as u64;
    let arena_ptr = self.arena.as_mut_ptr() as *mut u8;

    // Phase 1: Readiness scan
    let mut readiness = [false; MAX_CBS];
    for (i, meta) in self.entries.iter().enumerate() {
        if let Some(meta) = meta {
            let data_ptr = unsafe { arena_ptr.add(meta.offset) };
            readiness[i] = unsafe { (meta.has_data)(data_ptr) };
        }
    }

    // Phase 2: Trigger evaluation
    let should_dispatch = match self.trigger {
        Trigger::Always => true,
        Trigger::Any => readiness.iter()
            .enumerate()
            .any(|(i, &r)| r && self.entries[i].is_some()),
        Trigger::All => self.entries.iter()
            .enumerate()
            .all(|(i, e)| match e {
                Some(meta) if !matches!(meta.kind, EntryKind::Timer) => readiness[i],
                _ => true, // timers and empty slots don't block
            }),
        Trigger::One(idx) => readiness.get(idx).copied().unwrap_or(false),
    };

    if !should_dispatch {
        return SpinOnceResult::new();
    }

    // Phase 3: Dispatch (same as current, with invocation mode check)
    let mut result = SpinOnceResult::new();
    for (i, meta) in self.entries.iter().flatten().enumerate() {
        let should_invoke = match meta.invocation {
            InvocationMode::Always => true,
            InvocationMode::OnNewData => readiness[i],
        };
        if !should_invoke {
            continue;
        }

        let data_ptr = unsafe { arena_ptr.add(meta.offset) };
        match unsafe { (meta.try_process)(data_ptr, delta_ms) } {
            Ok(true) => match meta.kind {
                EntryKind::Subscription | EntryKind::ActionClient => {
                    result.subscriptions_processed += 1;
                }
                EntryKind::Service | EntryKind::ActionServer => {
                    result.services_handled += 1;
                }
                EntryKind::Timer => result.timers_fired += 1,
            },
            Ok(false) => {}
            Err(_) => match meta.kind {
                EntryKind::Subscription | EntryKind::ActionClient => {
                    result.subscription_errors += 1;
                }
                EntryKind::Service | EntryKind::ActionServer => {
                    result.service_errors += 1;
                }
                EntryKind::Timer => {}
            },
        }
    }

    // Parameter services (outside arena, unaffected by trigger)
    #[cfg(feature = "param-services")]
    if let Some(params) = &mut self.params {
        let crate::parameter_services::ParamState { server, services } = &mut **params;
        if let Ok(n) = services.process_services(server) {
            result.services_handled += n;
        }
    }

    result
}
```

**Key design decisions:**

- Timers are excluded from `Trigger::All` — they are time-based, not data-based
- Parameter services are outside the trigger gate — they always process
- `Trigger::Any` is the default, matching current unconditional dispatch
  (since `try_process` already returns `Ok(false)` when no data, the only
  behavioral change is the explicit readiness pre-scan)
- The readiness array is stack-allocated using `MAX_CBS` const generic

### 47.5 — Public API

```rust
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    Executor<S, MAX_CBS, CB_ARENA>
{
    /// Set the executor trigger condition.
    ///
    /// The trigger is evaluated on each `spin_once()` iteration after
    /// polling the transport. If the trigger returns false, no callbacks
    /// fire for that iteration.
    ///
    /// Default: `Trigger::Any` (fire if any handle has new data).
    pub fn set_trigger(&mut self, trigger: Trigger) {
        self.trigger = trigger;
    }

    /// Get the current trigger condition.
    pub fn trigger(&self) -> Trigger {
        self.trigger
    }
}
```

**Registration API extensions:**

```rust
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    Executor<S, MAX_CBS, CB_ARENA>
{
    /// Register a subscription with a specific invocation mode.
    ///
    /// Returns the entry index, which can be used with `Trigger::One(index)`.
    pub fn add_subscription_with_mode<M, F>(
        &mut self,
        topic_name: &str,
        mode: InvocationMode,
        callback: F,
    ) -> Result<usize, NodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M) + 'static,
        S::SubscriberHandle: Subscriber,
    {
        // ... same as add_subscription but sets meta.invocation = mode
        // Returns the slot index
    }
}
```

Existing `add_subscription()` remains unchanged (default `OnNewData`),
preserving backward compatibility.

### 47.6 — C API

```c
/// Trigger condition types
typedef enum {
    NROS_TRIGGER_ANY = 0,      ///< Fire if any handle has data (default)
    NROS_TRIGGER_ALL = 1,      ///< Fire if all handles have data (barrier)
    NROS_TRIGGER_ONE = 2,      ///< Fire if specific handle has data
    NROS_TRIGGER_ALWAYS = 3,   ///< Always fire
} nano_ros_trigger_t;

/// Invocation mode for individual handles
typedef enum {
    NROS_ON_NEW_DATA = 0,     ///< Fire only when new data available (default)
    NROS_ALWAYS = 1,          ///< Fire every iteration
} nano_ros_invocation_t;

/// Set the executor trigger condition.
void nano_ros_executor_set_trigger(
    nano_ros_executor_t *executor,
    nano_ros_trigger_t trigger,
    int32_t trigger_index    ///< Handle index for NROS_TRIGGER_ONE, ignored otherwise
);

/// Set invocation mode for a specific handle slot.
void nano_ros_executor_set_invocation(
    nano_ros_executor_t *executor,
    uint32_t handle_index,
    nano_ros_invocation_t mode
);
```

## Usage Examples

### Sensor Fusion (Barrier Sync)

Wait for both IMU and GPS data before processing:

```rust
let config = ExecutorConfig::from_env().node_name("fusion");
let mut executor = Executor::<_, 4, 8192>::open(&config)?;

executor.add_subscription::<Imu, _>("/imu", |imu| {
    // Process IMU data
})?;
executor.add_subscription::<NavSatFix, _>("/gps", |gps| {
    // Process GPS data
})?;
executor.set_trigger(Trigger::All);
executor.spin_blocking(SpinOptions::default())?;
```

### Event-Driven (Single Handle)

Only process when a specific sensor reports:

```rust
let config = ExecutorConfig::from_env().node_name("event_driven");
let mut executor = Executor::<_, 4, 8192>::open(&config)?;

let lidar_idx = executor.add_subscription_with_mode::<LaserScan, _>(
    "/scan", InvocationMode::OnNewData, |scan| {
        // Process scan
    }
)?;
executor.add_subscription::<Odometry, _>("/odom", |odom| {
    // Also gets latest odom when lidar fires
})?;
executor.set_trigger(Trigger::One(lidar_idx));
executor.spin_blocking(SpinOptions::default())?;
```

### Always-Invoke Watchdog

A timer callback that always runs, even when no data arrives:

```rust
executor.add_timer(TimerDuration::from_millis(1000), || {
    log::info!("Watchdog: still alive");
})?;
// Timers always report has_data=true, so they always fire
// regardless of trigger condition
```

## Implementation Plan

### 47.1 — Core Infrastructure

- [ ] Add `InvocationMode` enum to `executor/types.rs`
- [ ] Add `Trigger` enum to `executor/types.rs`
- [ ] Add `has_data` fn pointer field to `CallbackMeta`
- [ ] Add `invocation` field to `CallbackMeta`
- [ ] Add `trigger` field to `Executor` struct (default `Trigger::Any`)
- [ ] Write monomorphized `sub_has_data()` for `SubEntry`
- [ ] Write monomorphized `sub_info_has_data()` for `SubInfoEntry`
- [ ] Write monomorphized `sub_safety_has_data()` for `SubSafetyEntry` (cfg safety-e2e)
- [ ] Write monomorphized `srv_has_data()` for `SrvEntry`
- [ ] Write monomorphized `timer_has_data()` for `TimerEntry` (always true)
- [ ] Write monomorphized `action_server_has_data()` (always true)
- [ ] Write monomorphized `action_client_has_data()` (always true)

### 47.2 — Registration Wiring

- [ ] Update `add_subscription_sized()` to set `has_data` and `invocation` in `CallbackMeta`
- [ ] Update `add_subscription_with_info_sized()` similarly
- [ ] Update `add_subscription_with_safety_sized()` similarly (cfg safety-e2e)
- [ ] Update `add_service_sized()` to set `has_data` and `invocation`
- [ ] Update `add_timer()` and `add_timer_oneshot()` to set `has_data` and `invocation`
- [ ] Update `add_action_server()` to set `has_data` and `invocation`
- [ ] Update `add_action_client()` to set `has_data` and `invocation`
- [ ] Add `add_subscription_with_mode()` returning entry index
- [ ] Add `add_service_with_mode()` returning entry index

### 47.3 — spin_once() Trigger Logic

- [ ] Refactor `spin_once()` to implement the three-phase flow (readiness scan, trigger eval, dispatch)
- [ ] Add `set_trigger()` and `trigger()` public methods
- [ ] Verify `Trigger::Any` produces identical behavior to current code
- [ ] Ensure parameter services bypass the trigger gate

### 47.4 — C API

- [ ] Add `nano_ros_trigger_t` enum to `nano_ros/executor.h`
- [ ] Add `nano_ros_invocation_t` enum to `nano_ros/executor.h`
- [ ] Implement `nano_ros_executor_set_trigger()` in `nros-c/src/executor.rs`
- [ ] Implement `nano_ros_executor_set_invocation()` in `nros-c/src/executor.rs`

### 47.5 — Tests and Verification

- [ ] Unit test: `Trigger::Any` matches current behavior (default, no regression)
- [ ] Unit test: `Trigger::All` blocks when not all handles have data
- [ ] Unit test: `Trigger::One(idx)` only fires when specific handle ready
- [ ] Unit test: `Trigger::Always` always dispatches
- [ ] Unit test: `InvocationMode::Always` fires callback even without data
- [ ] Unit test: `InvocationMode::OnNewData` skips callback when no data
- [ ] Integration test: sensor-fusion pattern with two subscriptions
- [ ] Kani harness: trigger evaluation soundness (all variants)
- [ ] Verify `just quality` passes

## Files to Modify

| File                                               | Changes                                                                       |
|----------------------------------------------------|-------------------------------------------------------------------------------|
| `packages/core/nros-node/src/executor/types.rs`    | Add `InvocationMode`, `Trigger` enums                                         |
| `packages/core/nros-node/src/executor/arena.rs`    | Add `has_data` + `invocation` to `CallbackMeta`; add `*_has_data()` fns       |
| `packages/core/nros-node/src/executor/spin.rs`     | Three-phase `spin_once()`; `set_trigger()` API; `trigger` field on `Executor` |
| `packages/core/nros-node/src/executor/actions.rs`  | Update action registration to set `has_data`/`invocation`                     |
| `packages/core/nros-c/src/executor.rs`             | C API: `set_trigger()`, `set_invocation()`                                    |
| `packages/core/nros-c/include/nano_ros/executor.h` | C enum/function declarations                                                  |

## Verification

1. `just quality` — full format + clippy + nextest + miri + QEMU
2. Existing integration tests pass unchanged (default `Trigger::Any` = no regression)
3. New unit tests for all trigger variants and invocation modes
4. Kani bounded model checking on trigger evaluation
