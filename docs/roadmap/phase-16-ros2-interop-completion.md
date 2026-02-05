# Phase 16: ROS 2 Interoperability Completion

**Status**: PLANNING
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
| `spin_async()`      | `spin()` / `spin_once()` only        | zenoh-pico not thread-safe                |
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

3. **No `spin_async()`**: async runtime integration requires `Send` futures. Since zenoh-pico types are `!Send`, async spinning is blocked. Use `spin_once()` in RTIC/Embassy tasks instead.

**API Compatibility Approach:**
- **Method signatures** match rclrs where possible
- **Ownership semantics** follow rclc (direct ownership, no Arc)
- **Embedded features** use const generics for static allocation

### Current State

| Feature    | nano-ros → nano-ros | nano-ros → ROS 2 | ROS 2 → nano-ros |
|------------|---------------------|------------------|------------------|
| Pub/Sub    | ✅ Working          | ⚠️ Partial        | ⚠️ Partial        |
| Services   | ✅ Working          | ⚠️ Untested       | ⚠️ Untested       |
| Actions    | ✅ Working          | ❌ Not Working   | ❌ Not Working   |
| Parameters | ✅ Working          | ❌ Not Working   | ❌ Not Working   |
| Discovery  | N/A                 | ❌ Not Working   | ❌ Not Working   |

### Root Causes

1. **Discovery**: nano-ros publishes liveliness tokens, but ROS 2 may not recognize them due to QoS string format issues
2. **Parameters**: ROS 2 parameter services not implemented
3. **Actions**: Action protocol partially implemented but not tested against ROS 2
4. **QoS**: Hardcoded BEST_EFFORT in liveliness tokens regardless of actual settings

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
- [ ] Add `qos()` method (pending QoS struct implementation in A.8)
- [x] Add `MessageInfo` type with timestamp and source GID
- [x] ~~Support `async fn(T)`~~ **N/A for embedded** - requires `Send` futures

**Implementation Notes**:
- `MessageInfo` currently returns default values as transport layer doesn't yet extract RMW attachment data on receive
- TODO: Wire up proper MessageInfo from transport layer (requires transport layer changes)

**Passing Criteria**:
- [x] `node.create_subscription("topic", |msg: &Int32| { ... })` compiles
- [x] `node.create_subscription_with_info("topic", |msg, info| { ... })` compiles
- [ ] `MessageInfo` contains valid timestamp and GID (pending transport layer integration)
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

### A.8 QoS Profile API (MEDIUM)

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
- [ ] Add `QoSProfile` struct matching rclrs fields
- [ ] Add predefined constants: `QOS_PROFILE_DEFAULT`, `QOS_PROFILE_SENSOR_DATA`, etc.
- [ ] Add builder methods: `.reliability()`, `.durability()`, `.history()`, `.depth()`
- [ ] Update publisher/subscription creation to accept `QoSProfile`

**Passing Criteria**:
- [ ] `QOS_PROFILE_SENSOR_DATA` has BEST_EFFORT reliability
- [ ] `QOS_PROFILE_DEFAULT` has RELIABLE reliability
- [ ] Custom QoS can be built with fluent API
- [ ] QoS is applied to created publishers/subscriptions

---

### A.9 Logger API (LOW)

**Goal**: Match rclrs `Logger` patterns.

**rclrs Reference** (`external/rclrs/rclrs/src/logging.rs`):
```rust
node.logger().info("Message");
node.logger().once().warn("Only logged once");
node.logger().throttle(Duration::from_secs(1)).debug("Rate limited");
```

**Tasks**:
- [ ] Add `Logger` type with log level methods
- [ ] Add modifiers: `.once()`, `.throttle(duration)`, `.skip_first()`
- [ ] Integrate with `log` crate or `defmt` for embedded
- [ ] Add `node.logger()` method

**Passing Criteria**:
- [ ] `node.logger().info("msg")` logs at INFO level
- [ ] `.once()` modifier only logs first occurrence
- [ ] `.throttle()` rate limits log output

---

### A.10 Error Handling (MEDIUM)

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

**Tasks**:
- [ ] Create `NanoRosError` enum with variants for all failure modes
- [ ] Add error codes matching RCL return codes
- [ ] Implement `std::error::Error` trait (when `std` available)
- [ ] Ensure all public APIs return `Result<T, NanoRosError>`

