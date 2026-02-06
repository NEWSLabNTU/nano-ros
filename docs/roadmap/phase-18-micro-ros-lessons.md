# Phase 18: Micro-ROS Lessons — Executor, Lifecycle & Transport Improvements

**Status**: IN PROGRESS (18.1, 18.2 complete)
**Priority**: MEDIUM-HIGH
**Goal**: Adopt high-impact patterns from micro-ROS (rclc) to improve nano-ros executor determinism, add lifecycle node support, and expand transport options

## Overview

After a comprehensive study of the micro-ROS ecosystem (see [docs/micro-ros-comparison.md](../micro-ros-comparison.md)), we identified several features that would materially improve nano-ros for production embedded use. This phase focuses on the five highest-impact items:

1. **Executor trigger conditions** — deterministic callback scheduling for sensor fusion
2. **`spin_period()`** — fixed-rate periodic execution
3. **Lifecycle nodes** — managed node state machine (ROS 2 standard)
4. **Serial/UART transport** — enables MCUs without networking hardware
5. **Compile-time entity limits** — optional bounds for safety-critical certification

### Reference Implementations

- `external/rclrs/` — rclrs 0.7.0 (Rust ROS 2 client library)
- `external/micro-ros-rclc/` — rclc executor, lifecycle, parameter server
- `external/Micro-XRCE-DDS-Client/` — transport abstraction patterns
- `external/micro_ros_zephyr_module/` — Zephyr serial/USB transport

### Design Principles

- All features must remain `no_std` compatible
- No new heap allocations in steady-state execution paths
- **Rust API follows rclrs conventions**: builder patterns via `Into*Options` traits, `SpinOptions` for spin control, idiomatic Rust (generics, traits, RAII)
- **C API follows rclc conventions**: `nano_ros_*` prefix mirroring `rclc_*` signatures, function-pointer callbacks, `void *context` pattern, `nano_ros_ret_t` return codes
- Both APIs expose the same feature set; neither is a subset of the other
- Serial transport is additive — does not replace zenoh for data plane

### API Alignment Reference

| Concept            | rclrs 0.7.0            | rclc                          | nano-ros Rust   | nano-ros C                        |
|--------------------|------------------------|-------------------------------|-----------------|-----------------------------------|
| Trigger conditions | N/A                    | `rclc_executor_set_trigger()` | `set_trigger()` | `nano_ros_executor_set_trigger()` |
| Periodic spin      | N/A                    | `rclc_executor_spin_period()` | `spin_period()` | `nano_ros_executor_spin_period()` |
| Lifecycle nodes    | N/A                    | `rclc_lifecycle_*`            | `LifecycleNode` | `nano_ros_lifecycle_*`            |
| Entity options     | `IntoPrimitiveOptions` | CMake defines                 | const generics  | `#define` constants               |

**Note:** rclrs 0.7.0 does not implement trigger conditions, `spin_period`, or lifecycle nodes. These features originate from rclc. The nano-ros Rust API defines idiomatic Rust equivalents inspired by rclc, following rclrs builder-pattern and trait conventions where applicable.

---

## 18.1 Executor Trigger Conditions

### Background

rclc provides configurable trigger conditions that control *when* the executor processes callbacks during `spin_some()`. nano-ros currently uses implicit `trigger_any` semantics (process whenever any handle is ready).

**rclc trigger modes** (`external/micro-ros-rclc/rclc/include/rclc/executor.h`):
- `rclc_executor_trigger_any` — process when ANY handle is ready (current nano-ros behavior)
- `rclc_executor_trigger_all` — process only when ALL handles are ready (sensor fusion)
- `rclc_executor_trigger_always` — process unconditionally every cycle
- `rclc_executor_trigger_one` — process only when a specific handle is ready
- Custom: `bool (*rclc_executor_trigger_t)(rclc_executor_handle_t *, unsigned int, void *)`

### 18.1.1 Trigger Types

- [x] Define trigger condition types in `nano-ros-node/src/trigger.rs`
- [x] Add `TriggerCondition` enum for built-in modes
- [x] Add `TriggerFn` type alias for custom predicates (`no_std` compatible)
- [x] Re-export from `nano-ros` unified crate

```rust
/// Built-in trigger conditions (matches rclc trigger modes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriggerCondition {
    /// Process when any handle has data (default, matches rclc_executor_trigger_any)
    #[default]
    Any,
    /// Process only when all registered handles have data (rclc_executor_trigger_all)
    All,
    /// Process unconditionally every spin cycle (rclc_executor_trigger_always)
    Always,
    /// Process only when a specific handle index has data (rclc_executor_trigger_one)
    One(HandleIndex),
}

/// Handle index for trigger_one mode
pub type HandleIndex = usize;

/// Custom trigger predicate — no_std compatible function pointer.
///
/// Arguments: (ready_mask, handle_count) -> should_process
/// The ready_mask slice has one bool per registered handle, true if data available.
pub type TriggerFn = fn(&[bool]) -> bool;

/// Configured trigger for an executor
pub enum Trigger {
    /// Built-in trigger condition
    Builtin(TriggerCondition),
    /// User-defined predicate function (no_std: fn pointer, no allocation)
    Custom(TriggerFn),
}
```

