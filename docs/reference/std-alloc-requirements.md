# std and alloc Requirements

This document maps which parts of the nano-ros API require the `std` or `alloc`
Cargo features. All core crates are `#![no_std]` by default and gate
std/alloc-dependent code behind feature flags.

## Feature Hierarchy

```
std  (default)
 └─ alloc
     └─ (base no_std)
```

Enabling `std` automatically enables `alloc`. Enabling `alloc` does not enable
`std`. With `--no-default-features`, the entire library compiles without
std or alloc.

## Summary by Crate

| Crate       | no_std base                                                                                                                          | alloc additions                                                     | std additions                                                                             |
|-------------|--------------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------|-------------------------------------------------------------------------------------------|
| nros-serdes | CDR ser/de for primitives, `heapless` types, `&str`, `[T; N]`                                                                        | `String` and `Vec<T>` ser/de                                        | (none)                                                                                    |
| nros-core   | Time, Duration, Clock (atomic fallback), lifecycle, logger, error types, action types                                                | (none)                                                              | `Clock::now()` via `SystemTime`, `std::error::Error` impls                                |
| nros-rmw    | All traits, QoS, sync primitives, safety/E2E protocol                                                                                | `handle_request_boxed()` (Box\<Reply\>)                             | (none)                                                                                    |
| nros-params | `ParameterServer`, `ParameterValue`, all parameter types (heapless)                                                                  | (none)                                                              | `ParameterVariant` impls for `std::string::String`, `std::vec::Vec`                       |
| nros-node   | `Executor::open()`, `create_node()`, `spin_once()`, `spin_async()`, `Promise`, pub/sub/service/action, timers (fn pointer callbacks) | Boxed timer callbacks, `handle_request_boxed()`, parameter services | `spin_blocking()`, `spin_period()`, `ExecutorConfig::from_env()`, halt flag |
| nros        | Re-exports from above                                                                                                                | (same as above)                                                     | `SpinPeriodResult` re-export                                                |

### RMW Backend Crates

| Crate          | no_std base                                                   | alloc additions | std additions |
|----------------|---------------------------------------------------------------|-----------------|---------------|
| nros-rmw-zenoh | Zenoh-pico RMW implementation, all pub/sub/service/action ops | (none)          | (none)        |
| zpico-sys      | FFI bindings to zenoh-pico C library                          | (none)          | (none)        |
| nros-rmw-xrce  | XRCE-DDS RMW implementation, all pub/sub/service/action ops   | (none)          | (none)        |
| xrce-sys       | FFI bindings to Micro-XRCE-DDS-Client C library               | (none)          | (none)        |

All four backend crates are unconditionally `#![no_std]` and do not use `alloc`. The `std`
feature gates only `extern crate std` (for macro availability in transport modules) and
is propagated through the feature chain but does not add any API surface.

## Detailed API Availability

### Always Available (no_std, no alloc)

**Executor and Node:**
- `Executor::<S, MAX_CBS, CB_ARENA>::open(&config)` — open RMW session
- `ExecutorConfig::new(locator)` — manual configuration
- `executor.create_node(name)` — create a node
- `executor.spin_once(timeout_ms)` — single spin iteration
- `executor.spin_period_polling(period_ms)` — periodic spin without `std::thread::sleep`
- `executor.add_subscription()`, `add_service()`, `add_timer()`, `add_action_server()`, `add_action_client()` — arena-based callbacks

**Publish/Subscribe:**
- `node.create_publisher::<M>(topic)` — typed publisher
- `node.create_subscription::<M>(topic)` — typed subscription (poll with `try_recv()`)
- `publisher.publish(&msg)` / `publish_raw(&bytes)` — publish messages

**Services:**
- `node.create_service::<S>(name)` — service server (poll with `handle_request()`)
- `node.create_client::<S>(name)` — service client
- `client.call(&request)` — non-blocking, returns `Promise<Reply>`
- `promise.try_recv()` — poll for reply (returns `Ok(Some(reply))` or `Ok(None)`)
- `promise.await` — async poll (implements `core::future::Future`)

**Actions:**
- `node.create_action_server::<A>(name)` / `node.create_action_client::<A>(name)`
- `action_client.send_goal(&goal)` — returns `Promise<GoalId>`
- `action_client.cancel_goal(&goal_id)` — returns `Promise<CancelResponse>`
- `action_client.get_result(&goal_id)` — returns `Promise<(GoalStatus, Result)>`
- Full goal lifecycle: send, cancel, get result, feedback, status

**Async:**
- `executor.spin_async()` — async spin loop (drives I/O, dispatches callbacks, yields between iterations)
- `Promise<'a, T, Cli>` — allocation-free promise, borrows client's reply slot
- `Promise::try_recv()` — non-blocking poll for reply
- `Promise: Future` — implements `core::future::Future` for `.await`
- Uses only `core::future` and `core::task` — no external async runtime dependency

**Timers:**
- `TimerHandle` with function pointer callbacks (`fn()`)
- `TimerDuration`, `TimerMode`, `TimerState`

**Serialization:**
- `CdrWriter` / `CdrReader` — CDR serialization to/from byte buffers
- `Serialize` / `Deserialize` traits
- Implementations for: `bool`, `u8`-`u64`, `i8`-`i64`, `f32`, `f64`, `char`, `&str`, `[T; N]`, `heapless::String<N>`, `heapless::Vec<T, N>`

**Time:**
- `Time::new()`, `Time::from_nanos()`, `Time::to_nanos()`
- `Time::from_secs_f64()`, `Time::to_secs_f64()`
- `Duration::new()`, `Duration::from_nanos()`, `Duration::to_nanos()`
- `Duration::from_secs_f64()`, `Duration::to_secs_f64()`
- `Clock` with atomic counter fallback (no wall-clock time)

