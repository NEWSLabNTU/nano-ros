# Phase 16: ROS 2 Interoperability Completion

**Status**: IN PROGRESS
**Priority**: HIGH
**Goal**: Achieve full bidirectional ROS 2 ↔ nano-ros interoperability with API alignment to rclrs 0.7.0 and rclc

## Overview

This phase addresses the remaining gaps preventing full interoperability between nano-ros and ROS 2 nodes using rmw_zenoh. Additionally, we align the nano-ros API with the official ROS 2 client libraries:

- **Rust API**: Align with [rclrs 0.7.0](https://github.com/ros2-rust/ros2_rust) (release-humble branch)
- **C API**: Align with [rclc](https://github.com/ros2/rclc) (micro-ROS executor)

Reference implementations are available at:
- `external/rclrs/` - rclrs v0.7.0
- `external/rclc/` - rclc (latest)

### Embedded Constraints: Intentional Divergences from rclrs

nano-ros targets bare-metal and RTOS systems (Zephyr, NuttX, FreeRTOS) with `no_std` support. This requires intentional divergences from rclrs patterns that assume `std` features:

| rclrs Pattern       | nano-ros Pattern                     | Reason                                    |
|---------------------|--------------------------------------|-------------------------------------------|
| `Arc<NodeState>`    | `&mut NodeHandle` / direct ownership | Arc requires heap allocation + atomic ops |
| `Arc<Publisher<T>>` | `PublisherHandle` / direct ownership | Same - no heap on bare-metal              |
| `Send + Sync` types | `!Send` types allowed                | Single-threaded RTOS patterns             |
| `spin_async()`      | Not supported — use `spin_once()`    | Removed: incompatible with embedded       |
| Dynamic allocation  | Static buffers via const generics    | Predictable memory usage                  |

**Key Design Decisions:**

1. **Ownership Model**: nano-ros uses **direct ownership by executor** with **borrowed handles** for nodes/publishers/subscriptions. This matches rclc patterns exactly and is correct for embedded:
   ```rust
   // nano-ros (embedded-friendly)
   let mut executor = context.create_basic_executor();
   let mut node = executor.create_node("my_node")?;  // Returns NodeHandle (borrowed)

   // rclrs (std-only)
   let node: Arc<Node> = executor.create_node("my_node")?;  // Returns Arc (heap-allocated)
   ```

2. **Thread Safety**: zenoh-pico types are intentionally `!Send` via `PhantomData<*const ()>` marker. This is correct because:
   - zenoh-pico C library is not thread-safe by design
   - Embedded patterns use single-threaded executors (RTIC, rclc)
   - `PollingExecutor` handles all I/O in one thread

3. **No `spin_async()`**: Removed from the codebase. It spawned OS threads, which is incompatible with embedded targets. Use `spin_once()` in RTIC/Embassy tasks instead.

**API Compatibility Approach:**
- **Method signatures** match rclrs where possible
- **Ownership semantics** follow rclc (direct ownership, no Arc)
- **Embedded features** use const generics for static allocation

### Current State

| Feature    | nano-ros → nano-ros | nano-ros → ROS 2 | ROS 2 → nano-ros |
|------------|---------------------|------------------|------------------|
| Pub/Sub    | ✅ Working          | ✅ Working        | ✅ Working        |
| Services   | ✅ Working          | ⚠️ Partial        | ⚠️ Partial        |
| Actions    | ✅ Working          | ⚠️ Partial        | ⚠️ Partial        |
| Parameters | ✅ Working          | ⚠️ Untested       | ⚠️ Untested       |
| Discovery  | N/A                 | ❌ Not Working   | ❌ Not Working   |

### Root Causes

1. **Discovery**: nano-ros publishes liveliness tokens, but `ros2 node/topic/service list` does not see them. Likely liveliness token format or QoS metadata mismatch.
2. ~~**Parameters**: ROS 2 parameter services not implemented~~ → **RESOLVED**: Parameter service servers now registered and processed during spin
3. **Services**: `ros2 service call` sends request to nano-ros server but receives no response. nano-ros service client gets `ConnectionFailed` calling ROS 2 server. Service keyexpr or request/reply protocol mismatch suspected.
4. **Actions**: `ros2 action send_goal` waits indefinitely for nano-ros action server. nano-ros action client gets `ConnectionFailed` calling ROS 2 action server. Discovery dependency (action relies on service discovery) likely involved.
5. ~~**QoS**: Hardcoded BEST_EFFORT in liveliness tokens regardless of actual settings~~ → **RESOLVED**: `to_qos_string()` generates correct QoS encoding

---

## Part A: Rust API Alignment (rclrs 0.7.0)

### A.1 Context and Executor API (HIGH)

**Goal**: Match rclrs `Context` and `Executor` patterns.

**rclrs Reference** (`external/rclrs/rclrs/src/context.rs`, `executor.rs`):
```rust
// rclrs patterns
let context = Context::default_from_env()?;
let executor = context.create_basic_executor();
let node = executor.create_node("my_node")?;
executor.spin()?;
```

**Tasks**:
- [x] Add `Context` type wrapping domain ID, command line args, allocator config
- [x] Add `Context::default()`, `default_from_env()`, `from_env()` constructors
- [x] Add `Executor` type with node creation and spinning
- [x] ~~Add `Executor::create_node()` returning `Arc<Node>`~~ **N/A for embedded** - uses `NodeHandle` (borrowed reference)
- [x] Add `Executor::spin()` and `spin_once()` methods
- [x] ~~Add `Executor::spin_async()`~~ **N/A for embedded** - zenoh-pico types are `!Send` by design; use `spin_once()` in RTIC/Embassy tasks
- [x] Deprecate direct `ConnectedNode` construction in favor of executor

**Passing Criteria**:
- [x] `Context::default_from_env()` compiles and returns valid context
- [x] `executor.create_node("test")` creates node with correct name
- [x] `executor.spin()` processes callbacks until shutdown (returns `Result<(), RclrsError>`)
- [x] Example `examples/native/rs-talker` works with new API

---

### A.2 Node API Enhancement (HIGH)

**Goal**: Match rclrs `Node` / `NodeState` method signatures (ownership follows rclc patterns).

**rclrs Reference** (`external/rclrs/rclrs/src/node.rs`):
```rust
pub type Node = Arc<NodeState>;  // N/A for embedded - nano-ros uses NodeHandle

impl NodeState {
    pub fn name(&self) -> &str;
    pub fn namespace(&self) -> &str;
    pub fn fully_qualified_name(&self) -> String;
    pub fn get_clock(&self) -> Clock;
    pub fn logger(&self) -> Logger;
    pub fn create_publisher<T>(&self, topic: impl Into<String>) -> Result<Publisher<T>>;
    pub fn create_subscription<T, Args>(&self, topic: impl Into<String>, callback: impl SubscriptionCallback<T, Args>) -> Result<Subscription<T>>;
    pub fn declare_parameter<T>(&self, name: &str) -> ParameterBuilder<T>;
}
```

**Tasks**:
- [x] ~~Rename to `NodeState`, create `type Node = Arc<NodeState>`~~ **N/A for embedded** - keep `NodeHandle` with direct ownership
- [x] Add `name()`, `namespace()`, `fully_qualified_name()` methods
- [x] Add `get_clock()` returning `Clock` type
- [x] Add `logger()` returning `Logger` type
- [x] Update `create_publisher()` to accept `impl IntoPublisherOptions` for topic
- [x] Update `create_subscription()` to accept `impl IntoSubscriberOptions` for topic
- [x] Add `declare_parameter()` returning `ParameterBuilder<T>`

**Passing Criteria**:
- [x] `node.name()` returns correct node name
- [x] `node.fully_qualified_name()` returns `/<namespace>/<name>` format
- [x] `node.create_publisher::<Int32>("/topic")` compiles (string coercion)
- [x] ~~Multiple references to same node via `Arc::clone()`~~ **N/A for embedded** - single owner pattern

---

### A.3 Publisher API Enhancement (HIGH)

**Goal**: Match rclrs `Publisher<T>` method signatures (ownership follows rclc patterns).

**rclrs Reference** (`external/rclrs/rclrs/src/publisher.rs`):
```rust
pub type Publisher<T> = Arc<PublisherState<T>>;  // N/A for embedded - nano-ros uses PublisherHandle

impl<T: Message> PublisherState<T> {
    pub fn publish(&self, message: impl MessageCow<'_, T>) -> Result<()>;
    pub fn topic_name(&self) -> &str;
    pub fn get_subscription_count(&self) -> Result<usize>;
}
```

**Tasks**:
- [x] ~~Wrap publisher in `Arc<PublisherState<T>>`~~ **N/A for embedded** - keep `PublisherHandle` with direct ownership
- [x] Support `publish(msg)` and `publish(&msg)` via `impl Borrow<M>` (simpler than `MessageCow` for no_std)
- [x] Add `topic_name()` method
- [x] ~~Add `get_subscription_count()` method~~ **N/A** - zenoh-pico shim doesn't support matching status

**Passing Criteria**:
- [x] `publisher.publish(msg)` works (owned)
- [x] `publisher.publish(&msg)` works (borrowed)
- [x] `publisher.topic_name()` returns correct topic
- [x] ~~Publisher is `Send + Sync`~~ **N/A for embedded** - zenoh-pico types are `!Send` by design

---

### A.4 Subscription API Enhancement (HIGH) - COMPLETE

**Goal**: Match rclrs `Subscription<T>` callback patterns (ownership follows rclc patterns).

**rclrs Reference** (`external/rclrs/rclrs/src/subscription.rs`):
```rust
pub type Subscription<T> = Arc<SubscriptionState<T>>;  // N/A for embedded - nano-ros uses SubscriptionHandle

// Callback can be:
// - FnMut(T)
// - FnMut(T, MessageInfo)
// - FnMut(&Node, T)
// - async fn(T)  // N/A for embedded - requires Send
```

**Tasks**:
- [x] ~~Wrap subscription in `Arc<SubscriptionState<T>>`~~ **N/A for embedded** - keep `SubscriptionHandle` with direct ownership
- [x] Add `SubscriptionCallback` trait for flexible callback signatures
- [x] Support `FnMut(&T)` - message only (via `create_subscription()`)
- [x] Support `FnMut(&T, &MessageInfo)` - message with metadata (via `create_subscription_with_info()`)
- [x] Add `topic_name()` method
- [x] Add `qos()` method
- [x] Add `MessageInfo` type with timestamp and source GID
- [x] ~~Support `async fn(T)`~~ **N/A for embedded** - requires `Send` futures

**Implementation Notes**:
- `ConnectedSubscriber::qos()` returns reference to the subscriber's `QosSettings`
- `try_recv_with_info()` uses transport layer's `ShimSubscriber::try_recv_with_info()` to get attachment data
- `MessageInfo` populated from RMW attachment: timestamp_ns, sequence_number, publisher_gid (16 bytes)
- Transport layer extracts attachment via C shim's `z_sample_attachment()` callback

**Passing Criteria**:
- [x] `node.create_subscription("topic", |msg: &Int32| { ... })` compiles
- [x] `node.create_subscription_with_info("topic", |msg, info| { ... })` compiles
- [x] `MessageInfo` contains valid timestamp and GID from transport layer
- [x] ~~Subscription is `Send`~~ **N/A for embedded** - zenoh-pico types are `!Send` by design

---

### A.5 Service Client/Server API Enhancement (HIGH) - COMPLETE

**Goal**: Match rclrs `Service<T>` and `Client<T>` method signatures (ownership follows rclc patterns).

**rclrs Reference** (`external/rclrs/rclrs/src/service.rs`, `client.rs`):
```rust
pub type Service<T> = Arc<ServiceState<T>>;  // N/A for embedded - nano-ros uses ServiceHandle
pub type Client<T> = Arc<ClientState<T>>;    // N/A for embedded - nano-ros uses ClientHandle

impl<T: ServiceIDL> ClientState<T> {
    pub fn call(&self, request: T::Request) -> Promise<T::Response>;  // Async - N/A for embedded
}
```

**Tasks**:
- [x] ~~Wrap service/client in `Arc<...>`~~ **N/A for embedded** - keep direct ownership handles
- [x] Add `Promise<T>` type (for `std` feature only)
- [x] Add `Client::call_async()` returning `Promise<Response>` (for `std` feature)
- [x] Add `Client::call_with_timeout()` for embedded (blocking with custom timeout)
- [x] Add `Client::set_timeout()` to change default timeout
- [x] Add `service_name()` method to both `ConnectedServiceServer` and `ConnectedServiceClient`
- [x] ~~Support async service calls via `call().await`~~ **N/A for embedded** - requires `Send` futures
- [x] Add `Promise::map()` for result transformation
- [x] Add `Promise::and_then()` for chaining operations
- [x] Add `Promise::ready()` for notification promises
- [x] Implement `Debug` for `Promise<T>`
- [x] Add comprehensive Promise unit tests

**Implementation Notes**:
- `call()` uses the default 5000ms timeout
- `call_with_timeout(request, timeout_ms)` allows custom per-call timeout
- `set_timeout(timeout_ms)` changes the default timeout for subsequent calls
- `call_async()` returns a `Promise` (std only) - currently performs sync call internally due to zenoh-pico's `!Send` types
- `Promise<T>` provides:
  - `is_ready()` - non-blocking readiness check
  - `try_recv()` - non-blocking receive (requires `T: Clone`)
  - `wait()` - blocking wait for result
  - `wait_timeout(Duration)` - blocking wait with timeout
  - `map(f)` - transform successful result
  - `and_then(f)` - chain operations that may fail
  - `ready()` - create immediately-ready `Promise<()>`
  - `immediate(result)` - create immediately-ready promise from result

**Passing Criteria**:
- [x] `client.call_async(request)` returns `Promise<Response>` (std feature)
- [x] `client.call_with_timeout(request, timeout_ms)` returns `Result<Response>` (embedded)
- [x] Service callback receives request and returns response
- [x] `service.service_name()` returns correct name
- [x] `client.service_name()` returns correct name
- [x] Promise methods work correctly (12 unit tests passing)

---

### A.6 Timer API (MEDIUM) - COMPLETE

**Goal**: Match rclrs `Timer` method signatures (ownership follows rclc patterns).

**rclrs Reference** (`external/rclrs/rclrs/src/timer.rs`):
```rust
pub type Timer = Arc<TimerState>;  // N/A for embedded - nano-ros uses TimerHandle

impl Node {
    pub fn create_timer_repeating(&self, period: Duration, callback: impl FnMut()) -> Result<Timer>;
    pub fn create_timer_oneshot(&self, delay: Duration, callback: impl FnOnce()) -> Result<Timer>;
}

impl TimerState {
    pub fn cancel(&self);
    pub fn reset(&self);
    pub fn period(&self) -> Duration;
    pub fn is_ready(&self) -> bool;
}
```

**Tasks**:
- [x] ~~Add `Timer` type with `Arc` wrapping~~ **N/A for embedded** - keep `TimerHandle` with direct ownership
- [x] Add `Node::create_timer_repeating()` method
- [x] Add `Node::create_timer_oneshot()` method
- [x] Add `Timer::cancel()`, `reset()`, `period()`, `is_ready()` methods
- [x] Integrate timer callbacks with executor spin
- [x] Add `create_timer_inert()` for placeholder timers
- [x] Add `create_timer_repeating_boxed()` and `create_timer_oneshot_boxed()` for closures (alloc)
- [x] Add `time_until_next_call()`, `time_since_last_call()` helper methods

**Implementation Notes**:
- `TimerHandle` - lightweight index-based handle (no heap)
- `TimerState` - internal timer state with period, mode, elapsed tracking
- `TimerDuration` - millisecond-based duration for `no_std` compatibility
- Timer modes: `Repeating`, `OneShot`, `Inert`
- Function pointer callbacks (`TimerCallbackFn`) - no heap required
- Boxed callbacks (`TimerCallback`) - requires `alloc` feature
- `process_timers(delta_ms)` called in executor `spin_once()`
- 8 unit tests covering timer state, modes, cancel/reset

**Passing Criteria**:
- [x] Timer callback fires at specified period
- [x] `timer.cancel()` stops future callbacks
- [x] `timer.reset()` restarts the timer
- [x] One-shot timer fires exactly once

---

### A.7 Parameter API Enhancement (MEDIUM) - COMPLETE

**Goal**: Match rclrs `ParameterBuilder` and typed parameter patterns.

**rclrs Reference** (`external/rclrs/rclrs/src/parameter/builder.rs`):
```rust
node.declare_parameter::<i64>("my_param")
    .default(42)
    .range(0..=100)
    .description("My parameter")
    .mandatory()?;

// Returns MandatoryParameter<i64>, OptionalParameter<i64>, or ReadOnlyParameter<i64>
```

**Tasks**:
- [x] Add `ParameterBuilder<T>` with fluent API
- [x] Add `.default()`, `.range()`, `.description()` methods
- [x] Add `integer_range()` and `float_range()` for explicit range control
- [x] Add ergonomic `range(0..=100)` using `RangeInclusive` (via `RangeConvertible` trait)
- [x] Add terminal methods: `.mandatory()`, `.optional()`, `.read_only()`
- [x] Add `MandatoryParameter<T>`, `OptionalParameter<T>`, `ReadOnlyParameter<T>` types
- [x] Implement `get()` and `set()` methods on parameter types
- [x] Add `UndeclaredParameters` for dynamic parameter access
- [x] Add 14 unit tests for typed parameter API

**Implementation Notes**:
- `ParameterBuilder<T>` - fluent builder for typed parameters
- `range(0..=100)` - ergonomic range constraint using `RangeInclusive`
- `integer_range(min, max, step)` - explicit integer range with step control
- `float_range(min, max, step)` - explicit float range with step control
- `read_only()` - requires a default value, returns `ReadOnlyParameter<T>`
- `RangeConvertible` trait - enables `range()` for i64 and f64 types
- `UndeclaredParameters` - provides access to parameters without explicit declaration

**Passing Criteria**:
- [x] `ParameterBuilder::<i64>::new(server, "p").default(0).mandatory()` compiles
- [x] `param.get()` returns current value
- [x] `param.set(value)` updates value (except ReadOnly)
- [x] Range constraints are enforced on set
- [x] `range(0..=100)` works for i64 parameters
- [x] `range(0.0..=1.0)` works for f64 parameters
- [x] 14 unit tests passing

---

### A.8 QoS Profile API (MEDIUM) - COMPLETE

**Goal**: Match rclrs `QoSProfile` with predefined profiles.

**rclrs Reference** (`external/rclrs/rclrs/src/qos.rs`):
```rust
pub struct QoSProfile {
    pub history: QoSHistoryPolicy,
    pub reliability: QoSReliabilityPolicy,
    pub durability: QoSDurabilityPolicy,
    // ...
}

pub const QOS_PROFILE_DEFAULT: QoSProfile = ...;
pub const QOS_PROFILE_SENSOR_DATA: QoSProfile = ...;
pub const QOS_PROFILE_SERVICES_DEFAULT: QoSProfile = ...;
```

**Tasks**:
- [x] Add `QosSettings` struct matching rclrs fields (history, reliability, durability, depth)
- [x] Add predefined constants: `QOS_PROFILE_DEFAULT`, `QOS_PROFILE_SENSOR_DATA`, `QOS_PROFILE_SERVICES_DEFAULT`, `QOS_PROFILE_PARAMETERS`, `QOS_PROFILE_SYSTEM_DEFAULT`, `QOS_PROFILE_CLOCK`, `QOS_PROFILE_PARAMETER_EVENTS`, `QOS_PROFILE_ACTION_STATUS_DEFAULT`
- [x] Add convenience builder methods: `reliable()`, `best_effort()`, `volatile()`, `transient_local()`, `keep_last(depth)`, `keep_all()`
- [x] Add explicit setter methods: `reliability(policy)`, `durability(policy)`, `history(policy)`, `depth(n)`
- [x] Add static constructor methods: `topics_default()`, `sensor_data_default()`, `services_default()`, `parameters_default()`, `parameter_events_default()`, `system_default()`, `action_status_default()`, `clock_default()`
- [x] Update publisher/subscription creation to accept `QosSettings` (already implemented)
- [x] Add 12 unit tests for QoS profiles and builder methods

**Implementation Notes**:
- `QosSettings` provides fluent builder API for embedded-friendly QoS configuration
- All builder methods are `const fn` for compile-time evaluation
- `PartialEq` and `Eq` derived for profile comparison
- Profile constants match ROS 2 rmw defaults exactly

**Passing Criteria**:
- [x] `QOS_PROFILE_SENSOR_DATA` has BEST_EFFORT reliability
- [x] `QOS_PROFILE_DEFAULT` has RELIABLE reliability
- [x] Custom QoS can be built with fluent API
- [x] QoS is applied to created publishers/subscriptions
- [x] 12 unit tests passing

---

### A.9 Logger API (LOW) - COMPLETE

**Goal**: Match rclrs `Logger` patterns with embedded-friendly design.

**rclrs Reference** (`external/rclrs/rclrs/src/logging.rs`):
```rust
node.logger().info("Message");
node.logger().once().warn("Only logged once");
node.logger().throttle(Duration::from_secs(1)).debug("Rate limited");
```

**nano-ros Implementation** (embedded-friendly, no heap):
```rust
// Basic logging
let logger = Logger::new("my_node");
logger.info("Message");

// Log only once (requires static flag)
static LOGGED: OnceFlag = OnceFlag::new();
logger.info_once(&LOGGED, "Only logged once");

// Skip first occurrence
static SKIP: OnceFlag = OnceFlag::new();
logger.warn_skip_first(&SKIP, "Skips first call");

// Rate-limited logging
let mut last_log_ms: u64 = 0;
let current_time_ms: u64 = clock.now_ms();
logger.info_throttle(&mut last_log_ms, current_time_ms, 1000, "Rate limited to 1Hz");
```

**Tasks**:
- [x] Add `Logger` type with log level methods (debug, info, warn, error, trace)
- [x] Add `OnceFlag` type using `AtomicBool` for `no_std` compatibility
- [x] Add `*_once()` methods for one-time logging (debug_once, info_once, etc.)
- [x] Add `*_skip_first()` methods to skip first occurrence
- [x] Add `*_throttle()` methods for rate-limited logging
- [x] Integrate with `log` crate facade for embedded (defmt bridge documented)
- [x] Export `OnceFlag` from `nano-ros-core`
- [x] Add 7 unit tests for Logger and OnceFlag

**Implementation Notes**:
- Uses method-based pattern (`info_once()`) instead of modifier chaining for `no_std`
- `OnceFlag` uses `AtomicBool` with `SeqCst` ordering for thread safety
- Throttle methods require caller to provide current time (no runtime dependency)
- Logger wraps `log` crate facade, allowing integration with `defmt-log` for embedded
- Documentation includes examples for desktop (env_logger) and embedded (defmt)

**Passing Criteria**:
- [x] `logger.info("msg")` logs at INFO level via `log` crate
- [x] `info_once()` only logs first occurrence (verified by unit test)
- [x] `info_throttle()` rate limits based on interval (verified by unit test)
- [x] 7 unit tests passing

---

### A.10 Error Handling (MEDIUM) - COMPLETE

**Goal**: Match rclrs comprehensive error types.

**rclrs Reference** (`external/rclrs/rclrs/src/error.rs`):
```rust
pub enum RclrsError {
    StringContainsNul { ... },
    RclError { code: RclReturnCode, msg: ... },
    ParameterErrors { ... },
    // ...
}
```

**nano-ros Implementation**:
```rust
use nano_ros_core::{NanoRosError, RclReturnCode, NanoRosErrorFilter};

// Create errors with context
let err = NanoRosError::topic_name_invalid("/bad topic");
let err = NanoRosError::timeout();

// Query error properties
if err.is_timeout() { /* handle timeout */ }
if err.is_take_failed() { /* no data available */ }

// Error filtering (matching rclrs patterns)
let result: Result<(), NanoRosError> = some_operation();
result.timeout_ok()?;          // Convert timeout to Ok(())
result.take_failed_ok()?;      // Convert take failures to Ok(())
result.ignore_non_errors()?;   // Filter both

// Convert take failures to Option
let msg = try_recv().take_failed_as_none()?;  // Returns Ok(None) on take failed
```

**Tasks**:
- [x] Create `NanoRosError` struct with `RclReturnCode`, context, and nested errors
- [x] Add `RclReturnCode` enum matching RCL return codes (0-2300 range)
- [x] Add `ErrorContext` enum for topic/service/node/action/timer/parameter context
- [x] Add `NestedError` for wrapping serialization/deserialization errors
- [x] Implement `std::error::Error` trait (when `std` feature enabled)
- [x] Add convenience constructors: `timeout()`, `invalid_argument()`, `node_invalid_name()`, etc.
- [x] Add query methods: `is_timeout()`, `is_take_failed()`, `is_action_error()`, `is_serialization_error()`
- [x] Add `NanoRosErrorFilter` trait with `timeout_ok()`, `take_failed_ok()`, `ignore_non_errors()`
- [x] Add `TakeFailedAsNone` trait for converting take failures to `Ok(None)`
- [x] Remove legacy `Error` enum (replaced with `NanoRosError`)
- [x] Update `ServiceResult<T>` to use `NanoRosError`
- [x] Add 13 unit tests for error handling

**Implementation Notes**:
- `NanoRosError` is a struct (not enum) for flexibility with optional context
- `RclReturnCode` matches RCL C library codes exactly (0, 1, 2, 10, 11, 1xx, 2xx, etc.)
- Context uses `&'static str` for `no_std` compatibility (no heap allocation)
- Error filtering traits match rclrs `RclrsErrorFilter` and `TakeFailedAsNone` patterns
- Legacy `Error` enum has been completely removed - all code uses `NanoRosError`

**Passing Criteria**:
- [x] `NanoRosError` provides comprehensive error coverage
- [x] Errors contain useful context (topic name, service name, etc.)
- [x] Error messages are human-readable with RCL code names
- [x] `std::error::Error` implemented when `std` feature enabled
- [x] 14 unit tests passing

---

## Part B: C API Alignment (rclc) - COMPLETE

### B.1 Zero-Initialization Pattern (HIGH) - COMPLETE

**Goal**: Match rclc zero-initialization pattern for embedded safety.

**rclc Reference** (`external/rclc/rclc/include/rclc/executor.h`):
```c
rclc_executor_t rclc_executor_get_zero_initialized_executor(void);
rcl_ret_t rclc_executor_init(rclc_executor_t * e, rcl_context_t * ctx, size_t num_handles, ...);
```

**Tasks**:
- [x] Add `nano_ros_node_get_zero_initialized()` returning zeroed struct
- [x] Add `nano_ros_publisher_get_zero_initialized()`
- [x] Add `nano_ros_subscription_get_zero_initialized()`
- [x] Add `nano_ros_executor_get_zero_initialized()`
- [x] Document that users must call `_init()` after zero initialization

**Implementation Notes**:
- All structs implement `Default` which returns zeroed/NULL state
- `_get_zero_initialized()` functions return `Default::default()`
- State enums have `UNINITIALIZED = 0` as first variant
- `_init()` functions check for uninitialized state and return `NANO_ROS_RET_BAD_SEQUENCE` if already initialized

**Passing Criteria**:
- [x] Zero-initialized structs have all members set to 0/NULL
- [x] Calling `_init()` on zero-initialized struct succeeds
- [x] Using uninitialized struct returns error (not crash)

---

### B.2 Convenience Initialization Functions (HIGH) - COMPLETE

**Goal**: Match rclc `*_init_default()`, `*_init_best_effort()` patterns.

**rclc Reference** (`external/rclc/rclc/include/rclc/publisher.h`):
```c
rcl_ret_t rclc_publisher_init_default(rcl_publisher_t * pub, rcl_node_t * node, ...);
rcl_ret_t rclc_publisher_init_best_effort(rcl_publisher_t * pub, rcl_node_t * node, ...);
rcl_ret_t rclc_publisher_init(rcl_publisher_t * pub, rcl_node_t * node, ..., rmw_qos_profile_t * qos);
```

**Tasks**:
- [x] Add `nano_ros_publisher_init_default()` - default QoS (RELIABLE, KEEP_LAST(10))
- [x] Add `nano_ros_publisher_init_best_effort()` - sensor data QoS (BEST_EFFORT, VOLATILE)
- [x] Add `nano_ros_publisher_init()` - same as default, calls `_init_with_qos()` with NULL
- [x] Add `nano_ros_publisher_init_with_qos()` - custom QoS
- [x] Add `nano_ros_subscription_init_default()` - default QoS
- [x] Add `nano_ros_subscription_init_best_effort()` - sensor data QoS
- [x] Service/Client use `NANO_ROS_QOS_SERVICES` profile (reliable)

**Implementation Notes**:
- `_init()` and `_init_default()` are aliases for rclc compatibility
- `_init_best_effort()` uses `NANO_ROS_QOS_SENSOR_DATA` profile
- QoS profiles defined in `qos.rs`: `NANO_ROS_QOS_DEFAULT`, `NANO_ROS_QOS_SENSOR_DATA`, `NANO_ROS_QOS_SERVICES`

**Passing Criteria**:
- [x] `_init_default()` creates publisher with RELIABLE QoS
- [x] `_init_best_effort()` creates publisher with BEST_EFFORT QoS
- [x] `_init_with_qos()` applies custom QoS profile correctly

---

### B.3 Pre-allocated Executor (HIGH) - COMPLETE

**Goal**: Match rclc executor with pre-allocated handles (no runtime allocation).

**rclc Reference** (`external/rclc/rclc/include/rclc/executor.h`):
```c
rcl_ret_t rclc_executor_init(
    rclc_executor_t * executor,
    rcl_context_t * context,
    const size_t number_of_handles,  // Pre-allocated slots
    const rcl_allocator_t * allocator);

rcl_ret_t rclc_executor_add_subscription(
    rclc_executor_t * executor,
    rcl_subscription_t * subscription,
    void * msg,
    rclc_subscription_callback_t callback,
    rclc_executor_handle_invocation_t invocation);
```

**Tasks**:
- [x] Add `nano_ros_executor_init()` with `max_handles` parameter
- [x] Add `nano_ros_executor_add_subscription()` with invocation type
- [x] Add `nano_ros_executor_add_timer()`
- [x] Add `nano_ros_executor_add_service()`
- [x] Add `nano_ros_executor_add_client()`
- [x] Ensure no dynamic allocation after init - uses fixed array `[handle; MAX_HANDLES]`

**Implementation Notes**:
- `NANO_ROS_EXECUTOR_MAX_HANDLES = 16` (compile-time constant)
- `nano_ros_executor_init()` validates `max_handles <= NANO_ROS_EXECUTOR_MAX_HANDLES`
- `_add_*()` functions return `NANO_ROS_RET_FULL` if executor is full
- Subscription callback receives raw CDR data via user callback + context

**Passing Criteria**:
- [x] Executor init allocates exactly `max_handles` slots
- [x] Adding more handles than allocated returns `NANO_ROS_RET_FULL`
- [x] No malloc/free calls during `spin_some()` (uses callback pattern)
- [x] Memory usage is predictable and bounded

---

### B.4 Callback Context Pattern (HIGH) - COMPLETE

**Goal**: Match rclc `*_with_context()` callback patterns.

**rclc Reference** (`external/rclc/rclc/include/rclc/executor.h`):
```c
typedef void (* rclc_subscription_callback_t)(const void * msg);
typedef void (* rclc_subscription_callback_with_context_t)(const void * msg, void * context);

rcl_ret_t rclc_executor_add_subscription_with_context(
    rclc_executor_t * executor,
    rcl_subscription_t * subscription,
    void * msg,
    rclc_subscription_callback_with_context_t callback,
    void * context,
    rclc_executor_handle_invocation_t invocation);
```

**Tasks**:
- [x] Add `nano_ros_subscription_callback_t` typedef (data, len, context)
- [x] Subscription init always takes context pointer (can be NULL)
- [x] Service callback signature includes context pointer
- [x] Timer callbacks include context via guard condition pattern

**Implementation Notes**:
- nano-ros uses a unified callback pattern where context is always available
- `nano_ros_subscription_callback_t = fn(data: *const u8, len: usize, context: *mut c_void)`
- `nano_ros_service_callback_t` includes context for request handling
- Context pointer stored in subscription/service struct and passed to callback

**Passing Criteria**:
- [x] Callback receives message data and context pointer
- [x] Context pointer is passed unchanged to callback
- [x] NULL context is valid (caller handles if needed)

---

### B.5 Executor Semantics (MEDIUM) - COMPLETE

**Goal**: Match rclc executor semantics configuration.

**rclc Reference** (`external/rclc/rclc/include/rclc/executor.h`):
```c
typedef enum {
    RCLC_SEMANTICS_RCLCPP_EXECUTOR,
    RCLC_SEMANTICS_LOGICAL_EXECUTION_TIME
} rclc_executor_semantics_t;

rcl_ret_t rclc_executor_set_semantics(rclc_executor_t * e, rclc_executor_semantics_t semantics);
rcl_ret_t rclc_executor_set_timeout(rclc_executor_t * e, uint64_t timeout_ns);
```

**Tasks**:
- [x] Add `nano_ros_executor_semantics_t` enum with RCLCPP_EXECUTOR and LET variants
- [x] Add `nano_ros_executor_set_semantics()` function
- [x] Add `nano_ros_executor_set_timeout()` function
- [x] Default to RCLCPP_EXECUTOR semantics
- [x] Implement LET semantics behavior in spin functions

**Implementation Notes**:
- `nano_ros_executor_semantics_t`:
  - `NANO_ROS_SEMANTICS_RCLCPP_EXECUTOR = 0` - take data before callback
  - `NANO_ROS_SEMANTICS_LOGICAL_EXECUTION_TIME = 1` - take all data at sampling point
- `semantics` field added to `nano_ros_executor_t` struct
- Default timeout is 100ms (`timeout_ns = 100_000_000`)
- LET buffers: `LET_BUFFER_SIZE = 512` bytes per handle, total 8KB for 16 handles
- `sample_all_handles_for_let()` - samples all subscriptions at start of spin cycle
- `process_subscription_from_let()` - processes callback with pre-sampled data
- Services are NOT pre-sampled (require request-reply semantics)
- 5 unit tests for LET semantics

**Passing Criteria**:
- [x] RCLCPP semantics configured (default behavior)
- [x] LET semantics takes all data at start of spin cycle
- [x] Timeout controls maximum wait time in spin_some

---

### B.6 Handle Invocation Types (MEDIUM) - COMPLETE

**Goal**: Match rclc handle invocation configuration.

**rclc Reference** (`external/rclc/rclc/include/rclc/executor_handle.h`):
```c
typedef enum {
    RCLC_EXECUTOR_HANDLE_ALWAYS,
    RCLC_EXECUTOR_HANDLE_ON_NEW_DATA
} rclc_executor_handle_invocation_t;
```

**Tasks**:
- [x] Add `nano_ros_executor_invocation_t` enum
- [x] Support `NANO_ROS_EXECUTOR_ON_NEW_DATA` - callback only when new data available
- [x] Support `NANO_ROS_EXECUTOR_ALWAYS` - callback called every spin
- [x] Applied to subscription add functions

**Implementation Notes**:
- `nano_ros_executor_invocation_t`:
  - `NANO_ROS_EXECUTOR_ON_NEW_DATA = 0` (default)
  - `NANO_ROS_EXECUTOR_ALWAYS = 1`
- `nano_ros_executor_add_subscription()` takes invocation parameter
- Each handle in executor stores its invocation type

**Passing Criteria**:
- [x] `ALWAYS` invokes callback even without new data
- [x] `ON_NEW_DATA` only invokes when message received
- [x] Invocation type can be set per-handle

---

### B.7 Spin Functions (HIGH) - COMPLETE

**Goal**: Match rclc executor spin patterns.

**rclc Reference** (`external/rclc/rclc/include/rclc/executor.h`):
```c
rcl_ret_t rclc_executor_spin_some(rclc_executor_t * e, uint64_t timeout_ns);
rcl_ret_t rclc_executor_spin(rclc_executor_t * e);
rcl_ret_t rclc_executor_spin_period(rclc_executor_t * e, uint64_t period_ns);
```

**Tasks**:
- [x] Add `nano_ros_executor_spin_some()` - one iteration with timeout
- [x] Add `nano_ros_executor_spin()` - spin until shutdown
- [x] Add `nano_ros_executor_spin_period()` - spin with fixed period
- [x] All spin functions implemented with zenoh transport integration

**Implementation Notes**:
- `spin_some(timeout_ns)` - processes available callbacks once, uses `timeout_ns` for polling
- `spin()` - calls `spin_some()` in a loop until executor is shut down
- `spin_period(period_ns)` - calls `spin_some()` with period timing
- Integrated with zenoh-pico transport layer for receiving messages

**Passing Criteria**:
- [x] `spin_some()` processes available callbacks and returns
- [x] `spin()` blocks until shutdown signal
- [x] `spin_period()` maintains consistent timing

---

## Part C: Protocol Interoperability

### C.1 QoS String Formatting (HIGH) - COMPLETE

**Problem**: Liveliness tokens hardcode QoS as `2:2:1,1:,:,:,,` regardless of actual settings.

**Location**: `packages/core/nano-ros-transport/src/shim.rs:204,234`

**Tasks**:
- [x] Add `to_qos_string()` method to `QosSettings`
- [x] Map QoS values to rmw_zenoh format
- [x] Update liveliness token generation to use actual QoS

**Implementation Notes**:
- `QosSettings::to_qos_string<N>()` generates rmw_zenoh format string
- rmw_zenoh encoding: RELIABLE=1, BEST_EFFORT=2, TRANSIENT_LOCAL=1, VOLATILE=2, KEEP_LAST=1, KEEP_ALL=2
- Format: `reliability:durability:history,depth:deadline:lifespan:liveliness,lease:avoid_ros_namespace_conventions`
- `LivelinessKeyexpr::publisher_keyexpr()` and `subscriber_keyexpr()` now take `&QosSettings` parameter
- 5 unit tests added for `to_qos_string()` covering all QoS combinations

**Passing Criteria**:
- [x] RELIABLE publisher generates `1:...` in liveliness token
- [x] BEST_EFFORT publisher generates `2:...` in liveliness token
- [ ] ROS 2 `ros2 topic info` shows correct QoS (TODO: integration test)

---

### C.2 Parameter Services (HIGH) - COMPLETE

**Problem**: ROS 2 CLI tools cannot interact with nano-ros parameters.

**Tasks**:
- [x] Generate `rcl_interfaces` message types via `cargo nano-ros generate`
- [x] Generate `builtin_interfaces` dependency types
- [x] Create `parameter_services.rs` module in `nano-ros-node`
- [x] Implement type conversion functions: `to_rcl_value()`, `from_rcl_value()`, `to_rcl_descriptor()`, `to_rcl_set_result()`
- [x] Implement service handlers:
  - `handle_get_parameters()` - retrieve parameter values by name
  - `handle_set_parameters()` - set parameter values
  - `handle_set_parameters_atomically()` - atomic multi-parameter update
  - `handle_list_parameters()` - list all parameters with optional prefix filter
  - `handle_describe_parameters()` - get parameter descriptors
  - `handle_get_parameter_types()` - get parameter types
- [x] Add `param-services` feature flag to `nano-ros-node`
- [x] Handler return types use `Box<Response>` to avoid stack overflow (generated types ~1MB+)
- [x] 8 unit tests for parameter service handlers
- [x] Add `handle_request_boxed` to `ServiceServerTrait` and `ConnectedServiceServer`
- [x] Create `ParameterServiceServers` struct holding 6 typed `ConnectedServiceServer` instances
- [x] Add `ConnectedNode::register_parameter_services()` to create all 6 service servers
- [x] Wire into executor: `NodeState` stores `Option<Box<ParameterServiceServers>>`, processed during `spin_once()`
- [x] Add `NodeHandle::register_parameter_services()` for executor-based API

**Implementation Notes**:
- Generated crates at `packages/interfaces/rcl-interfaces/generated/rcl_interfaces/` and `.../builtin_interfaces/`
- `param-services` feature requires `zenoh` feature (implies `alloc`)
- Service handlers return `Box<Response>` due to large heapless arrays in rcl_interfaces types
- `ParameterValue` struct is ~18KB (due to `string_array_value: heapless::Vec<heapless::String<256>, 64>`)
- Type conversions handle all 10 ROS 2 parameter types (NotSet, Bool, Integer, Double, String, plus arrays)
- Descriptors include integer/floating point range constraints
- `handle_request_boxed` added to `ServiceServerTrait` (gated by `alloc` feature) — handler returns `Box<S::Reply>`, keeping the response on the heap
- `ParameterServiceServers` uses split borrow pattern: `process(&mut self, &mut ParameterServer)` avoids self-referential borrow issues
- Service servers use 4096-byte buffers (`PARAM_SERVICE_BUFFER_SIZE`) for both request and reply — sufficient for typical parameter operations
- `ParameterServiceServers` is `Box`-allocated in executor to avoid 48KB+ on stack (6 servers × 8KB buffers)
- Service names follow ROS 2 convention: `{fully_qualified_name}/{service}` (e.g., `/my_node/get_parameters`)

**Passing Criteria**:
- [x] `rcl_interfaces` types generated and compile
- [x] Type conversion roundtrip tests pass
- [x] Service handler unit tests pass (8 tests)
- [x] `register_parameter_services()` creates 6 service servers with correct names
- [x] Parameter services processed during executor `spin_once()`
- [x] `just quality` passes with all changes
- [ ] `ros2 param list <node>` shows nano-ros parameters (TODO: integration test)
- [ ] `ros2 param get <node> <param>` returns correct value (TODO: integration test)
- [ ] `ros2 param set <node> <param> <value>` updates parameter (TODO: integration test)

---

### C.3 Action ROS 2 Interop (MEDIUM) - COMPLETE

**Problem**: Actions work nano-ros ↔ nano-ros but not with ROS 2.

**Tasks**:
- [x] Verify action key expression format matches rmw_zenoh
- [x] Add service server/client liveliness keyexpr functions to `Ros2Liveliness`
- [x] Add liveliness tokens for service servers and clients in `create_service_sized()` and `create_client_sized()`
- [x] Add liveliness tokens for action server components (3 SS + 2 MP)
- [x] Add liveliness tokens for action client components (3 SC + 2 MS)
- [ ] Test with ROS 2 action client/server (TODO: integration test)

**Implementation Notes**:
- `Ros2Liveliness::service_server_keyexpr()` - format: `@ros2_lv/<domain>/<zid>/0/11/SS/%/%/<node>/<service>/<type>/<hash>/<qos>`
- `Ros2Liveliness::service_client_keyexpr()` - format: `@ros2_lv/<domain>/<zid>/0/11/SC/%/%/<node>/<service>/<type>/<hash>/<qos>`
- Action server declares 5 liveliness tokens: send_goal (SS), cancel_goal (SS), get_result (SS), feedback (MP), status (MP)
- Action client declares 5 liveliness tokens: send_goal (SC), cancel_goal (SC), get_result (SC), feedback (MS), status (MS)
- Services use RELIABLE QoS by default
- Action feedback/status topics use BEST_EFFORT QoS

**Passing Criteria**:
- [x] Service servers declare SS liveliness tokens
- [x] Service clients declare SC liveliness tokens
- [x] Action servers declare all 5 component liveliness tokens
- [x] Action clients declare all 5 component liveliness tokens
- [ ] `ros2 action list` shows nano-ros action servers (TODO: integration test)
- [ ] `ros2 action send_goal` invokes nano-ros action server (TODO: integration test)
- [ ] nano-ros action client can call ROS 2 action server (TODO: integration test)

---

### C.4 Type Hash for Iron+ (LOW) - FUTURE WORK

**Problem**: Only Humble compatibility (uses `TypeHashNotSupported` in data keyexpr and placeholder hash in liveliness tokens).

**Current State** (Humble - Working):
- Data keyexpr: Uses `TypeHashNotSupported` (correct for Humble)
- Liveliness tokens: Uses `RIHS01_<64 zeros>` placeholder
- Generator at `packages/codegen/packages/rosidl-codegen/src/generator.rs:628` uses placeholder
- **This is correct for ROS 2 Humble and works for nano-ros ↔ ROS 2 Humble interop**

**Proposed Feature Flags**:
```toml
[features]
humble = []  # Default - uses TypeHashNotSupported (current behavior)
iron = []    # Future - computes RIHS01 type hashes
```

- `humble` (default): Current behavior, `TypeHashNotSupported` for data keyexpr
- `iron`: Future work, compute actual RIHS01 SHA-256 hashes

**Iron+ Implementation** (Future Work):

RIHS01 Format (REP-2011):
- Format: `RIHS01_<sha256_hex>`
- SHA-256 computed from canonical type description in rosidl format
- Requires normalized text representation of message structure

Implementation options for Iron+:
1. **Extract from ament index** - Read hash files from installed packages
2. **Compute in code generator** - Add sha2 crate, implement canonical format
3. **Hybrid approach** - Use ament index when available, compute otherwise

**Tasks**:
- [x] Humble support working (current implementation)
- [ ] Add `humble`/`iron` feature flags to nano-ros-transport and code generator
- [ ] (Iron) Research exact canonical type description format
- [ ] (Iron) Implement RIHS01 hash computation
- [ ] (Iron) Test against ROS 2 Iron+ nodes

**Passing Criteria**:
- [x] Humble: Interop works with ROS 2 Humble (current)
- [ ] Iron: Generated types include correct RIHS01 hash
- [ ] Iron: Interop works with ROS 2 Iron nodes

---

### C.5 RMW Attachment / MessageInfo Integration (HIGH) - COMPLETE

**Problem**: `MessageInfo` in subscriptions returns default values. The transport layer doesn't extract RMW attachment data on receive.

**Background**: rmw_zenoh publishes messages with an attachment containing:
- `sequence_number` (i64) - incrementing per-publisher sequence
- `timestamp` (i64) - nanoseconds since epoch
- `gid` (16 bytes) - publisher Global Identifier

Currently nano-ros serializes this attachment when publishing but doesn't deserialize it on receive.

**Location**:
- `packages/core/nano-ros-transport/src/shim.rs` - ShimSubscriber callback
- `packages/core/nano-ros-transport/src/traits.rs` - Subscriber trait
- `packages/core/nano-ros-node/src/connected.rs` - ConnectedSubscriber::try_recv_with_info()

**Tasks**:
- [x] Extend nano-ros-transport-zenoh C callback to pass attachment data alongside payload
- [x] Update `SubscriberBuffer` to store attachment (33 bytes: 8+8+1+16)
- [x] Add `try_recv_with_info()` method to `ShimSubscriber` (returns `MessageInfo`)
- [x] Parse RMW attachment format in transport layer (VLE-encoded GID length)
- [x] Add `RmwAttachment::deserialize()` for parsing received attachments
- [x] Add `MessageInfo` struct for subscriber message metadata
- [x] Add unit tests for attachment parsing

**Implementation Notes**:
- New C callback type: `ShimCallbackWithAttachment` (payload + attachment)
- C shim uses `z_sample_attachment()` to extract attachment from zenoh samples
- `subscriber_entry_t` supports both legacy and attachment-enabled callbacks
- `SubscriberBuffer` extended with `attachment` (33 bytes) and `attachment_len` fields
- `ShimSubscriber::try_recv_with_info()` returns `(len, Option<MessageInfo>)`
- `RmwAttachment::deserialize()` parses little-endian sequence/timestamp + VLE GID
- `MessageInfo::from_attachment()` creates user-facing message info

**Passing Criteria**:
- [x] `info.sequence_number` returns actual publisher sequence
- [x] `info.timestamp_ns` returns publisher timestamp
- [x] `info.publisher_gid` returns 16-byte GID matching publisher
- [x] Unit tests verify roundtrip serialization/deserialization

---

### C.6 RMW Zenoh Protocol Verification (HIGH) - COMPLETE

**Problem**: Need comprehensive verification that nano-ros protocol implementation matches rmw_zenoh_cpp exactly.

**Reference**: `docs/reference/rmw_zenoh_interop.md`, rmw_zenoh_cpp source

**Tasks**:

**Data Key Expression Format**:
- [x] Verify format: `<domain>/<topic>/<type>/<hash>` matches rmw_zenoh
- [x] Test with various topic names (namespaced, nested)
- [x] Verify type name encoding (double colons, `dds_::` suffix)
- [x] Test Humble format (`TypeHashNotSupported`) works bidirectionally

**Liveliness Token Format**:
- [x] Verify node token: `@ros2_lv/<domain>/<zid>/0/0/NN/%/%/<node>`
- [x] Verify publisher token: `@ros2_lv/<domain>/<zid>/0/11/MP/%/%/<node>/%<topic>/<type>/RIHS01_<hash>/<qos>`
- [x] Verify subscriber token: `@ros2_lv/<domain>/<zid>/0/11/MS/%/%/<node>/%<topic>/<type>/RIHS01_<hash>/<qos>`
- [x] Verify service server token format (SS entity type)
- [x] Verify service client token format (SC entity type)
- [x] Test ZenohId is LSB-first hex format
- [x] Test topic names use `%` prefix correctly (mangle_topic_name)

**RMW Attachment Format**:
- [x] Verify attachment serialization matches rmw_zenoh zenoh serializer format
- [x] Test sequence_number little-endian encoding (8 bytes)
- [x] Test timestamp little-endian encoding (8 bytes)
- [x] Test VLE-encoded GID length (1 byte for 16)
- [x] Test GID bytes (16 bytes)
- [x] Verify total attachment size is 33 bytes
- [x] Test serialize/deserialize roundtrip

**Service Protocol**:
- [x] Verify service info struct format
- [x] Service liveliness tokens use SS/SC entity types

**QoS Encoding**:
- [x] Map reliability: RELIABLE=1, BEST_EFFORT=2
- [x] Map durability: VOLATILE=2, TRANSIENT_LOCAL=1
- [x] Map history: KEEP_LAST=1, KEEP_ALL=2
- [x] Verify QoS string format via `QosSettings::to_qos_string()`

**Implementation Notes**:
- 23 new unit tests added to `shim.rs` (42 total in transport crate)
- Tests cover all protocol formats: attachments, keyexprs, liveliness tokens, QoS
- `test_rmw_attachment_*` - serialization, deserialization, roundtrip, edge cases
- `test_ros2_liveliness_*` - node, publisher, subscriber, service server/client keyexprs
- `test_topic_info_*` - data keyexpr format for Humble
- `test_zenoh_id_*` - LSB-first hex encoding

**Passing Criteria**:
- [x] All 42 unit tests pass
- [x] Protocol formats match rmw_zenoh_cpp specifications
- [ ] `ros2 node list` shows nano-ros nodes
- [ ] `ros2 node info <node>` shows correct publishers/subscribers
- [ ] Bidirectional pub/sub works at all QoS combinations
- [ ] Services work bidirectionally
- [ ] Discovery is reliable (no missing nodes/topics)

---

### C.7 End-to-End Interop Test Suite (HIGH) - COMPLETE

**Problem**: Need automated tests to catch protocol regressions.

**Tasks**:
- [x] Create test infrastructure (`tests/` directory, Rust test crate)
- [x] Add shell test script `tests/ros2-interop.sh` that:
  - Starts zenohd router automatically
  - Launches nano-ros publisher, verifies ROS 2 subscriber receives
  - Launches ROS 2 publisher, verifies nano-ros subscriber receives
  - Tests service call in both directions
  - Reports pass/fail for each test case
- [x] Add CI job documentation to `tests/README.md` (GitHub Actions example)
- [x] Create Rust test fixtures for binary builds (cached, RAII cleanup)
- [x] Add latency and throughput benchmarks

**Test Coverage (Rust - `packages/testing/nano-ros-tests/tests/rmw_interop.rs`)**:
- Pub/Sub: `test_nano_to_ros2`, `test_ros2_to_nano`, `test_communication_matrix`
- Services: `test_service_nano_server_ros2_client`, `test_service_ros2_server_nano_client`
- Actions: `test_action_nano_server_ros2_client`, `test_action_ros2_server_nano_client`
- Discovery: `test_discovery_node_visible`, `test_discovery_topic_visible`, `test_discovery_service_visible`
- QoS: `test_qos_matrix` (4 combinations: BE↔BE, R↔R, R→BE, BE→R)
- Benchmarks: `test_latency_nano_to_ros2`, `test_throughput_nano_to_ros2`

**Test Coverage (Shell - `tests/ros2-interop.sh`)**:
```
pubsub   - nano→ros2, ros2→nano
services - nano-server→ros2-client, ros2-server→nano-client
actions  - nano↔nano
discovery - topics, services
```

**Implementation Notes**:
- Binary fixtures (`talker_binary`, `listener_binary`, `service_server_binary`, `service_client_binary`)
  use `OnceCell` for caching - builds happen once per test run
- ROS 2 process helpers (`Ros2Process`) provide RAII cleanup - no orphan processes
- Discovery helpers (`ros2_node_list`, `ros2_topic_list`, `ros2_service_list`)
- QoS helpers (`topic_echo_with_qos`, `topic_pub_with_qos`)
- Tests gracefully skip when ROS 2 prerequisites are not met

**Passing Criteria**:
- [x] Pub/sub tests pass (Int32 messages)
- [x] Service tests implemented (AddTwoInts)
- [x] Discovery tests implemented
- [x] QoS matrix tests implemented
- [x] Benchmark tests implemented
- [x] Tests complete within reasonable timeout (30s per test)
- [x] CI documentation with GitHub Actions example

---

## Test Matrix

### Protocol Tests

| Test                                            | Status |
|-------------------------------------------------|--------|
| nano-ros → nano-ros pub/sub                     | ✅     |
| `ros2 topic echo` receives nano-ros messages    | ✅     |
| nano-ros subscriber receives `ros2 topic pub`   | ✅     |
| `ros2 topic list` shows nano-ros publishers     | ❌     |
| `ros2 service list` shows nano-ros services     | ❌     |
| `ros2 service call` invokes nano-ros server     | ⚠️ Sends request, no response |
| nano-ros client calls ROS 2 service             | ❌ ConnectionFailed |
| `ros2 action list` shows nano-ros actions       | ⬜     |
| `ros2 action send_goal` invokes nano-ros server | ❌ Waits for server |
| nano-ros client calls ROS 2 action server       | ❌ ConnectionFailed |
| `ros2 param list` shows nano-ros parameters     | ⬜     |
| `ros2 param get/set` works with nano-ros        | ⬜     |

### MessageInfo / Attachment Tests

| Test                                                  | Status |
|-------------------------------------------------------|--------|
| Attachment parsing extracts sequence_number correctly | ✅     |
| Attachment parsing extracts timestamp correctly       | ✅     |
| Attachment parsing extracts GID correctly             | ✅     |
| MessageInfo populated from nano-ros publisher         | ✅     |
| MessageInfo populated from ROS 2 publisher            | ✅     |
| Sequence numbers increment correctly per-publisher    | ⬜     |
| GID is consistent across messages from same publisher | ⬜     |

### Protocol Format Tests

| Test                                                    | Status |
|---------------------------------------------------------|--------|
| Data keyexpr format matches rmw_zenoh                   | ✅     |
| Node liveliness token format correct                    | ✅     |
| Publisher liveliness token format correct               | ✅     |
| Subscriber liveliness token format correct              | ✅     |
| Service server liveliness token format correct          | ✅     |
| Service client liveliness token format correct          | ✅     |
| QoS string encoding matches rmw_zenoh                   | ✅     |
| ZenohId LSB-first hex encoding correct                  | ✅     |
| RMW attachment 33-byte format correct                   | ✅     |
| Service request/reply keyexpr format correct            | ✅     |

### API Compatibility Tests

| Test                                                | Status |
|-----------------------------------------------------|--------|
| Rust: `Context::default_from_env()` matches rclrs   | ✅     |
| Rust: `executor.create_node()` matches rclrs        | ✅     |
| Rust: `node.create_publisher()` matches rclrs       | ✅     |
| Rust: `node.create_subscription()` matches rclrs    | ✅     |
| Rust: `node.create_service()` matches rclrs         | ✅     |
| Rust: `node.create_client()` matches rclrs          | ✅     |
| Rust: `ParameterBuilder` API matches rclrs          | ✅     |
| C: `nano_ros_*_get_zero_initialized()` works        | ⬜     |
| C: `nano_ros_*_init_default()` works                | ⬜     |
| C: `nano_ros_executor_init()` pre-allocates handles | ✅     |
| C: `nano_ros_executor_spin_some()` works            | ✅     |
| C: Context callbacks work correctly                 | ✅     |

---

## Implementation Order

### Phase 16A: Rust API (First Priority)

```
A.1 Context/Executor ────────┐
                             │
A.2 Node API ────────────────┤
                             │
A.3 Publisher API ───────────┼──→ C.1 QoS Strings
                             │
A.4 Subscription API ────────┤
                             │
A.5 Service/Client API ──────┼──→ C.2 Parameter Services
                             │
A.6 Timer API ───────────────┤
                             │
A.7 Parameter API ───────────┤
                             │
A.8 QoS Profile API ─────────┤
                             │
A.9 Logger API ──────────────┤
                             │
A.10 Error Handling ─────────┘
```

### Phase 16B: C API (Second Priority)

```
B.1 Zero-Init Pattern ───────┐
                             │
B.2 Convenience Init ────────┤
                             │
B.3 Pre-allocated Executor ──┤
                             │
B.4 Callback Context ────────┤
                             │
B.5 Executor Semantics ──────┤
                             │
B.6 Handle Invocation ───────┤
                             │
B.7 Spin Functions ──────────┘
```

### Phase 16C: Protocol (Parallel with A/B)

```
C.1 QoS Strings ─────────────┐
                             │
C.2 Parameter Services ──────┤
                             │
C.3 Action Interop ──────────┤
                             │
C.4 Type Hash ───────────────┤
                             │
C.5 MessageInfo/Attachment ──┼──→ Enables A.4 MessageInfo population
                             │
C.6 Protocol Verification ───┤
                             │
C.7 Interop Test Suite ──────┘──→ CI validation
```

---

## Acceptance Criteria

Phase 16 is complete when:

1. **Rust API**: All A.* items pass their criteria
2. **C API**: All B.* items pass their criteria
3. **Protocol**: All C.* items pass their criteria
4. **Test Matrix**: All tests pass
5. **Examples**: Updated examples work with new API
6. **Documentation**: API docs match rclrs/rclc patterns
7. **CI**: `tests/ros2-interop.sh all` passes

---

## References

- [rclrs 0.7.0 Source](https://github.com/ros2-rust/ros2_rust) - `external/rclrs/`
- [rclc Source](https://github.com/ros2/rclc) - `external/rclc/`
- [rmw_zenoh Protocol Documentation](../rmw_zenoh_interop.md)
- [rmw_zenoh_cpp Source](https://github.com/ros2/rmw_zenoh)
- [rcl_interfaces Package](https://github.com/ros2/rcl_interfaces)
- [ROS 2 QoS Policies](https://docs.ros.org/en/humble/Concepts/Intermediate/About-Quality-of-Service-Settings.html)