**Passing Criteria**:
- [ ] All public methods return `Result<_, NanoRosError>`
- [ ] Errors contain useful context (topic name, service name, etc.)
- [ ] Error messages are human-readable

---

## Part B: C API Alignment (rclc)

### B.1 Zero-Initialization Pattern (HIGH)

**Goal**: Match rclc zero-initialization pattern for embedded safety.

**rclc Reference** (`external/rclc/rclc/include/rclc/executor.h`):
```c
rclc_executor_t rclc_executor_get_zero_initialized_executor(void);
rcl_ret_t rclc_executor_init(rclc_executor_t * e, rcl_context_t * ctx, size_t num_handles, ...);
```

**Tasks**:
- [ ] Add `nano_ros_node_get_zero_initialized()` returning zeroed struct
- [ ] Add `nano_ros_publisher_get_zero_initialized()`
- [ ] Add `nano_ros_subscription_get_zero_initialized()`
- [ ] Add `nano_ros_executor_get_zero_initialized()`
- [ ] Document that users must call `_init()` after zero initialization

**Passing Criteria**:
- [ ] Zero-initialized structs have all members set to 0/NULL
- [ ] Calling `_init()` on zero-initialized struct succeeds
- [ ] Using uninitialized struct returns error (not crash)

---

### B.2 Convenience Initialization Functions (HIGH)

**Goal**: Match rclc `*_init_default()`, `*_init_best_effort()` patterns.

**rclc Reference** (`external/rclc/rclc/include/rclc/publisher.h`):
```c
rcl_ret_t rclc_publisher_init_default(rcl_publisher_t * pub, rcl_node_t * node, ...);
rcl_ret_t rclc_publisher_init_best_effort(rcl_publisher_t * pub, rcl_node_t * node, ...);
rcl_ret_t rclc_publisher_init(rcl_publisher_t * pub, rcl_node_t * node, ..., rmw_qos_profile_t * qos);
```

**Tasks**:
- [ ] Add `nano_ros_publisher_init_default()` - default QoS
- [ ] Add `nano_ros_publisher_init_best_effort()` - sensor data QoS
- [ ] Add `nano_ros_publisher_init()` - custom QoS
- [ ] Apply same pattern to subscription, service, client, action

**Passing Criteria**:
- [ ] `_init_default()` creates publisher with RELIABLE QoS
- [ ] `_init_best_effort()` creates publisher with BEST_EFFORT QoS
- [ ] `_init()` applies custom QoS profile correctly

---

### B.3 Pre-allocated Executor (HIGH)

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
- [ ] Add `nano_ros_executor_init()` with `number_of_handles` parameter
- [ ] Add `nano_ros_executor_add_subscription()` with pre-allocated message buffer
- [ ] Add `nano_ros_executor_add_timer()`
- [ ] Add `nano_ros_executor_add_service()`
- [ ] Add `nano_ros_executor_add_client()`
- [ ] Ensure no dynamic allocation after init

**Passing Criteria**:
- [ ] Executor init allocates exactly `number_of_handles` slots
- [ ] Adding more handles than allocated returns error
- [ ] No malloc/free calls during `spin_some()`
- [ ] Memory usage is predictable and bounded

---

### B.4 Callback Context Pattern (HIGH)

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
- [ ] Add `nano_ros_subscription_callback_t` typedef (msg only)
- [ ] Add `nano_ros_subscription_callback_with_context_t` typedef (msg + context)
- [ ] Add `nano_ros_executor_add_subscription_with_context()`
- [ ] Apply same pattern to service, client, timer callbacks

**Passing Criteria**:
- [ ] Callback without context receives only message pointer
- [ ] Callback with context receives message and context pointers
- [ ] Context pointer is passed unchanged to callback

---

### B.5 Executor Semantics (MEDIUM)

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
- [ ] Add `nano_ros_executor_semantics_t` enum
- [ ] Add `nano_ros_executor_set_semantics()` function
- [ ] Add `nano_ros_executor_set_timeout()` function
- [ ] Implement RCLCPP semantics (take data before callback)
- [ ] Implement LET semantics (take all data at sampling point)

**Passing Criteria**:
- [ ] RCLCPP semantics takes data immediately before callback
- [ ] LET semantics takes all data at start of spin cycle
- [ ] Timeout controls maximum wait time in spin_some

---

### B.6 Handle Invocation Types (MEDIUM)

**Goal**: Match rclc handle invocation configuration.

