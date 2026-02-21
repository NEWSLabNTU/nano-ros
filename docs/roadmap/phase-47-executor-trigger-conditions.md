# Phase 47 — Executor Trigger Conditions

## Status: Complete

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
6. Return `HandleId` from registration methods for type-safe handle references
7. Provide `HandleSet` bitset for subset triggers (`AllOf` / `AnyOf`)
8. Provide `ReadinessSnapshot` for custom predicates

### Non-Goals

- Wait-set based blocking (like rclrs) — nano-ros uses polling, not blocking
- Dynamic trigger changes during spin — trigger is set before spinning
- Custom triggers with heap-allocated closures — use fn pointer
- C API changes — nros-c already has its own trigger implementation (see
  [nros-c Gap Analysis](#nros-c-gap-analysis) for the unification plan)

---

## Rust API Design

### HandleId

Opaque index returned from every registration method:

```rust
/// Opaque handle identifier returned by registration methods.
///
/// Used with `Trigger::One` and `HandleSet` for type-safe trigger
/// configuration. The inner value is the entry slot index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandleId(pub(crate) usize);
```

**Registration return type changes:**

```rust
// Before:
pub fn add_subscription<M, F>(...) -> Result<(), NodeError>
pub fn add_service<Svc, F>(...) -> Result<(), NodeError>
pub fn add_timer<F>(...) -> Result<(), NodeError>

// After:
pub fn add_subscription<M, F>(...) -> Result<HandleId, NodeError>
pub fn add_service<Svc, F>(...) -> Result<HandleId, NodeError>
pub fn add_timer<F>(...) -> Result<HandleId, NodeError>
```

Backward compatible for callers using `?` — they just get a value they can
ignore. Action handles already have `entry_index`; add a `.handle_id()`
accessor:

```rust
impl<A: RosAction> ActionServerHandle<A> {
    pub fn handle_id(&self) -> HandleId {
        HandleId(self.entry_index)
    }
}

impl<A: RosAction> ActionClientHandle<A> {
    pub fn handle_id(&self) -> HandleId {
        HandleId(self.entry_index)
    }
}
```

### HandleSet

`no_std` bitset backed by `u64` (supports up to 64 handles — far beyond any
realistic embedded `MAX_CBS`):

```rust
/// A set of handle IDs, represented as a bitset.
///
/// Supports up to 64 handles. Construct via `HandleId` operators:
/// ```ignore
/// let set = imu | gps | lidar;  // HandleSet from 3 handles
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HandleSet(u64);

impl HandleSet {
    pub const EMPTY: Self = Self(0);

    pub const fn insert(self, id: HandleId) -> Self {
        Self(self.0 | (1u64 << id.0))
    }

    pub const fn contains(self, id: HandleId) -> bool {
        self.0 & (1u64 << id.0) != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}
```

Ergonomic `BitOr` operators for set construction:

```rust
// HandleId | HandleId → HandleSet
impl BitOr for HandleId {
    type Output = HandleSet;
    fn bitor(self, rhs: HandleId) -> HandleSet {
        HandleSet::EMPTY.insert(self).insert(rhs)
    }
}

// HandleSet | HandleId → HandleSet
impl BitOr<HandleId> for HandleSet {
    type Output = HandleSet;
    fn bitor(self, rhs: HandleId) -> HandleSet {
        self.insert(rhs)
    }
}

// HandleSet | HandleSet → HandleSet
impl BitOr for HandleSet {
    type Output = HandleSet;
    fn bitor(self, rhs: HandleSet) -> HandleSet {
        self.union(rhs)
    }
}
```

### ReadinessSnapshot

Type-safe view of handle readiness passed to custom predicates. Avoids
exposing raw `&[bool]` arrays or requiring callers to know slot indices:

```rust
/// Snapshot of handle readiness at the start of a spin iteration.
///
/// Passed to `Trigger::Predicate` functions. Query by `HandleId`.
pub struct ReadinessSnapshot {
    bits: u64,
    count: usize,
}

impl ReadinessSnapshot {
    /// Check if a specific handle has data.
    pub const fn is_ready(&self, id: HandleId) -> bool {
        self.bits & (1u64 << id.0) != 0
    }

    /// Check if all handles in the set have data.
    pub const fn all_ready(&self, set: HandleSet) -> bool {
        self.bits & set.0 == set.0
    }

    /// Check if any handle in the set has data.
    pub const fn any_ready(&self, set: HandleSet) -> bool {
        self.bits & set.0 != 0
    }

    /// Number of handles that have data.
    pub const fn ready_count(&self) -> u32 {
        self.bits.count_ones()
    }

    /// Total registered handles.
    pub const fn total(&self) -> usize {
        self.count
    }
}
```

### InvocationMode

Per-callback invocation mode:

```rust
/// Per-callback invocation mode.
///
/// Controls whether a callback fires only when new data is available
/// or on every spin iteration that passes the trigger gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvocationMode {
    /// Fire only when `has_data()` returns true (default).
    #[default]
    OnNewData,
    /// Fire on every spin iteration, regardless of data availability.
    Always,
}
```

### Trigger Enum

```rust
/// Executor-level trigger condition.
///
/// Controls when the executor dispatches callbacks during `spin_once()`.
/// The trigger is evaluated after polling the transport but before any
/// callback dispatch.
#[derive(Debug, Clone, Copy, Default)]
pub enum Trigger {
    /// Fire if any registered handle has data (default).
    /// Matches current behavior.
    #[default]
    Any,

    /// Fire only when ALL non-timer handles have data.
    /// Classic barrier synchronization.
    All,

    /// Fire only when a specific handle has data.
    One(HandleId),

    /// Fire only when every handle in the set has data.
    /// Subset barrier — more flexible than `All`.
    AllOf(HandleSet),

    /// Fire when any handle in the set has data.
    /// Subset event-driven — more flexible than `One`.
    AnyOf(HandleSet),

    /// Always fire, regardless of data availability.
    Always,

    /// Custom predicate over a readiness snapshot.
    ///
    /// no_std compatible — fn pointer, no closure. The function receives
    /// a `ReadinessSnapshot` which can be queried by `HandleId`.
    Predicate(fn(&ReadinessSnapshot) -> bool),
}
```

### CallbackMeta Changes

Add `has_data` fn pointer and `invocation` field:

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

Monomorphized `has_data` functions for each entry type:

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

// Action server/client: conservative (multiple internal handles)
pub(crate) unsafe fn action_server_has_data<...>(_ptr: *mut u8) -> bool {
    true
}
pub(crate) unsafe fn action_client_has_data<...>(_ptr: *mut u8) -> bool {
    true
}
```

### Executor Struct Changes

```rust
pub struct Executor<S, const MAX_CBS: usize = 0, const CB_ARENA: usize = 0> {
    pub(crate) session: S,
    pub(crate) arena: [MaybeUninit<u8>; CB_ARENA],
    pub(crate) arena_used: usize,
    pub(crate) entries: [Option<CallbackMeta>; MAX_CBS],
    pub(crate) trigger: Trigger,                            // NEW
    // ... existing fields unchanged ...
}
```

### Modified `spin_once()` — Three-Phase Flow

```
Phase 1: Readiness scan
  - drive_io() to pump the transport
  - For each entry, call has_data() → build readiness bitset

Phase 2: Trigger evaluation
  - Evaluate trigger against readiness snapshot
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

    // Phase 1: Readiness scan → bitset
    let mut readiness_bits: u64 = 0;
    let mut entry_count: usize = 0;
    for (i, meta) in self.entries.iter().enumerate() {
        if let Some(meta) = meta {
            entry_count += 1;
            let data_ptr = unsafe { arena_ptr.add(meta.offset) };
            if unsafe { (meta.has_data)(data_ptr) } {
                readiness_bits |= 1u64 << i;
            }
        }
    }

    let snapshot = ReadinessSnapshot {
        bits: readiness_bits,
        count: entry_count,
    };

    // Phase 2: Trigger evaluation
    let should_dispatch = match self.trigger {
        Trigger::Always => true,
        Trigger::Any => readiness_bits != 0,
        Trigger::All => self.entries.iter().enumerate().all(|(i, e)| match e {
            Some(meta) if !matches!(meta.kind, EntryKind::Timer) => {
                readiness_bits & (1u64 << i) != 0
            }
            _ => true,
        }),
        Trigger::One(id) => readiness_bits & (1u64 << id.0) != 0,
        Trigger::AllOf(set) => readiness_bits & set.0 == set.0,
        Trigger::AnyOf(set) => readiness_bits & set.0 != 0,
        Trigger::Predicate(f) => f(&snapshot),
    };

    if !should_dispatch {
        return SpinOnceResult::new();
    }

    // Phase 3: Dispatch with invocation mode check
    let mut result = SpinOnceResult::new();
    for (i, meta) in self.entries.iter().flatten().enumerate() {
        let ready = readiness_bits & (1u64 << i) != 0;
        let should_invoke = match meta.invocation {
            InvocationMode::Always => true,
            InvocationMode::OnNewData => ready,
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
- `Trigger::Any` is the default, preserving current behavior
- Readiness is a `u64` bitset, not an array — zero-cost for subset operations
- `ReadinessSnapshot` wraps the bitset for type-safe queries in `Predicate`

### Public API

```rust
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    Executor<S, MAX_CBS, CB_ARENA>
{
    /// Set the executor trigger condition.
    pub fn set_trigger(&mut self, trigger: Trigger) {
        self.trigger = trigger;
    }

    /// Get the current trigger condition.
    pub fn trigger(&self) -> Trigger {
        self.trigger
    }

    /// Set invocation mode for a specific handle.
    pub fn set_invocation(&mut self, id: HandleId, mode: InvocationMode) {
        if let Some(meta) = self.entries.get_mut(id.0).and_then(|e| e.as_mut()) {
            meta.invocation = mode;
        }
    }
}
```

Existing registration methods change return type from `Result<(), NodeError>`
to `Result<HandleId, NodeError>`. No new `_with_mode` variants needed —
`set_invocation()` is cleaner for the common case where invocation mode
is set after registration.

### Comparison with rclc

| Feature | rclc | nano-ros Rust API |
|---------|------|-------------------|
| Trigger gate | `trigger_any/all/one/always` + custom fn ptr | `Any/All/One/Always` + `AllOf/AnyOf` + `Predicate` |
| Handle reference | Raw pointer comparison | Type-safe `HandleId` |
| Subset operations | Only via custom fn | First-class `AllOf(HandleSet)` / `AnyOf(HandleSet)` |
| Set construction | Manual | `BitOr` operator: `imu \| gps` |
| Custom predicate | `fn(*bool, usize, *void) -> bool` | `fn(&ReadinessSnapshot) -> bool` |
| Per-handle invocation | `ON_NEW_DATA` / `ALWAYS` | `OnNewData` / `Always` |
| no_std compatible | N/A (C) | Yes — fn pointers, bitset, no heap |

### What This Doesn't Have (By Design)

- **Closure-based predicates** — would require `Box<dyn Fn>` or
  arena-allocated closures. `fn` pointer + `ReadinessSnapshot` covers real
  use cases without heap.
- **Trigger composition operators** (`trigger_a & trigger_b`) — expression
  tree needs allocation. `AllOf`/`AnyOf` cover practical cases; `Predicate`
  covers the rest.
- **Wait-set blocking** — nano-ros is polling-based. Trigger conditions gate
  dispatch, they don't block the thread.
- **Runtime handle removal** — handles are arena-allocated with no dealloc.
  Matches the embedded lifecycle (register once, spin forever).

---

## Usage Examples

### Sensor Fusion (Barrier Sync)

Wait for both IMU and GPS data before processing:

```rust
let config = ExecutorConfig::from_env().node_name("fusion");
let mut executor = Executor::<_, 4, 8192>::open(&config)?;

let imu = executor.add_subscription::<Imu, _>("/imu", |msg| {
    // Process IMU data
})?;
let gps = executor.add_subscription::<NavSatFix, _>("/gps", |msg| {
    // Process GPS data
})?;
executor.set_trigger(Trigger::AllOf(imu | gps));
executor.spin_blocking(SpinOptions::default())?;
```

### Event-Driven (Single Handle)

Only process when a specific sensor reports:

```rust
let config = ExecutorConfig::from_env().node_name("event_driven");
let mut executor = Executor::<_, 4, 8192>::open(&config)?;

let lidar = executor.add_subscription::<LaserScan, _>("/scan", |scan| {
    // Process scan
})?;
executor.add_subscription::<Odometry, _>("/odom", |odom| {
    // Gets latest odom when lidar fires
})?;
executor.set_trigger(Trigger::One(lidar));
executor.spin_blocking(SpinOptions::default())?;
```

### Always-Invoke Watchdog

A timer that always runs, even when trigger is not satisfied:

```rust
let sub = executor.add_subscription::<Int32, _>("/data", |msg| { /* ... */ })?;
let wd = executor.add_timer(TimerDuration::from_millis(1000), || {
    log::warn!("watchdog tick");
})?;

// Gate on data subscription, but watchdog always fires
executor.set_trigger(Trigger::One(sub));
executor.set_invocation(wd, InvocationMode::Always);
```

### Subset Barrier (Critical Trio)

```rust
let a = executor.add_subscription::<Imu, _>("/a", |_| {})?;
let b = executor.add_subscription::<Imu, _>("/b", |_| {})?;
let c = executor.add_subscription::<Imu, _>("/c", |_| {})?;
let _d = executor.add_subscription::<Imu, _>("/d", |_| {})?;
let _e = executor.add_subscription::<Imu, _>("/e", |_| {})?;

// Gate on the critical trio only
executor.set_trigger(Trigger::AllOf(a | b | c));
```

### Custom Predicate

```rust
// HandleId values are known at registration order
let imu = executor.add_subscription::<Imu, _>("/imu", |_| {})?;
let gps = executor.add_subscription::<NavSatFix, _>("/gps", |_| {})?;

// fn pointer — no captures, no heap
fn my_trigger(snap: &ReadinessSnapshot) -> bool {
    // Fire when both IMU and GPS are ready, or when at least 3 handles are
    snap.ready_count() >= 3
}

executor.set_trigger(Trigger::Predicate(my_trigger));
```

Note: `Predicate` uses a bare `fn` pointer (no captures), so handle IDs must
be known statically or stored in a `static`. For most use cases, `AllOf` /
`AnyOf` / `One` cover the need without custom predicates.

---

## nros-c Gap Analysis

### Current State

The C API (`nros-c`) has a **completely parallel executor** that does not use
`nros-node` at all. It wraps `nros-rmw` directly:

```
nros-c → nros-rmw (transport traits)
nros-c ✗ does NOT use nros-node
```

Self-implementations in nros-c:

| Component             | nros-c                                   | nros-node equivalent                                |
|-----------------------|------------------------------------------|-----------------------------------------------------|
| Executor struct       | `nano_ros_executor_t` (fixed arrays)     | `Executor<S, MAX_CBS, CB_ARENA>` (arena)            |
| Spin loop             | `spin_some()` with manual dispatch       | `spin_once()` with `CallbackMeta` dispatch          |
| Trigger conditions    | `nano_ros_executor_trigger_t` fn ptr     | **Not yet** (Phase 47 adds this)                    |
| Invocation mode       | `NROS_EXECUTOR_ON_NEW_DATA` / `ALWAYS`   | **Not yet** (Phase 47 adds this)                    |
| LET semantics         | Per-handle LET buffers + sampling        | **Not in nros-node**                                |
| Timer                 | `nano_ros_timer_t` (own period tracking) | `TimerEntry<F>` in arena                            |
| Guard condition       | `nano_ros_guard_condition_t`             | **Not in nros-node**                                |
| Subscription dispatch | `process_subscription()` (raw bytes)     | `sub_try_process()` (typed deserialize)             |
| Service dispatch      | `process_service_request()` (raw bytes)  | `srv_try_process()` (typed deserialize)             |
| Action                | Own goal UUID tracking                   | `ActionServerArenaEntry` / `ActionClientArenaEntry` |

### Missing Features in nros-node Required for Unification

To make nros-c a thin wrapper over nros-node's `Executor`, these features
must exist in the Rust executor first:

1. **Trigger conditions + invocation modes** — Phase 47 (this doc)
2. **Raw-bytes callback variant** — C callbacks receive `(*const u8, usize)`,
   not deserialized `&M`. Need `add_subscription_raw()` that passes CDR bytes
   to the callback without deserialization.
3. **LET semantics** — Sample all subscriptions at start of spin cycle, store
   in per-handle buffers, process from snapshot. The Rust executor currently
   only supports RCLCPP-style interleaved semantics.
4. **Guard conditions** — Manual trigger from another thread or ISR. Need a
   `GuardCondition` type with `trigger()` / `is_triggered()` / `clear()`.
5. **Concrete type instantiation** — nros-c needs `Executor` instantiated
   with build-time `MAX_CBS` and `CB_ARENA` constants (from `build.rs`), not
   user-specified const generics.

### Unification Path

Phase 47 addresses item 1 (trigger conditions) and items 2–5 (nros-node
infrastructure prerequisites) in sub-phases 47.6–47.9. Phase 49 then
handles the C API refactor (rename + delegation) on top of the complete
Rust executor API.

---

## Implementation Plan

### 47.1 — Core Types

- [x] Add `HandleId` struct to `executor/types.rs`
- [x] Add `HandleSet` struct with `insert`/`contains`/`union`/`len`/`is_empty`
- [x] Add `BitOr` impls: `HandleId | HandleId`, `HandleSet | HandleId`, `HandleSet | HandleSet`
- [x] Add `ReadinessSnapshot` struct with `is_ready`/`all_ready`/`any_ready`/`ready_count`/`total`
- [x] Add `InvocationMode` enum (`OnNewData`, `Always`)
- [x] Add `Trigger` enum (`Any`, `All`, `One`, `AllOf`, `AnyOf`, `Always`, `Predicate`)

### 47.2 — CallbackMeta and has_data Functions

- [x] Add `has_data` fn pointer field to `CallbackMeta`
- [x] Add `invocation` field to `CallbackMeta`
- [x] Write monomorphized `sub_has_data()` for `SubEntry`
- [x] Write monomorphized `sub_info_has_data()` for `SubInfoEntry`
- [x] Write monomorphized `sub_safety_has_data()` for `SubSafetyEntry` (cfg safety-e2e)
- [x] Write monomorphized `srv_has_data()` for `SrvEntry`
- [x] Write monomorphized `timer_has_data()` for `TimerEntry` (always true)
- [x] Write monomorphized `action_server_has_data()` (always true)
- [x] Write monomorphized `action_client_has_data()` (always true)

### 47.3 — Registration Wiring

- [x] Change `add_subscription_sized()` to return `HandleId` and set `has_data`/`invocation`
- [x] Change `add_subscription_with_info_sized()` similarly
- [x] Change `add_subscription_with_safety_sized()` similarly (cfg safety-e2e)
- [x] Change `add_service_sized()` to return `HandleId` and set `has_data`/`invocation`
- [x] Change `add_timer()` and `add_timer_oneshot()` to return `HandleId` and set `has_data`/`invocation`
- [x] Update `add_action_server_sized()` to set `has_data`/`invocation`
- [x] Update `add_action_client_sized()` to set `has_data`/`invocation`
- [x] Add `handle_id()` method to `ActionServerHandle` and `ActionClientHandle`

### 47.4 — Executor Fields and spin_once()

- [x] Add `trigger: Trigger` field to `Executor` struct (default `Trigger::Any`)
- [x] Add `set_trigger()` and `trigger()` public methods
- [x] Add `set_invocation(HandleId, InvocationMode)` public method
- [x] Refactor `spin_once()` to implement three-phase flow (readiness scan → trigger eval → dispatch)
- [x] Verify `Trigger::Any` produces identical behavior to current code
- [x] Ensure parameter services bypass the trigger gate

### 47.5 — Tests and Verification

- [x] Unit test: `Trigger::Any` matches current behavior (no regression)
- [x] Unit test: `Trigger::All` blocks when not all non-timer handles have data
- [x] Unit test: `Trigger::One(id)` only fires when specific handle ready
- [x] Unit test: `Trigger::AllOf(set)` subset barrier
- [x] Unit test: `Trigger::AnyOf(set)` subset event-driven
- [x] Unit test: `Trigger::Always` always dispatches
- [x] Unit test: `Trigger::Predicate` custom function
- [x] Unit test: `InvocationMode::Always` fires callback even without data
- [x] Unit test: `InvocationMode::OnNewData` skips callback when no data
- [x] Unit test: `HandleSet` bitwise operations
- [x] Unit test: `ReadinessSnapshot` queries
- [x] Unit test: `HandleId` returned from registration matches entry index
- [ ] Integration test: sensor-fusion pattern with two subscriptions
- [ ] Kani harness: trigger evaluation soundness (all variants)
- [ ] Kani harness: `HandleSet` insert/contains correctness
- [x] Verify `just quality` passes

### 47.6 — Raw-bytes Callbacks

C callbacks receive `(*const u8, usize)`, not typed `&M`. Add raw-bytes
entry types to the executor arena so the C API can delegate to nros-node
without deserializing/re-serializing CDR data.

**New types:**

- `RawSubscriptionCallback` — `unsafe extern "C" fn(data: *const u8, len: usize, context: *mut c_void)`
- `RawServiceCallback` — `unsafe extern "C" fn(req: *const u8, req_len: usize, resp: *mut u8, resp_cap: usize, resp_len: *mut usize, context: *mut c_void) -> bool`
- `SubRawEntry<Sub, RX_BUF>` — subscriber + raw fn ptr + context + buffer
- `SrvRawEntry<Srv, REQ_BUF, REPLY_BUF>` — service server + raw fn ptr + context + buffers

**Dispatch functions:**

- `sub_raw_try_process()` — `try_recv_raw()` → raw callback
- `srv_raw_try_process()` — `try_recv_request()` → raw callback → `send_reply()`
- `sub_raw_has_data()` / `srv_raw_has_data()` — delegates to transport `has_data()`/`has_request()`

**Registration:**

- `Executor::add_subscription_raw()` → `HandleId`
- `Executor::add_service_raw()` → `HandleId`

**Tasks:**

- [x] Define `RawSubscriptionCallback` and `RawServiceCallback` type aliases
  in `executor/types.rs`
- [x] Add `SubRawEntry<Sub, RX_BUF>` struct to `executor/arena.rs`
- [x] Add `SrvRawEntry<Srv, REQ_BUF, REPLY_BUF>` struct to `executor/arena.rs`
- [x] Implement `sub_raw_try_process()` and `srv_raw_try_process()`
- [x] Implement `sub_raw_has_data()` and `srv_raw_has_data()`
- [x] Add `Executor::add_subscription_raw()` and `add_service_raw()`
- [x] Unit tests for raw subscription dispatch
- [x] Unit tests for raw service dispatch

**Files:** `nros-node/src/executor/{arena.rs, spin.rs, types.rs}`

### 47.7 — Guard Conditions

Guard conditions are manual triggers — an atomic flag that can be set from
any thread or ISR to wake the executor. Used for shutdown signaling,
inter-thread notifications, and custom event injection.

**Arena integration:**

- `GuardConditionEntry` — atomic bool flag + callback fn + context
- `guard_try_process()` — check flag, clear, invoke callback if set
- `guard_has_data()` — check triggered flag
- `GuardConditionHandle` — external handle for triggering (Send + Sync)

**Registration:**

- `Executor::add_guard_condition()` → `HandleId`

Guard conditions are excluded from `Trigger::All` (like timers — they are
event-driven, not data-driven).

**Tasks:**

- [x] Add `GuardConditionEntry` to `executor/arena.rs`
- [x] Add `GuardConditionHandle` to `executor/types.rs`
- [x] Implement `guard_try_process()` dispatch function
- [x] Implement `guard_has_data()` readiness function
- [x] Add `EntryKind::GuardCondition` variant
- [x] Add `Executor::add_guard_condition()` → returns `(HandleId, GuardConditionHandle)`
- [x] Ensure `GuardConditionHandle` is `Send + Sync`
- [x] Unit tests for guard condition trigger/clear/callback
- [x] Unit tests for executor integration (spin_once processes guard conditions)

**Files:** `nros-node/src/executor/{arena.rs, spin.rs, types.rs}`

### 47.8 — LET Semantics

Logical Execution Time: sample all subscription data at the start of each
spin cycle, then process callbacks from the snapshot. Prevents data races
where a callback sees newer data than earlier callbacks in the same cycle.

**Types:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutorSemantics {
    #[default]
    RclcppExecutor,
    LogicalExecutionTime,
}
```

**Mechanism:**

- Add `semantics: ExecutorSemantics` field to `Executor`
- Add `Executor::set_semantics()` method
- If LET: pre-sample all subscription entries into per-entry buffer before
  dispatch phase. Callbacks read from snapshot, not live transport.
- Services always use immediate mode (request-reply is sequential)
- LET buffer reuses existing `RX_BUF` const generic on sub entries

**Tasks:**

- [x] Define `ExecutorSemantics` enum in `executor/types.rs`
- [x] Add `semantics` field to `Executor` struct
- [x] Add `Executor::set_semantics()` method
- [x] Add LET buffer field to `SubEntry` and `SubRawEntry`
- [x] Implement pre-sample phase in `spin_once()` (between readiness scan
  and dispatch, only when `semantics == LogicalExecutionTime`)
- [x] Modify `sub_try_process()` / `sub_raw_try_process()` to read from
  LET buffer when in LET mode
- [x] Unit tests for LET semantics (data sampled once, consistent snapshot)
- [x] Unit tests verifying default RclcppExecutor behavior is unchanged

**Files:** `nros-node/src/executor/{types.rs, spin.rs, arena.rs}`

### 47.9 — Session-Borrowing Executor

nros-node's `Executor<S>` owns the session. The C API requires the support
object to own the session, with the executor borrowing it.

**Implementation:**

Store an enum `SessionStore<S>` with `Owned(S)` / `Borrowed(*mut S)` and
`Deref`/`DerefMut` impls. The existing code uses `self.session` unchanged.

```rust
pub unsafe fn from_session_ptr(session_ptr: *mut S) -> Self
```

**Tasks:**

- [x] Add `SessionStore<S>` enum to executor
- [x] Implement `Deref`/`DerefMut` for `SessionStore`
- [x] Implement `from_session_ptr()` constructor
- [x] Add `session()` / `session_mut()` accessors
- [x] Ensure `from_session()` wraps in `SessionStore::Owned`
- [x] Unit tests for borrowed-session executor lifecycle
- [x] Document safety requirements

**Files:** `nros-node/src/executor/spin.rs`

---

## Files to Modify

| File                                             | Changes                                                                                                                 |
|--------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------|
| `packages/core/nros-node/src/executor/types.rs`  | `HandleId`, `HandleSet`, `ReadinessSnapshot`, `InvocationMode`, `Trigger`, `RawSubscriptionCallback`, `RawServiceCallback`, `ExecutorSemantics`, `GuardConditionHandle` |
| `packages/core/nros-node/src/executor/arena.rs`  | `has_data` + `invocation` on `CallbackMeta`; `*_has_data()` fns; `SubRawEntry`, `SrvRawEntry`, `GuardConditionEntry`; LET buffer fields |
| `packages/core/nros-node/src/executor/spin.rs`   | Three-phase `spin_once()`; `trigger`/`semantics` fields; `set_trigger()`/`set_invocation()`/`set_semantics()` API; `SessionStore`; `from_session_ptr()`; `add_subscription_raw()`/`add_service_raw()`/`add_guard_condition()`; return `HandleId` from registration |
| `packages/core/nros-node/src/executor/action.rs` | Set `has_data`/`invocation` in action registration; `handle_id()` accessor                                              |
| `packages/core/nros-node/src/executor/mod.rs`    | Re-export new public types                                                                                              |
| `packages/core/nros-node/src/lib.rs`             | Re-export new types from executor module                                                                                |

## Verification

1. `just quality` — full format + clippy + nextest + miri + QEMU
2. Existing integration tests pass unchanged (default `Trigger::Any` = no regression)
3. New unit tests for all trigger variants, invocation modes, and handle set ops
4. Kani bounded model checking on trigger evaluation and HandleSet
