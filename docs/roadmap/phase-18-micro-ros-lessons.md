# Phase 18: Micro-ROS Lessons — Executor, Lifecycle & Transport Improvements

**Status**: PLANNING
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

- `external/micro-ros-rclc/` — rclc executor, lifecycle, parameter server
- `external/Micro-XRCE-DDS-Client/` — transport abstraction patterns
- `external/micro_ros_zephyr_module/` — Zephyr serial/USB transport

### Design Principles

- All features must remain `no_std` compatible
- No new heap allocations in steady-state execution paths
- Rust API uses idiomatic patterns; C API mirrors rclc conventions
- Serial transport is additive — does not replace zenoh for data plane

---

## 18.1 Executor Trigger Conditions

### Background

rclc provides configurable trigger conditions that control *when* the executor processes callbacks during `spin_some()`. nano-ros currently uses implicit `trigger_any` semantics (process whenever any handle is ready).

**rclc trigger modes** (`external/micro-ros-rclc/rclc/include/rclc/executor.h`):
- `trigger_any` — process when ANY handle is ready (current nano-ros behavior)
- `trigger_all` — process only when ALL handles are ready (sensor fusion)
- `trigger_always` — process unconditionally every cycle
- `trigger_one` — process only when a specific handle is ready
- Custom function: `bool (*trigger_fn)(handles[], count, context)`

### Work Items

#### 18.1.1 Trigger Trait and Types

- [ ] Define trigger condition types in `nano-ros-node/src/executor.rs`
- [ ] Add `TriggerCondition` enum for built-in modes
- [ ] Add `CustomTrigger` trait for user-defined predicates

```rust
/// Built-in trigger conditions
pub enum TriggerCondition {
    /// Process when any handle has data (default, matches rclcpp)
    Any,
    /// Process only when all registered handles have data
    All,
    /// Process unconditionally every spin cycle
    Always,
    /// Process only when a specific handle index has data
    One(usize),
}

/// User-defined trigger predicate (no_std compatible)
pub trait TriggerPredicate {
    fn should_process(&self, ready_mask: &[bool]) -> bool;
}
```

#### 18.1.2 Executor Integration

- [ ] Add `set_trigger()` method to `PollingExecutor` and `BasicExecutor`
- [ ] Modify `spin_once()` to check trigger condition before processing callbacks
- [ ] Implement ready-mask collection (scan handles without invoking callbacks)

```rust
impl<const N: usize> PollingExecutor<N> {
    pub fn set_trigger(&mut self, condition: TriggerCondition) { ... }
    pub fn set_custom_trigger<T: TriggerPredicate>(&mut self, trigger: T) { ... }
}
```

#### 18.1.3 C API

- [ ] Add `nano_ros_executor_set_trigger()` to C API
- [ ] Add trigger enum to `nano-ros-c/src/executor.rs`
- [ ] Match rclc function signature for migration ease

```c
nano_ros_ret_t nano_ros_executor_set_trigger(
    nano_ros_executor_t *executor,
    nano_ros_executor_trigger_t trigger,
    void *trigger_context);
```

#### 18.1.4 Tests

- [ ] Unit test: `trigger_all` blocks until all subscriptions have data
- [ ] Unit test: `trigger_one` only fires when target handle is ready
- [ ] Unit test: custom trigger with user predicate
- [ ] Integration test: sensor fusion scenario (IMU + LIDAR synchronized)

---

## 18.2 Periodic Spin (`spin_period`)

### Background

rclc provides `rclc_executor_spin_period()` for fixed-rate control loops. This spins the executor at a precise rate, compensating for processing time. nano-ros has `spin_once()` but requires the user to manage timing manually.

**rclc reference** (`external/micro-ros-rclc/rclc/src/executor.c`):
```c
rcl_ret_t rclc_executor_spin_period(rclc_executor_t *executor, uint64_t period_ns) {
    while (true) {
        int64_t start = now();
        rclc_executor_spin_some(executor, period_ns);
        int64_t elapsed = now() - start;
        int64_t remaining = period_ns - elapsed;
        if (remaining > 0) sleep(remaining);
    }
}
```