**rclc Reference** (`external/rclc/rclc/include/rclc/executor_handle.h`):
```c
typedef enum {
    RCLC_EXECUTOR_HANDLE_ALWAYS,
    RCLC_EXECUTOR_HANDLE_ON_NEW_DATA
} rclc_executor_handle_invocation_t;
```

**Tasks**:
- [ ] Add `nano_ros_handle_invocation_t` enum
- [ ] Support `ALWAYS` - callback called every spin
- [ ] Support `ON_NEW_DATA` - callback only when new data available
- [ ] Apply to subscription, service, client add functions

**Passing Criteria**:
- [ ] `ALWAYS` invokes callback even without new data
- [ ] `ON_NEW_DATA` only invokes when message received
- [ ] Invocation type can be set per-handle

---

### B.7 Spin Functions (HIGH)

**Goal**: Match rclc executor spin patterns.

**rclc Reference** (`external/rclc/rclc/include/rclc/executor.h`):
```c
rcl_ret_t rclc_executor_spin_some(rclc_executor_t * e, uint64_t timeout_ns);
rcl_ret_t rclc_executor_spin(rclc_executor_t * e);
rcl_ret_t rclc_executor_spin_period(rclc_executor_t * e, uint64_t period_ns);
```

**Tasks**:
- [ ] Add `nano_ros_executor_spin_some()` - one iteration with timeout
- [ ] Add `nano_ros_executor_spin()` - spin until shutdown
- [ ] Add `nano_ros_executor_spin_period()` - spin with fixed period
- [ ] Ensure spin functions are non-blocking when no data

**Passing Criteria**:
- [ ] `spin_some()` processes available callbacks and returns
- [ ] `spin()` blocks until shutdown signal
- [ ] `spin_period()` maintains consistent timing

---

## Part C: Protocol Interoperability

### C.1 QoS String Formatting (HIGH)

**Problem**: Liveliness tokens hardcode QoS as `2:2:1,1:,:,:,,` regardless of actual settings.

**Location**: `crates/nano-ros-transport/src/shim.rs:204,234`

**Tasks**:
- [ ] Add `to_qos_string()` method to `QosSettings`
- [ ] Map QoS values to rmw_zenoh format
- [ ] Update liveliness token generation to use actual QoS

**Passing Criteria**:
- [ ] RELIABLE publisher generates `1:...` in liveliness token
- [ ] BEST_EFFORT publisher generates `2:...` in liveliness token
- [ ] ROS 2 `ros2 topic info` shows correct QoS

---

### C.2 Parameter Services (HIGH)

**Problem**: ROS 2 CLI tools cannot interact with nano-ros parameters.

**Tasks**:
- [ ] Generate `rcl_interfaces` message types
- [ ] Implement `~/get_parameters`, `~/set_parameters`, etc. services
- [ ] Auto-register services on node creation

**Passing Criteria**:
- [ ] `ros2 param list <node>` shows nano-ros parameters
- [ ] `ros2 param get <node> <param>` returns correct value
- [ ] `ros2 param set <node> <param> <value>` updates parameter

---

### C.3 Action ROS 2 Interop (MEDIUM)

**Problem**: Actions work nano-ros ↔ nano-ros but not with ROS 2.

**Tasks**:
- [ ] Verify action key expression format matches rmw_zenoh
- [ ] Add action liveliness tokens
- [ ] Test with ROS 2 action client/server

**Passing Criteria**:
- [ ] `ros2 action list` shows nano-ros action servers
- [ ] `ros2 action send_goal` invokes nano-ros action server
- [ ] nano-ros action client can call ROS 2 action server

---

### C.4 Type Hash for Iron+ (MEDIUM)

**Problem**: Only Humble compatibility (uses `TypeHashNotSupported`).

**Tasks**:
- [ ] Compute RIHS01 type hashes in code generator
- [ ] Support Iron/Jazzy/Rolling ROS 2 versions

**Passing Criteria**:
- [ ] Generated types include correct RIHS01 hash
- [ ] Interop works with ROS 2 Iron

---

### C.5 RMW Attachment / MessageInfo Integration (HIGH)

**Problem**: `MessageInfo` in subscriptions returns default values. The transport layer doesn't extract RMW attachment data on receive.

**Background**: rmw_zenoh publishes messages with an attachment containing:
- `sequence_number` (i64) - incrementing per-publisher sequence
- `timestamp` (i64) - nanoseconds since epoch
- `gid` (16 bytes) - publisher Global Identifier