### 18.1.2 Rust API

- [x] Add `set_trigger()` method to `PollingExecutor`
- [x] Add `set_trigger()` method to `BasicExecutor`
- [x] Modify `spin_once()` to check trigger condition before processing callbacks
- [x] Implement ready-mask collection (scan handles without invoking callbacks)

**PollingExecutor (no_std):**
```rust
impl<const MAX_NODES: usize> PollingExecutor<MAX_NODES> {
    /// Set a built-in trigger condition.
    pub fn set_trigger(&mut self, condition: TriggerCondition) { ... }

    /// Set a custom trigger predicate (function pointer, no allocation).
    pub fn set_custom_trigger(&mut self, trigger: TriggerFn) { ... }
}
```

**BasicExecutor (std):**
```rust
impl BasicExecutor {
    /// Set a built-in trigger condition.
    pub fn set_trigger(&mut self, condition: TriggerCondition) { ... }

    /// Set a custom trigger predicate (function pointer).
    pub fn set_custom_trigger(&mut self, trigger: TriggerFn) { ... }

    /// Set a custom trigger predicate (closure, requires alloc).
    pub fn set_trigger_fn(&mut self, trigger: impl Fn(&[bool]) -> bool + Send + 'static) { ... }
}
```

**Usage example:**
```rust
let context = Context::from_env()?;
let mut executor = context.create_basic_executor();

// Sensor fusion: only process when both IMU and LIDAR have data
executor.set_trigger(TriggerCondition::All);

let mut node = executor.create_node("fusion")?;
let _imu_sub = node.create_subscription::<Imu>("imu/data", |msg| { ... })?;
let _lidar_sub = node.create_subscription::<LaserScan>("scan", |msg| { ... })?;

executor.spin(SpinOptions::new())?;
```

### 18.1.3 C API

- [x] Add `nano_ros_executor_set_trigger()` matching rclc signature
- [x] Add built-in trigger functions matching rclc naming
- [x] Add trigger function pointer typedef

```c
/// Trigger function pointer type (matches rclc_executor_trigger_t)
typedef bool (*nano_ros_executor_trigger_t)(
    const nano_ros_executor_handle_t *handles,
    size_t handle_count,
    void *context);

/// Set executor trigger condition (matches rclc_executor_set_trigger)
nano_ros_ret_t nano_ros_executor_set_trigger(
    nano_ros_executor_t *executor,
    nano_ros_executor_trigger_t trigger_function,
    void *trigger_object);

/// Built-in triggers (match rclc_executor_trigger_*)
bool nano_ros_executor_trigger_any(
    const nano_ros_executor_handle_t *handles, size_t size, void *obj);
bool nano_ros_executor_trigger_all(
    const nano_ros_executor_handle_t *handles, size_t size, void *obj);
bool nano_ros_executor_trigger_one(
    const nano_ros_executor_handle_t *handles, size_t size, void *obj);
bool nano_ros_executor_trigger_always(
    const nano_ros_executor_handle_t *handles, size_t size, void *obj);
```

**C usage example (mirrors rclc):**
```c
nano_ros_executor_t executor = nano_ros_executor_get_zero_initialized();
nano_ros_executor_init(&executor, &support, 4);

// Sensor fusion: trigger only when all handles have data
nano_ros_executor_set_trigger(&executor,
    nano_ros_executor_trigger_all, NULL);

// trigger_one: pass handle pointer as trigger_object
nano_ros_executor_set_trigger(&executor,
    nano_ros_executor_trigger_one, &my_subscription);
```

### 18.1.4 Tests

- [x] Unit test: `TriggerCondition::All` blocks until all subscriptions have data
- [x] Unit test: `TriggerCondition::One` only fires when target handle is ready
- [x] Unit test: custom `TriggerFn` with user predicate
- [x] Unit test: `TriggerCondition::Always` fires even with no data
- [x] C API test: `nano_ros_executor_trigger_all` matches behavior
- [x] Integration test: sensor fusion scenario (two synchronized topics)

---

## 18.2 Periodic Spin (`spin_period`)

### Background

rclc provides `rclc_executor_spin_period()` and `rclc_executor_spin_one_period()` for fixed-rate control loops. The executor spins at a precise rate, compensating for processing time.

**rclc reference** (`external/micro-ros-rclc/rclc/src/rclc/executor.c`):
```c
rcl_ret_t rclc_executor_spin_one_period(rclc_executor_t *executor, const uint64_t period) {
    // Record invocation_time on first call
    // spin_some(executor, timeout_ns)
    // sleep_time = (invocation_time + period) - current_time
    // if (sleep_time > 0) rclc_sleep_ms(sleep_time / 1000000)
    // invocation_time += period  (accumulate for drift compensation)
}
```

**Current state:** The C API already implements `nano_ros_executor_spin_period()` in `nano-ros-c/src/executor.rs`. The Rust API (`BasicExecutor`, `PollingExecutor`) does not yet expose periodic spin.

### 18.2.1 Rust API — BasicExecutor

- [x] Add `spin_period()` to `BasicExecutor`
- [x] Add `spin_one_period()` for single-iteration variant (useful for testing)