### Work Items

#### 18.2.1 Rust API

- [ ] Add `spin_period()` to `BasicExecutor`
- [ ] Add `spin_one_period()` for single-iteration variant (useful for testing)
- [ ] Add period field to `SpinOptions` as alternative entry point

```rust
impl BasicExecutor {
    /// Spin at a fixed rate, compensating for processing time.
    /// Returns when halt flag is set or error occurs.
    pub fn spin_period(&mut self, period: Duration) -> Result<(), RclrsError> { ... }

    /// Execute one period: spin_once + sleep for remainder.
    pub fn spin_one_period(&mut self, period: Duration) -> SpinOnceResult { ... }
}
```

#### 18.2.2 PollingExecutor Support

- [ ] Add `spin_one_period()` to `PollingExecutor` using platform time
- [ ] For `no_std`: accept elapsed time from caller (no sleep, user handles timing)

```rust
impl<const N: usize> PollingExecutor<N> {
    /// Process one period. Returns remaining time in ms that the caller should sleep.
    /// This is no_std compatible — the caller is responsible for the actual delay.
    pub fn spin_one_period(&mut self, period_ms: u64, elapsed_ms: u64) -> SpinPeriodResult {
        let result = self.spin_once(elapsed_ms);
        SpinPeriodResult {
            work: result,
            remaining_ms: period_ms.saturating_sub(elapsed_ms),
        }
    }
}
```

#### 18.2.3 C API

- [ ] Add `nano_ros_executor_spin_period()` matching rclc signature
- [ ] Add `nano_ros_executor_spin_one_period()`

#### 18.2.4 Tests

- [ ] Unit test: `spin_period` maintains target rate within 10% tolerance
- [ ] Unit test: `spin_one_period` returns correct remaining time
- [ ] Example: fixed 100Hz control loop

---

## 18.3 Lifecycle Nodes

### Background

ROS 2 lifecycle nodes (REP-2002) provide a managed state machine for deterministic startup/shutdown. micro-ROS implements this in `rclc_lifecycle` (`external/micro-ros-rclc/rclc_lifecycle/`).

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

### Work Items

#### 18.3.1 Lifecycle State Types

- [ ] Create `crates/nano-ros-core/src/lifecycle.rs`
- [ ] Define `LifecycleState` enum
- [ ] Define `LifecycleTransition` enum
- [ ] Define transition result type

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

pub enum TransitionResult {
    Success,
    Failure,
    Error,
}
```

#### 18.3.2 Lifecycle Node Implementation

- [ ] Create lifecycle node wrapper in `nano-ros-node/src/lifecycle.rs`
- [ ] Implement state machine with transition validation
- [ ] Add user callback registration for each transition
- [ ] Integrate with executor (lifecycle node registers as regular node)

```rust
pub struct LifecycleNode<'a> {
    node: NodeHandle<'a>,
    state: LifecycleState,
    on_configure: Option<Box<dyn FnMut() -> TransitionResult>>,
    on_activate: Option<Box<dyn FnMut() -> TransitionResult>>,
    on_deactivate: Option<Box<dyn FnMut() -> TransitionResult>>,
    on_cleanup: Option<Box<dyn FnMut() -> TransitionResult>>,
    on_shutdown: Option<Box<dyn FnMut() -> TransitionResult>>,
    on_error: Option<Box<dyn FnMut() -> TransitionResult>>,
}