Currently nano-ros serializes this attachment when publishing but doesn't deserialize it on receive.

**Location**:
- `crates/nano-ros-transport/src/shim.rs` - ShimSubscriber callback
- `crates/nano-ros-transport/src/traits.rs` - Subscriber trait
- `crates/nano-ros-node/src/connected.rs` - ConnectedSubscriber::try_recv_with_info()

**Tasks**:
- [ ] Extend zenoh-pico-shim C callback to pass attachment data alongside payload
- [ ] Update `SubscriberBuffer` to store attachment (33 bytes: 8+8+1+16)
- [ ] Add `try_recv_raw_with_attachment()` to `Subscriber` trait
- [ ] Parse RMW attachment format in transport layer (VLE-encoded GID length)
- [ ] Populate `MessageInfo` fields from parsed attachment in `try_recv_with_info()`
- [ ] Add unit tests for attachment parsing

**Passing Criteria**:
- [ ] `info.source_timestamp()` returns actual publisher timestamp
- [ ] `info.publication_sequence_number()` returns incrementing sequence
- [ ] `info.publisher_gid()` returns 16-byte GID matching publisher
- [ ] MessageInfo works for both nano-ros → nano-ros and ROS 2 → nano-ros

---

### C.6 RMW Zenoh Protocol Verification (HIGH)

**Problem**: Need comprehensive verification that nano-ros protocol implementation matches rmw_zenoh_cpp exactly.

**Reference**: `docs/rmw_zenoh_interop.md`, rmw_zenoh_cpp source

**Tasks**:

**Data Key Expression Format**:
- [ ] Verify format: `<domain>/<topic>/<type>/<hash>` matches rmw_zenoh
- [ ] Test with various topic names (namespaced, nested)
- [ ] Verify type name encoding (double colons, `dds_::` suffix)
- [ ] Test Humble format (`TypeHashNotSupported`) works bidirectionally

**Liveliness Token Format**:
- [ ] Verify node token: `@ros2_lv/<domain>/<zid>/0/0/NN/%/%/<node>`
- [ ] Verify publisher token: `@ros2_lv/<domain>/<zid>/0/11/MP/%/%/<node>/%<topic>/<type>/RIHS01_<hash>/<qos>`
- [ ] Verify subscriber token: `@ros2_lv/<domain>/<zid>/0/11/MS/%/%/<node>/%<topic>/<type>/RIHS01_<hash>/<qos>`
- [ ] Verify service server token format
- [ ] Verify service client token format
- [ ] Test ZenohId is LSB-first hex format
- [ ] Test topic names use `%` prefix correctly

**RMW Attachment Format**:
- [ ] Verify attachment serialization matches rmw_zenoh zenoh serializer format
- [ ] Test sequence_number little-endian encoding (8 bytes)
- [ ] Test timestamp little-endian encoding (8 bytes)
- [ ] Test VLE-encoded GID length (1 byte for 16)
- [ ] Test GID bytes (16 bytes)
- [ ] Verify total attachment size is 33 bytes

**Service Protocol**:
- [ ] Verify service request key expression format
- [ ] Verify service reply key expression format
- [ ] Test request/reply sequence number matching
- [ ] Verify service attachment format

**QoS Encoding**:
- [ ] Map reliability: RELIABLE=1, BEST_EFFORT=2
- [ ] Map durability: VOLATILE=2, TRANSIENT_LOCAL=1
- [ ] Map history: KEEP_LAST=1, KEEP_ALL=2
- [ ] Verify QoS string format: `reliability:durability:history,depth:deadline:lifespan:liveliness,lease:avoid_ros_namespace_conventions`

**Passing Criteria**:
- [ ] `ros2 topic list` shows nano-ros publishers with correct names
- [ ] `ros2 topic info <topic> -v` shows correct QoS settings
- [ ] `ros2 node list` shows nano-ros nodes
- [ ] `ros2 node info <node>` shows correct publishers/subscribers
- [ ] Bidirectional pub/sub works at all QoS combinations
- [ ] Services work bidirectionally
- [ ] Discovery is reliable (no missing nodes/topics)

---

### C.7 End-to-End Interop Test Suite (HIGH)

**Problem**: Need automated tests to catch protocol regressions.

**Tasks**:
- [ ] Create `tests/ros2-interop/` test directory
- [ ] Add test script `run-interop-tests.sh` that:
  - Starts zenohd router
  - Launches nano-ros publisher, verifies ROS 2 subscriber receives
  - Launches ROS 2 publisher, verifies nano-ros subscriber receives
  - Tests service call in both directions
  - Reports pass/fail for each test case