```rust
impl BasicExecutor {
    /// Spin at a fixed rate, compensating for processing time.
    /// Blocks until halt flag is set or error occurs.
    ///
    /// Uses wall-clock time to maintain the target rate, compensating
    /// for callback processing time (same approach as rclc_executor_spin_period).
    pub fn spin_period(&mut self, period: Duration) -> Result<(), RclrsError> { ... }

    /// Execute one period: spin_once + sleep for remainder of period.
    /// Returns the spin result and whether the period was fully utilized.
    pub fn spin_one_period(&mut self, period: Duration) -> SpinPeriodResult { ... }
}

/// Result from a single period of execution
#[derive(Debug, Clone)]
pub struct SpinPeriodResult {
    /// Work performed during this period
    pub work: SpinOnceResult,
    /// Whether processing exceeded the period (overrun)
    pub overrun: bool,
    /// Actual processing time
    pub elapsed: Duration,
}
```

### 18.2.2 Rust API — PollingExecutor (no_std)

- [x] Add `spin_one_period()` to `PollingExecutor`
- [x] For `no_std`: caller provides elapsed time, executor returns remaining sleep time

```rust
impl<const MAX_NODES: usize> PollingExecutor<MAX_NODES> {
    /// Process one period. Returns remaining time in ms that the caller should sleep.
    /// This is no_std compatible — the caller is responsible for the actual delay.
    ///
    /// Typical usage in a bare-metal main loop:
    /// ```rust,no_run
    /// loop {
    ///     let start = platform_time_ms();
    ///     let result = executor.spin_one_period(period_ms, elapsed_ms);
    ///     if result.remaining_ms > 0 {
    ///         platform_sleep_ms(result.remaining_ms);
    ///     }
    ///     elapsed_ms = platform_time_ms() - start;
    /// }
    /// ```
    pub fn spin_one_period(&mut self, period_ms: u64, elapsed_ms: u64) -> SpinPeriodPollingResult {
        let result = self.spin_once(elapsed_ms);
        SpinPeriodPollingResult {
            work: result,
            remaining_ms: period_ms.saturating_sub(elapsed_ms),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SpinPeriodPollingResult {
    pub work: SpinOnceResult,
    pub remaining_ms: u64,
}
```

### 18.2.3 C API

The C API already implements `nano_ros_executor_spin_period()`. Remaining work:

- [x] Add `nano_ros_executor_spin_one_period()` (single iteration, matches rclc)
- [x] Improve drift compensation to match rclc's accumulating invocation_time pattern

```c
/// Already implemented in nano-ros-c/src/executor.rs:
nano_ros_ret_t nano_ros_executor_spin_period(
    nano_ros_executor_t *executor,
    uint64_t period_ns);

/// New: single-period variant (matches rclc_executor_spin_one_period)
nano_ros_ret_t nano_ros_executor_spin_one_period(
    nano_ros_executor_t *executor,
    uint64_t period_ns);
```

**C usage example:**
```c
// Fixed 100Hz control loop
nano_ros_executor_spin_period(&executor, 10000000ULL);  // 10ms = 100Hz

// Or manual single-period control:
while (running) {
    nano_ros_executor_spin_one_period(&executor, 10000000ULL);
}
```

### 18.2.4 Tests

- [x] Unit test: `spin_period` maintains target rate within 10% tolerance
- [x] Unit test: `spin_one_period` returns correct remaining time / overrun flag
- [x] Unit test: drift compensation over 100+ iterations
- [x] C API test: `nano_ros_executor_spin_one_period` single iteration
- [ ] Example: fixed 100Hz control loop (Rust + C) (deferred to examples)

---

## 18.3 Lifecycle Nodes

### Background

ROS 2 lifecycle nodes (REP-2002) provide a managed state machine for deterministic startup/shutdown. micro-ROS implements this in `rclc_lifecycle` (`external/micro-ros-rclc/rclc_lifecycle/`).

**Neither rclrs 0.7.0 nor nano-ros currently implements lifecycle nodes.** This is a new feature for both Rust and C APIs, with rclc_lifecycle as the primary reference.

**State machine:**
```
                ┌──────────────┐
                │ Unconfigured │◄──────────────┐
                └──────┬───────┘               │
                       │ configure()           │ cleanup()
                       ▼                       │
                ┌──────────────┐               │
         ┌─────│   Inactive   │───────────────┘
         │      └──────┬───────┘
         │             │ activate()
         │             ▼
         │      ┌──────────────┐
         │      │    Active    │
         │      └──────┬───────┘
         │             │ deactivate()
         │             ▼
         │      ┌──────────────┐
         └──────│   Inactive   │
                └──────┬───────┘
                       │ shutdown()
                       ▼
                ┌──────────────┐
                │  Finalized   │
                └──────────────┘
```

**States:** Unconfigured, Inactive, Active, Finalized, ErrorProcessing
**Transitions:** configure, activate, deactivate, cleanup, shutdown, error_recovery

### 18.3.1 Lifecycle State Types

- [x] Create `crates/nano-ros-core/src/lifecycle.rs`
- [x] Define `LifecycleState` enum (shared by Rust and C APIs)
- [x] Define `LifecycleTransition` enum
- [x] Define `TransitionResult` matching rclc callback return convention
- [x] Re-export from `nano-ros` unified crate

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LifecycleState {
    Unconfigured = 1,
    Inactive = 2,
    Active = 3,
    Finalized = 4,
    ErrorProcessing = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LifecycleTransition {
    Configure = 1,
    Activate = 2,
    Deactivate = 3,
    Cleanup = 4,
    Shutdown = 5,
    ErrorRecovery = 6,
}

/// Callback return value (matches rclc convention: 0 = success)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionResult {
    Success,
    Failure,
    Error,
}
```

### 18.3.2 Rust API — LifecycleNode (std)

- [x] Create `crates/nano-ros-node/src/lifecycle.rs`
- [x] `LifecycleNode` wraps a `NodeHandle` and adds state machine
- [ ] Builder pattern via `LifecycleNodeOptions` / `IntoLifecycleNodeOptions` (deferred — not needed for core functionality)
- [x] Callback registration via builder (before construction) or setter (after)
- [x] `LifecycleNode` exposes the inner `NodeHandle` for creating publishers/subscriptions
- [x] Integrate with executor: lifecycle node registers as a regular node

```rust
/// Options for creating a lifecycle node (follows rclrs IntoNodeOptions pattern)
pub struct LifecycleNodeOptions<'a> {
    pub node_options: NodeOptions<'a>,
    pub enable_communication_interface: bool,
}

pub trait IntoLifecycleNodeOptions<'a>: Sized {
    fn into_lifecycle_node_options(self) -> LifecycleNodeOptions<'a>;
    fn enable_communication_interface(self, enable: bool) -> LifecycleNodeOptions<'a>;
}

// &str automatically provides lifecycle node options (inherits IntoNodeOptions)
impl<'a> IntoLifecycleNodeOptions<'a> for &'a str { ... }
impl<'a> IntoLifecycleNodeOptions<'a> for NodeOptions<'a> { ... }

/// Managed lifecycle node (std, uses Box callbacks)
pub struct LifecycleNode<'a> {
    node: NodeHandle<'a>,
    state: LifecycleState,
    on_configure: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
    on_activate: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
    on_deactivate: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
    on_cleanup: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
    on_shutdown: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
    on_error: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
}

impl<'a> LifecycleNode<'a> {
    /// Current lifecycle state
    pub fn state(&self) -> LifecycleState { ... }

    /// Trigger a state transition, invoking the registered callback.
    /// Returns the new state on success, or error if transition is invalid.
    pub fn trigger_transition(&mut self, transition: LifecycleTransition)
        -> Result<LifecycleState, LifecycleError> { ... }

    /// Access the inner NodeHandle to create publishers, subscriptions, etc.
    pub fn node(&self) -> &NodeHandle<'a> { ... }

    /// Mutable access to the inner NodeHandle
    pub fn node_mut(&mut self) -> &mut NodeHandle<'a> { ... }

    /// Register callbacks (can also be set via builder before creation)
    pub fn set_on_configure(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) { ... }
    pub fn set_on_activate(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) { ... }
    pub fn set_on_deactivate(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) { ... }
    pub fn set_on_cleanup(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) { ... }
    pub fn set_on_shutdown(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) { ... }
    pub fn set_on_error(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) { ... }
}
```

**Creation through executor (follows rclrs pattern):**
```rust
impl BasicExecutor {
    /// Create a lifecycle-managed node (mirrors create_node pattern)
    pub fn create_lifecycle_node<'a, 'b>(
        &'a mut self,
        options: impl IntoLifecycleNodeOptions<'b>,
    ) -> Result<LifecycleNode<'a>, RclrsError> { ... }
}
```

**Usage example:**
```rust
let context = Context::from_env()?;
let mut executor = context.create_basic_executor();

let mut lifecycle_node = executor.create_lifecycle_node(
    "sensor_driver".namespace("/hw")
)?;

lifecycle_node.set_on_configure(|| {
    println!("Configuring sensor...");
    TransitionResult::Success
});
lifecycle_node.set_on_activate(|| {
    println!("Sensor activated");
    TransitionResult::Success
});

// Create publishers/subscriptions on the inner node
let publisher = lifecycle_node.node_mut()
    .create_publisher::<SensorData>("sensor/data")?;

// Drive state transitions
lifecycle_node.trigger_transition(LifecycleTransition::Configure)?;
lifecycle_node.trigger_transition(LifecycleTransition::Activate)?;
assert_eq!(lifecycle_node.state(), LifecycleState::Active);

executor.spin(SpinOptions::new())?;
```

### 18.3.3 Rust API — LifecyclePollingNode (no_std)

- [x] Implement `LifecyclePollingNode` without `Box` callbacks
- [x] Use function pointers instead of closures (matches `SubscriptionCallback` / `ExecutorTimerCallback` duality)
- [x] State transitions driven by user code (no services)

```rust
/// Lifecycle callback function pointer (no_std compatible)
/// Returns TransitionResult. Matches rclc convention: int (*cb)(void)
pub type LifecycleCallbackFn = fn() -> TransitionResult;

/// Managed lifecycle node for no_std (uses function pointers)
pub struct LifecyclePollingNode<'a> {
    node: NodeHandle<'a>,
    state: LifecycleState,
    on_configure: Option<LifecycleCallbackFn>,
    on_activate: Option<LifecycleCallbackFn>,
    on_deactivate: Option<LifecycleCallbackFn>,
    on_cleanup: Option<LifecycleCallbackFn>,
    on_shutdown: Option<LifecycleCallbackFn>,
    on_error: Option<LifecycleCallbackFn>,
}

impl<'a> LifecyclePollingNode<'a> {
    /// Same API as LifecycleNode but with fn pointers
    pub fn state(&self) -> LifecycleState { ... }
    pub fn trigger_transition(&mut self, transition: LifecycleTransition)
        -> Result<LifecycleState, LifecycleError> { ... }
    pub fn node(&self) -> &NodeHandle<'a> { ... }
    pub fn node_mut(&mut self) -> &mut NodeHandle<'a> { ... }

    pub fn set_on_configure(&mut self, cb: LifecycleCallbackFn) { ... }
    pub fn set_on_activate(&mut self, cb: LifecycleCallbackFn) { ... }
    pub fn set_on_deactivate(&mut self, cb: LifecycleCallbackFn) { ... }
    pub fn set_on_cleanup(&mut self, cb: LifecycleCallbackFn) { ... }
    pub fn set_on_shutdown(&mut self, cb: LifecycleCallbackFn) { ... }
    pub fn set_on_error(&mut self, cb: LifecycleCallbackFn) { ... }
}
```

**Creation through PollingExecutor:**
```rust
impl<const MAX_NODES: usize> PollingExecutor<MAX_NODES> {
    pub fn create_lifecycle_node<'a, 'b>(
        &'a mut self,
        options: impl IntoLifecycleNodeOptions<'b>,
    ) -> Result<LifecyclePollingNode<'a>, RclrsError> { ... }
}
```

### 18.3.4 Lifecycle Services (Optional, requires `alloc` + `zenoh`)

- [ ] Register `~/change_state` service on the lifecycle node
- [ ] Register `~/get_state` service on the lifecycle node
- [ ] Register `~/get_available_transitions` service on the lifecycle node
- [ ] Uses `lifecycle_msgs/srv/ChangeState`, `GetState`, `GetAvailableTransitions`
- [ ] Enabled by `enable_communication_interface` option (matches rclc)

```rust
// Enabled by default when alloc+zenoh available:
let lifecycle_node = executor.create_lifecycle_node(
    "sensor".enable_communication_interface(true)
)?;

// Then from ROS 2 CLI:
// ros2 lifecycle set /sensor configure
// ros2 lifecycle get /sensor
```

### 18.3.5 C API

- [x] Add `nano_ros_lifecycle_state_machine_t` struct
- [x] Match rclc_lifecycle function signatures
- [x] Callback registration via `nano_ros_lifecycle_register_on_*()` (matches rclc)
- [x] State transition via `nano_ros_lifecycle_change_state()` (matches rclc)

```c
/// Lifecycle callback type (matches rclc: int (*cb)(void))
typedef int (*nano_ros_lifecycle_callback_t)(void);

/// Lifecycle node (wraps a regular node, matches rclc_lifecycle_node_t)
typedef struct nano_ros_lifecycle_node_t {
    nano_ros_node_t *node;
    uint8_t state;              /* LifecycleState as u8 */
    bool publish_transitions;
    void *_internal;            /* Opaque Rust lifecycle state */
} nano_ros_lifecycle_node_t;

/// Convert a regular node into a lifecycle node
/// (matches rclc_make_node_a_lifecycle_node)
nano_ros_ret_t nano_ros_make_node_a_lifecycle_node(
    nano_ros_lifecycle_node_t *lifecycle_node,
    nano_ros_node_t *node,
    bool enable_communication_interface);

/// Trigger a state transition (matches rclc_lifecycle_change_state)
nano_ros_ret_t nano_ros_lifecycle_change_state(
    nano_ros_lifecycle_node_t *lifecycle_node,
    unsigned int transition_id,
    bool publish_update);

/// Register transition callbacks (match rclc_lifecycle_register_on_*)
nano_ros_ret_t nano_ros_lifecycle_register_on_configure(
    nano_ros_lifecycle_node_t *node, nano_ros_lifecycle_callback_t cb);
nano_ros_ret_t nano_ros_lifecycle_register_on_activate(
    nano_ros_lifecycle_node_t *node, nano_ros_lifecycle_callback_t cb);
nano_ros_ret_t nano_ros_lifecycle_register_on_deactivate(
    nano_ros_lifecycle_node_t *node, nano_ros_lifecycle_callback_t cb);
nano_ros_ret_t nano_ros_lifecycle_register_on_cleanup(
    nano_ros_lifecycle_node_t *node, nano_ros_lifecycle_callback_t cb);
nano_ros_ret_t nano_ros_lifecycle_register_on_shutdown(
    nano_ros_lifecycle_node_t *node, nano_ros_lifecycle_callback_t cb);

/// Query current state
uint8_t nano_ros_lifecycle_get_state(
    const nano_ros_lifecycle_node_t *node);

/// Finalize lifecycle node
nano_ros_ret_t nano_ros_lifecycle_node_fini(
    nano_ros_lifecycle_node_t *node);
```

**C usage example (mirrors rclc_lifecycle):**
```c
nano_ros_node_t node;
nano_ros_node_init(&node, &support, "sensor", "/hw");

nano_ros_lifecycle_node_t lifecycle_node;
nano_ros_make_node_a_lifecycle_node(&lifecycle_node, &node, true);

nano_ros_lifecycle_register_on_configure(&lifecycle_node, my_on_configure);
nano_ros_lifecycle_register_on_activate(&lifecycle_node, my_on_activate);

nano_ros_lifecycle_change_state(&lifecycle_node,
    NANO_ROS_LIFECYCLE_TRANSITION_CONFIGURE, true);
nano_ros_lifecycle_change_state(&lifecycle_node,
    NANO_ROS_LIFECYCLE_TRANSITION_ACTIVATE, true);
```

### 18.3.6 Tests

- [x] Unit test: valid transition sequence (unconfigured -> inactive -> active -> finalized)
- [x] Unit test: invalid transition rejected (active -> configure fails)
- [x] Unit test: error recovery path (active -> error -> unconfigured)
- [x] Unit test: callback invocation on transitions
- [x] Unit test: `LifecyclePollingNode` with fn pointers (no_std)
- [x] C API test: `nano_ros_lifecycle_change_state` with callback
- [ ] Integration test: lifecycle node with ROS 2 `ros2 lifecycle` CLI (requires `alloc` + services, deferred with 18.3.4)

---

## 18.4 Serial/UART Transport

### Background

micro-ROS's most-used transport on MCUs is UART serial. Many embedded boards lack Ethernet/WiFi but always have UART. Adding serial transport to nano-ros would enable these platforms.

**Important**: Serial transport does NOT replace zenoh. It provides a point-to-point link between an MCU and a host running a serial bridge. The bridge translates serial frames into zenoh messages.

**Architecture:**
```
┌──────────────┐  UART  ┌──────────────────┐  Zenoh  ┌──────────────┐
│     MCU      │◄──────►│  Serial Bridge   │◄───────►│   zenohd     │
│  nano-ros    │        │  (host process)  │         │  + ROS 2     │
│  serial shim │        │  serial <-> zenoh│         │              │
└──────────────┘        └──────────────────┘         └──────────────┘
```

**micro-ROS reference** (`external/micro_ros_zephyr_module/`):
- Ring buffer pattern (2KB) for interrupt-driven RX
- Framing protocol for message boundaries
- Custom transport callbacks via `rmw_uros_set_custom_transport()`

### 18.4.1 Serial Framing Protocol

- [ ] Design framing protocol (length-prefixed with CRC) for reliable message boundaries
- [ ] Implement frame encoder/decoder in `crates/nano-ros-transport/src/serial/`
- [ ] Support variable-length messages up to MTU
- [ ] CRC-16 for error detection

```rust
/// Serial frame format:
/// [START_BYTE(0xAA)] [LENGTH_HI] [LENGTH_LO] [TOPIC_ID_HI] [TOPIC_ID_LO] [PAYLOAD...] [CRC_HI] [CRC_LO]
pub struct SerialFrame<'a> {
    pub topic_id: u16,
    pub payload: &'a [u8],
}

pub struct SerialFramer {
    rx_buf: [u8; MAX_FRAME_SIZE],
    rx_pos: usize,
    state: FrameState,
}

impl SerialFramer {
    /// Feed a received byte, returns a complete frame when available
    pub fn feed(&mut self, byte: u8) -> Option<SerialFrame<'_>> { ... }

    /// Encode a frame into an output buffer
    pub fn encode(frame: &SerialFrame<'_>, out: &mut [u8]) -> Result<usize, SerialError> { ... }
}
```

### 18.4.2 Rust API — Transport Configuration

- [ ] Add `SerialTransport` variant to `TransportConfig`
- [ ] Implement `Session`, `Publisher`, `Subscriber` traits for serial backend
- [ ] Topic multiplexing: map topic names to 16-bit IDs within serial frames
- [ ] Request/reply correlation for services

```rust
/// Extended TransportConfig with serial option
pub enum TransportConfig<'a> {
    /// Existing: connect to zenoh router
    Zenoh {
        locator: &'a str,
        mode: SessionMode,
    },
    /// New: serial point-to-point link
    Serial {
        /// Platform-specific serial port configuration
        port: SerialPortConfig<'a>,
        /// Baud rate (default: 115200)
        baud_rate: u32,
    },
}

/// Platform-specific serial port identifier
pub enum SerialPortConfig<'a> {
    /// POSIX: path to device (e.g., "/dev/ttyUSB0")
    #[cfg(feature = "posix")]
    Device(&'a str),
    /// Zephyr: UART device binding name (e.g., "uart0")
    #[cfg(feature = "zephyr")]
    ZephyrUart(&'a str),
}
```

**Rust usage example:**
```rust
let context = Context::new(InitOptions::new()
    .transport(TransportConfig::Serial {
        port: SerialPortConfig::Device("/dev/ttyUSB0"),
        baud_rate: 115200,
    }))?;

let mut executor = context.create_basic_executor();
let mut node = executor.create_node("mcu_node")?;

// API is identical regardless of transport
let publisher = node.create_publisher::<Int32>("sensor/data")?;
publisher.publish(&Int32 { data: 42 })?;
```

### 18.4.3 C API — Serial Transport

- [ ] Add serial variant to `nano_ros_support_init` or new init function
- [ ] Transport selection transparent to publisher/subscription C API

```c
/// Initialize support with serial transport
/// (alternative to nano_ros_support_init which uses zenoh)
nano_ros_ret_t nano_ros_support_init_serial(
    nano_ros_support_t *support,
    const char *port,       /* e.g., "/dev/ttyUSB0" or "uart0" (Zephyr) */
    uint32_t baud_rate,
    uint8_t domain_id);

/// Check if support uses serial transport
c_int nano_ros_support_is_serial(const nano_ros_support_t *support);
```

**C usage example:**
```c
nano_ros_support_t support = nano_ros_support_get_zero_initialized();
nano_ros_support_init_serial(&support, "uart0", 115200, 0);

// All other API calls are identical regardless of transport
nano_ros_node_t node;
nano_ros_node_init(&node, &support, "mcu_node", "");

nano_ros_publisher_t pub;
nano_ros_publisher_init_default(&pub, &node, &type_info, "sensor/data");
nano_ros_publish_raw(&pub, data, data_len);
```

### 18.4.4 Zephyr Serial Backend

- [ ] Implement UART driver using Zephyr UART async/interrupt API
- [ ] Interrupt-driven RX with ring buffer (2KB, matches micro-ROS)
- [ ] Polling TX (synchronous writes)
- [ ] Kconfig integration: `CONFIG_NANO_ROS_TRANSPORT_SERIAL`

```kconfig
config NANO_ROS_TRANSPORT_SERIAL
    bool "Enable serial/UART transport"
    default n
    depends on SERIAL
    help
      Enable serial transport for nano-ros. Uses UART for
      point-to-point communication with a host serial bridge.

config NANO_ROS_SERIAL_UART_DEVICE
    string "UART device name"
    default "uart0"
    depends on NANO_ROS_TRANSPORT_SERIAL

config NANO_ROS_SERIAL_BAUD_RATE
    int "Serial baud rate"
    default 115200
    depends on NANO_ROS_TRANSPORT_SERIAL
```

### 18.4.5 POSIX Serial Backend (for testing)

- [ ] Implement over `/dev/ttyUSB*` or pseudo-terminals (`/dev/pts/*`)
- [ ] Use for host-side testing without hardware
- [ ] Pseudo-terminal pair creation for integration tests

### 18.4.6 Host-Side Serial Bridge

- [ ] Create `tools/serial-bridge/` — standalone Rust binary
- [ ] Reads serial frames from UART, publishes to zenoh
- [ ] Subscribes to zenoh topics, sends as serial frames to MCU
- [ ] Configuration: serial port, baud rate, zenoh locator
- [ ] Topic ID <-> topic name mapping (negotiated or configured)

```bash
# Usage:
serial-bridge --port /dev/ttyUSB0 --baud 115200 --zenoh tcp/127.0.0.1:7447
```

### 18.4.7 Tests

- [ ] Unit test: framing encode/decode roundtrip
- [ ] Unit test: CRC error detection
- [ ] Unit test: partial frame reassembly (byte-at-a-time feed)
- [ ] Integration test: serial pub/sub via pseudo-terminal pair
- [ ] Integration test: serial bridge end-to-end with zenohd
- [ ] C API test: `nano_ros_support_init_serial` + publish/subscribe

---

## 18.5 Compile-Time Entity Limits

### Background

micro-ROS enforces compile-time limits on entity counts (`RMW_UXRCE_MAX_NODES`, `RMW_UXRCE_MAX_PUBLISHERS`, etc.) via CMake/Kconfig. This ensures predictable memory usage and is required for safety-critical certification (e.g., ISO 26262).

**Current state:** nano-ros already uses const generics in several places:
- `PollingExecutor<const MAX_NODES: usize = 4>` — max nodes per executor
- `NodeHandle<const MAX_TOKENS: usize = 16, const MAX_TIMERS: usize, const MAX_SUBS: usize>` — max publishers+subscribers (tokens), timers, and subscriptions per node

This task unifies and extends these limits for consistency, and exposes them in the C API.

### 18.5.1 Rust API — Unified Const Generic Limits

- [ ] Reconcile existing const generics into a consistent naming scheme
- [ ] Add `MAX_SERVICES` to `NodeHandle` (currently missing)
- [ ] Add runtime capacity query methods
- [ ] Document default values and how to override

```rust
/// PollingExecutor with configurable node limit
pub struct PollingExecutor<const MAX_NODES: usize = 4> { ... }

/// NodeHandle with per-entity limits
/// (extends existing MAX_TOKENS/MAX_TIMERS/MAX_SUBS with MAX_SERVICES)
pub struct NodeHandle<
    'a,
    const MAX_TOKENS: usize = 16,       // publishers + subscribers (zenoh sessions)
    const MAX_TIMERS: usize = 8,
    const MAX_SUBS: usize = 8,          // subscription callbacks
    const MAX_SERVICES: usize = 4,      // service server callbacks
> { ... }

impl<'a, const T: usize, const TI: usize, const S: usize, const SV: usize>
    NodeHandle<'a, T, TI, S, SV>
{
    /// Remaining publisher/subscriber token capacity
    pub fn remaining_token_capacity(&self) -> usize { ... }
    /// Remaining timer capacity
    pub fn remaining_timer_capacity(&self) -> usize { ... }
    /// Remaining subscription callback capacity
    pub fn remaining_subscription_capacity(&self) -> usize { ... }
    /// Remaining service capacity
    pub fn remaining_service_capacity(&self) -> usize { ... }
}
```

**Usage with custom limits:**
```rust
// Default limits (most users)
let mut executor = context.create_polling_executor_default();

// Custom limits for resource-constrained system
let mut executor: PollingExecutor<2> = context.create_polling_executor();
let mut node: NodeHandle<'_, 4, 2, 2, 1> = executor.create_node("minimal")?;
```

### 18.5.2 C API — Entity Limits

- [ ] Expose limits as `#define` constants in C header
- [ ] Return `NANO_ROS_RET_NO_MEMORY` when limits exceeded
- [ ] Document limits in C API header comments

```c
/* Default entity limits — override via build system or Kconfig */
#ifndef NANO_ROS_MAX_NODES
#define NANO_ROS_MAX_NODES              4
#endif
#ifndef NANO_ROS_MAX_PUBLISHERS
#define NANO_ROS_MAX_PUBLISHERS         8
#endif
#ifndef NANO_ROS_MAX_SUBSCRIPTIONS
#define NANO_ROS_MAX_SUBSCRIPTIONS      8
#endif
#ifndef NANO_ROS_MAX_SERVICES
#define NANO_ROS_MAX_SERVICES           4
#endif
#ifndef NANO_ROS_MAX_TIMERS
#define NANO_ROS_MAX_TIMERS             8
#endif
#ifndef NANO_ROS_EXECUTOR_MAX_HANDLES
#define NANO_ROS_EXECUTOR_MAX_HANDLES   16
#endif

/// Query remaining capacity
int nano_ros_executor_get_remaining_handles(const nano_ros_executor_t *executor);
int nano_ros_node_get_remaining_publishers(const nano_ros_node_t *node);
int nano_ros_node_get_remaining_subscriptions(const nano_ros_node_t *node);
```

### 18.5.3 Kconfig Integration (Zephyr)

- [ ] Add Kconfig entries for entity limits in BSP Zephyr
- [ ] Wire Kconfig values to Rust const generics via `build.rs`
- [ ] Wire Kconfig values to C `#define` constants via generated header

```kconfig
menu "nano-ros Entity Limits"

config NANO_ROS_MAX_NODES
    int "Maximum number of nodes"
    default 4

config NANO_ROS_MAX_PUBLISHERS
    int "Maximum number of publishers per node"
    default 8

config NANO_ROS_MAX_SUBSCRIPTIONS
    int "Maximum number of subscriptions per node"
    default 8

config NANO_ROS_MAX_SERVICES
    int "Maximum number of services per node"
    default 4

config NANO_ROS_MAX_TIMERS
    int "Maximum number of timers per node"
    default 8

config NANO_ROS_EXECUTOR_MAX_HANDLES
    int "Maximum executor handles"
    default 16

endmenu
```

### 18.5.4 Tests

- [ ] Compile-time test: exceeding limit produces clear error
- [ ] Unit test: entity creation succeeds up to limit
- [ ] Unit test: entity creation fails at limit with `NANO_ROS_RET_NO_MEMORY`
- [ ] Unit test: `remaining_*_capacity()` returns correct values
- [ ] C API test: `nano_ros_node_get_remaining_publishers` accuracy

---

## Passing Criteria

| Feature              | Rust API Criterion | C API Criterion |
|----------------------|-------------------|-----------------|
| Trigger conditions   | `TriggerCondition::All` blocks until all subs ready; custom `TriggerFn` works | `nano_ros_executor_trigger_all` matches behavior |
| `spin_period()`      | `BasicExecutor::spin_period()` maintains rate within 10% | `nano_ros_executor_spin_one_period()` works |
| Lifecycle nodes      | `LifecycleNode` + `LifecyclePollingNode` full state machine | `nano_ros_lifecycle_change_state()` + callbacks |
| Serial transport     | Pub/sub via `TransportConfig::Serial` over pseudo-terminal | `nano_ros_support_init_serial()` + pub/sub |
| Entity limits        | `NodeHandle` const generics enforced, capacity queries work | `#define` limits respected, capacity queries work |
| `just quality`       | Passes after all changes | Passes after all changes |

## Implementation Order

```
18.1 Trigger conditions ──┐
                          ├── 18.3 Lifecycle nodes ──── 18.4 Serial transport
18.2 spin_period() ───────┘                                      │
                                                                 │
18.5 Entity limits (independent) ────────────────────────────────┘
```

**Rationale:**
- 18.1 and 18.2 are small executor changes, good warmup
- 18.3 builds on executor improvements (lifecycle nodes register with executor)
- 18.4 is the largest item, benefits from stable executor and lifecycle
- 18.5 is independent and can be done in parallel with any other item