impl<'a> LifecycleNode<'a> {
    pub fn state(&self) -> LifecycleState { ... }
    pub fn trigger_transition(&mut self, transition: LifecycleTransition)
        -> Result<LifecycleState, LifecycleError> { ... }
}
```

#### 18.3.3 Lifecycle Services (Optional, requires `alloc`)

- [ ] Register `change_state` service on the node
- [ ] Register `get_state` service on the node
- [ ] Register `get_available_transitions` service on the node
- [ ] Use `lifecycle_msgs/srv/ChangeState`, `GetState`, `GetAvailableTransitions`

#### 18.3.4 no_std Lifecycle (PollingExecutor)

- [ ] Implement `LifecyclePollingNode` without `Box` callbacks
- [ ] Use function pointers instead of closures
- [ ] State transitions driven by user code (no services)

```rust
pub struct LifecyclePollingNode<'a> {
    node: NodeHandle<'a>,
    state: LifecycleState,
    on_configure: Option<fn() -> TransitionResult>,
    on_activate: Option<fn() -> TransitionResult>,
    on_deactivate: Option<fn() -> TransitionResult>,
    on_cleanup: Option<fn() -> TransitionResult>,
    on_shutdown: Option<fn() -> TransitionResult>,
    on_error: Option<fn() -> TransitionResult>,
}
```

#### 18.3.5 C API

- [ ] Add `nano_ros_lifecycle_node_t` to C API
- [ ] Add `nano_ros_lifecycle_node_init()`, `nano_ros_lifecycle_trigger_transition()`
- [ ] Add callback registration: `nano_ros_lifecycle_register_on_configure()`, etc.
- [ ] Match rclc_lifecycle function signatures

#### 18.3.6 Tests

- [ ] Unit test: valid transition sequence (unconfigured → inactive → active → finalized)
- [ ] Unit test: invalid transition rejected (active → configure fails)
- [ ] Unit test: error recovery path
- [ ] Unit test: callback invocation on transitions
- [ ] Integration test: lifecycle node with ROS 2 `ros2 lifecycle` CLI (requires `alloc`)

---

## 18.4 Serial/UART Transport

### Background

micro-ROS's most-used transport on MCUs is UART serial. Many embedded boards lack Ethernet/WiFi but always have UART. Adding serial transport to nano-ros would enable these platforms.

**Important**: Serial transport does NOT replace zenoh. It provides a point-to-point link between an MCU and a host running zenohd. The host-side bridge translates serial frames into zenoh messages.

**Architecture:**
```
┌──────────────┐  UART  ┌──────────────────┐  Zenoh  ┌──────────────┐
│     MCU      │◄──────►│  Serial Bridge   │◄───────►│   zenohd     │
│  nano-ros    │        │  (host process)  │         │  + ROS 2     │
│  serial shim │        │  serial ↔ zenoh  │         │              │
└──────────────┘        └──────────────────┘         └──────────────┘
```

**micro-ROS reference** (`external/micro_ros_zephyr_module/modules/libmicroros/microros_transports/serial/`):
- Ring buffer pattern (2KB) for interrupt-driven RX
- Framing protocol for message boundaries
- Custom transport callbacks

### Work Items

#### 18.4.1 Serial Framing Protocol

- [ ] Design framing protocol (COBS or length-prefixed) for reliable message boundaries
- [ ] Implement frame encoder/decoder in `crates/nano-ros-transport/src/serial/`
- [ ] Support variable-length messages up to MTU
- [ ] CRC-16 for error detection

```rust
/// Serial frame format:
/// [START_BYTE] [LENGTH_HI] [LENGTH_LO] [PAYLOAD...] [CRC_HI] [CRC_LO]
pub struct SerialFrame<'a> {
    pub payload: &'a [u8],
}

pub struct SerialFramer {
    rx_buf: [u8; MAX_FRAME_SIZE],
    rx_pos: usize,
    state: FrameState,
}

impl SerialFramer {
    /// Feed received bytes, returns complete frames
    pub fn feed(&mut self, byte: u8) -> Option<&[u8]> { ... }