- [ ] Add CI job to run interop tests (requires ROS 2 + rmw_zenoh in CI)
- [ ] Create test fixtures for common message types (Int32, String, custom)
- [ ] Add latency and throughput benchmarks

**Test Cases**:
```
pub-sub/nano-to-ros2-int32
pub-sub/nano-to-ros2-string
pub-sub/nano-to-ros2-custom-msg
pub-sub/ros2-to-nano-int32
pub-sub/ros2-to-nano-string
pub-sub/ros2-to-nano-custom-msg
service/nano-server-ros2-client
service/ros2-server-nano-client
discovery/nano-node-visible-to-ros2
discovery/nano-pub-visible-to-ros2
discovery/nano-sub-visible-to-ros2
qos/reliable-to-reliable
qos/best-effort-to-best-effort
qos/reliable-to-best-effort (should work)
qos/best-effort-to-reliable (should fail gracefully)
```

**Passing Criteria**:
- [ ] All test cases pass consistently
- [ ] Tests complete within reasonable timeout (30s per test)
- [ ] CI catches protocol regressions before merge

---

## Test Matrix

### Protocol Tests

| Test                                            | Status |
|-------------------------------------------------|--------|
| `ros2 topic list` shows nano-ros publishers     | ⬜     |
| `ros2 topic echo` receives nano-ros messages    | ⬜     |
| nano-ros subscriber receives `ros2 topic pub`   | ⬜     |
| `ros2 service list` shows nano-ros services     | ⬜     |
| `ros2 service call` invokes nano-ros server     | ⬜     |
| nano-ros client calls ROS 2 service             | ⬜     |
| `ros2 action list` shows nano-ros actions       | ⬜     |
| `ros2 action send_goal` invokes nano-ros server | ⬜     |
| nano-ros client calls ROS 2 action server       | ⬜     |
| `ros2 param list` shows nano-ros parameters     | ⬜     |
| `ros2 param get/set` works with nano-ros        | ⬜     |

### MessageInfo / Attachment Tests

| Test                                                  | Status |
|-------------------------------------------------------|--------|
| Attachment parsing extracts sequence_number correctly | ⬜     |
| Attachment parsing extracts timestamp correctly       | ⬜     |
| Attachment parsing extracts GID correctly             | ⬜     |
| MessageInfo populated from nano-ros publisher         | ⬜     |
| MessageInfo populated from ROS 2 publisher            | ⬜     |
| Sequence numbers increment correctly per-publisher    | ⬜     |
| GID is consistent across messages from same publisher | ⬜     |

### Protocol Format Tests

| Test                                                    | Status |
|---------------------------------------------------------|--------|
| Data keyexpr format matches rmw_zenoh                   | ⬜     |
| Node liveliness token format correct                    | ⬜     |
| Publisher liveliness token format correct               | ⬜     |
| Subscriber liveliness token format correct              | ⬜     |
| Service server liveliness token format correct          | ⬜     |
| Service client liveliness token format correct          | ⬜     |
| QoS string encoding matches rmw_zenoh                   | ⬜     |
| ZenohId LSB-first hex encoding correct                  | ⬜     |
| RMW attachment 33-byte format correct                   | ⬜     |
| Service request/reply keyexpr format correct            | ⬜     |

### API Compatibility Tests

| Test                                                | Status |
|-----------------------------------------------------|--------|
| Rust: `Context::default_from_env()` matches rclrs   | ⬜     |
| Rust: `executor.create_node()` matches rclrs        | ⬜     |
| Rust: `node.create_publisher()` matches rclrs       | ⬜     |
| Rust: `node.create_subscription()` matches rclrs    | ⬜     |
| Rust: `node.create_service()` matches rclrs         | ⬜     |
| Rust: `node.create_client()` matches rclrs          | ⬜     |
| Rust: `ParameterBuilder` API matches rclrs          | ⬜     |
| C: `nano_ros_*_get_zero_initialized()` works        | ⬜     |
| C: `nano_ros_*_init_default()` works                | ⬜     |
| C: `nano_ros_executor_init()` pre-allocates handles | ⬜     |
| C: `nano_ros_executor_spin_some()` works            | ⬜     |
| C: Context callbacks work correctly                 | ⬜     |

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