**Parameters (local only):**
- `ParameterServer` — store and retrieve parameters
- `ParameterValue` enum with heapless collections
- `ParameterDescriptor`, `ParameterType`, `Parameter`
- `ParameterBuilder` for declaring parameters with constraints

**Other:**
- `LifecycleState`, `LifecycleTransition`, `LifecyclePollingNode`
- `Logger` (uses `core::sync::atomic`)
- `GoalId`, `GoalStatus`, `GoalResponse`, `CancelResponse`
- `QosSettings`, `TopicInfo`, `ServiceInfo`
- `SafetyValidator`, `IntegrityStatus` (with `safety-e2e` feature)
- Sync primitives: `spin::Mutex` or `critical-section` (feature-selected)

### Requires `alloc`

| API                                              | Location                        | Why                                                   |
|--------------------------------------------------|---------------------------------|-------------------------------------------------------|
| `Serialize`/`Deserialize` for `String`, `Vec<T>` | nros-serdes                     | Heap-allocated containers                             |
| `TimerCallback` (`Box<dyn FnMut() + Send>`)      | nros-node/timer.rs              | Boxed closure for timer callbacks                     |
| `Timer::new_with_box()`, `set_callback_box()`    | nros-node/timer.rs              | Construct/update boxed timer callbacks                |
| `ServiceServerHandle::handle_request_boxed()`    | nros-node/handles.rs            | Returns `Box<Reply>` for large response types         |
| `param-services` feature (all of it)             | nros-node/parameter_services.rs | Service response types (~1MB) require heap allocation |

**Parameter services detail:** The `param-services` feature (which implies `alloc`)
provides ROS 2 parameter service handlers for `~/get_parameters`,
`~/set_parameters`, etc. Response types like `GetParametersResponse` contain
`heapless::Vec<ParameterValue, 64>` — each `ParameterValue` is large, making the
total response ~1MB. `Box<Response>` is required to avoid stack overflow.
The core `ParameterServer` API works without alloc; only the ROS 2 service
protocol layer requires it.

### Requires `std`

| API                                                           | Location             | Why                                                     |
|---------------------------------------------------------------|----------------------|---------------------------------------------------------|
| `Clock::now()` (system/steady clock)                          | nros-core/clock.rs   | Uses `std::time::SystemTime` / `UNIX_EPOCH`             |
| `std::error::Error` for `NanoRosError`, `RclReturnCode`       | nros-core/error.rs   | Trait requires std                                      |
| `ExecutorConfig::from_env()`                                  | nros-node/types.rs   | Uses `std::env::var()` + `Box::leak()`                  |
| `Executor::spin_blocking(options)`                            | nros-node/spin.rs    | Uses `std::thread::sleep()`, `Arc<AtomicBool>`          |
| `Executor::spin_period(duration)`                             | nros-node/spin.rs    | Uses `std::time::Instant`, `std::thread::sleep()`       |
| `Executor::halt_flag()`                                       | nros-node/spin.rs    | Returns `Arc<AtomicBool>` for cross-thread cancellation |
| `SpinPeriodResult`                                            | nros-node/types.rs   | Contains `std::time::Duration`                          |
| `ParameterVariant` for `std::string::String`, `std::vec::Vec` | nros-params/types.rs | Convenience conversions for std types                   |

## Typical Configurations

**Bare-metal / RTOS (no allocator):**
```toml
nros = { version = "*", default-features = false, features = ["rmw-zenoh", "platform-bare-metal"] }
```
Full pub/sub, services, actions, timers (fn pointers), parameters (local).
Async: `spin_async()`, `Promise`, `try_recv()`, `.await` — all available without std or alloc.
Use `spin_once()` or `spin_period_polling()` in your main loop, or `spin_async()` with an async runtime (Embassy, RTIC v2).

**Embedded with allocator (e.g., Zephyr with heap):**
```toml
nros = { version = "*", default-features = false, features = ["alloc", "rmw-zenoh", "platform-zephyr"] }
```
Adds boxed timer callbacks and `handle_request_boxed()` for large service replies.

**Desktop / Linux:**
```toml
nros = { version = "*", features = ["rmw-zenoh", "platform-posix"] }
```
Full API including `spin_blocking()`, `spin_period()`, `from_env()`, system clock.
For async, use an external runtime (tokio `current_thread` + `spawn_local` for background spin).

**Desktop with parameter services:**
```toml
nros = { version = "*", features = ["rmw-zenoh", "platform-posix", "param-services"] }
```
Adds `~/get_parameters`, `~/set_parameters`, etc. for `ros2 param` CLI interop.

## C-Level Allocation

Both RMW backends compile and link C libraries that perform heap allocation
independently of Rust's `alloc` feature. Disabling the Rust `alloc` feature
eliminates Rust-side heap usage (`Box`, `Vec`, `String`) but does **not**
eliminate allocation in the C transport layer.

| Backend   | C Library            | Allocator by Platform                                                                            |
|-----------|----------------------|--------------------------------------------------------------------------------------------------|
| rmw-zenoh | zenoh-pico 1.6.2     | POSIX: `malloc`; Zephyr: `k_malloc`; FreeRTOS: `pvPortMalloc`; bare-metal: custom bump allocator |
| rmw-xrce  | Micro-XRCE-DDS 3.0.1 | POSIX: `malloc`; Zephyr: `k_malloc`; FreeRTOS: `pvPortMalloc`; NuttX: `malloc`                   |

This is by design: the C libraries manage their own session state, stream
buffers, and protocol metadata using platform-provided allocators. The Rust
`alloc` feature controls only the Rust API surface (boxed callbacks, heap
containers, etc.) and is orthogonal to C-level memory management.