    /// Encode payload into frame
    pub fn encode(payload: &[u8], out: &mut [u8]) -> Result<usize, SerialError> { ... }
}
```

#### 18.4.2 Serial Transport Trait Implementation

- [ ] Implement `Session`, `Publisher`, `Subscriber` traits over serial link
- [ ] Topic multiplexing: include topic ID in serial frames
- [ ] Request/reply correlation for services

#### 18.4.3 Zephyr Serial Backend

- [ ] Implement UART driver using Zephyr UART API
- [ ] Interrupt-driven RX with ring buffer
- [ ] Polling TX (synchronous writes)
- [ ] Kconfig integration: `CONFIG_NANO_ROS_TRANSPORT_SERIAL`

#### 18.4.4 POSIX Serial Backend (for testing)

- [ ] Implement over `/dev/ttyUSB*` or `/dev/pts/*` (pseudo-terminals)
- [ ] Use for host-side testing without hardware

#### 18.4.5 Host-Side Serial Bridge

- [ ] Create `tools/serial-bridge/` — standalone binary
- [ ] Reads serial frames from UART, publishes to zenoh
- [ ] Subscribes to zenoh topics, sends as serial frames to MCU
- [ ] Configuration: serial port, baud rate, zenoh locator

#### 18.4.6 Tests

- [ ] Unit test: framing encode/decode roundtrip
- [ ] Unit test: CRC error detection
- [ ] Integration test: serial pub/sub via pseudo-terminal pair
- [ ] Integration test: serial bridge end-to-end with zenohd

---

## 18.5 Compile-Time Entity Limits

### Background

micro-ROS enforces compile-time limits on entity counts (`RMW_UXRCE_MAX_NODES`, `RMW_UXRCE_MAX_PUBLISHERS`, etc.) via CMake/Kconfig. This ensures predictable memory usage and is required for safety-critical certification (e.g., ISO 26262).

nano-ros already uses const generics for some limits (`MAX_NODES` on `PollingExecutor`). This task extends that pattern consistently.

### Work Items

#### 18.5.1 Const Generic Entity Limits

- [ ] Add `MAX_PUBLISHERS`, `MAX_SUBSCRIPTIONS`, `MAX_SERVICES`, `MAX_TIMERS` to `PollingExecutor`
- [ ] Enforce at compile time via `heapless::Vec` capacity
- [ ] Return clear error when limit exceeded

```rust
pub struct PollingExecutor<
    const MAX_NODES: usize = 4,
    const MAX_SUBS: usize = 8,
    const MAX_PUBS: usize = 8,
    const MAX_SERVICES: usize = 4,
    const MAX_TIMERS: usize = 8,
> { ... }
```

#### 18.5.2 C API Limits

- [ ] Expose limits as `#define` constants in C header
- [ ] Document limits in C API header comments
- [ ] Return `NANO_ROS_RET_NO_MEMORY` when limits exceeded

#### 18.5.3 Kconfig Integration (Zephyr)

- [ ] Add Kconfig entries for entity limits in BSP Zephyr
- [ ] Wire Kconfig values to Rust const generics via build.rs

```kconfig
config NANO_ROS_MAX_NODES
    int "Maximum number of nodes"
    default 4

config NANO_ROS_MAX_PUBLISHERS
    int "Maximum number of publishers"
    default 8

config NANO_ROS_MAX_SUBSCRIPTIONS
    int "Maximum number of subscriptions"
    default 8
```

#### 18.5.4 Tests

- [ ] Compile-time test: exceeding limit produces clear error
- [ ] Unit test: entity creation succeeds up to limit
- [ ] Unit test: entity creation fails at limit with correct error code

---

## Passing Criteria

| Feature              | Criterion                                                    |
|----------------------|--------------------------------------------------------------|
| Trigger conditions   | `trigger_all` blocks until all subs ready; custom trigger works |
| `spin_period()`      | Maintains target rate within 10% on desktop                  |
| Lifecycle nodes      | Full state machine with valid/invalid transition tests       |
| Serial transport     | Pub/sub over pseudo-terminal pair with framing               |
| Entity limits        | PollingExecutor rejects entities beyond const generic bounds |
| C API parity         | All new features accessible from C API                      |
| `just quality`       | Passes after all changes                                    |

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
- 18.3 builds on executor improvements
- 18.4 is the largest item, benefits from stable executor
- 18.5 is independent and can be done in parallel
