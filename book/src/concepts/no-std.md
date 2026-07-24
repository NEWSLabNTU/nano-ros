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
- `Executor::open(&config)` -- open RMW session
- `ExecutorConfig::new(locator)` -- manual configuration
- `executor.create_node(name)` -- create a node
- `executor.spin_once(timeout_ms)` -- single spin iteration
- `executor.spin_period_polling(period_ms)` -- periodic spin without `std::thread::sleep`

**Two-layer API.** unified the verb discipline:

- **Layer 1 (caller polls)** -- `Node::create_*` returns an owned
  handle. Caller drives `try_recv` / `call` / `try_accept_goal` /
  `try_recv_request_raw` itself. Good for RTIC, Embassy,
  task-per-entity FreeRTOS.
- **Layer 2 (executor dispatches)** -- `Executor::register_*`
  takes a closure; `spin_once` fires it on rx / reply / timer.
  Good for callback-shaped applications.

Both layers share the same session; mix per entity.

**Publish/Subscribe:**
- L1 — `node.create_publisher::<M>(topic)`,
  `node.create_subscription::<M>(topic)` (poll with `try_recv()`)
- L2 — `executor.register_timer(period, || publisher.publish(...))`,
  `executor.register_subscription::<M, _>(topic, |msg| { ... })`
- `publisher.publish(&msg)` / `publish_raw(&bytes)` — publish messages

**Services:**
- L1 — `node.create_service::<S>(name)` (poll with
  `handle_request()`), `node.create_client::<S>(name)` +
  `client.call(&request)` → `Promise<Reply>` (poll with
  `promise.try_recv()` or `.await`).
- L2 — `executor.register_service::<S, _>(name, |req| reply)`.
  Service clients keep the L1 `Promise` shape; the typed
  callback API isn't surfaced (only `register_service_client_raw`
  exists for byte-level use).

**Actions:**
- L1 — `node.create_action_server::<A>(name)` +
  `try_accept_goal` / `complete_goal`, or
  `node.create_action_client::<A>(name)` + `send_goal` →
  `Promise<GoalId>` / `get_result` → `Promise<(GoalStatus,
  Result)>`.
- L2 — `executor.register_action_server::<A, _, _>(name,
  goal_cb, cancel_cb)` returns a handle for publishing feedback
  and completing goals. Action clients keep the L1 `Promise`
  shape for the same reason as service clients.

**Async:**
- `executor.spin_async()` -- async spin loop (drives I/O, dispatches callbacks, yields between iterations)
- `Promise<'a, T, Cli>` -- allocation-free promise, borrows client's reply slot
- `Promise::try_recv()` -- non-blocking poll for reply
- `Promise: Future` -- implements `core::future::Future` for `.await`
- Uses only `core::future` and `core::task` -- no external async runtime dependency

**Timers:**
- `TimerHandle` with function pointer callbacks (`fn()`)
- `TimerDuration`, `TimerMode`, `TimerState`

**Serialization:**
- `CdrWriter` / `CdrReader` -- CDR serialization to/from byte buffers
- `Serialize` / `Deserialize` traits
- Implementations for: `bool`, `u8`-`u64`, `i8`-`i64`, `f32`, `f64`, `char`, `&str`, `[T; N]`, `heapless::String<N>`, `heapless::Vec<T, N>`

**Time:**
- `Time::new()`, `Time::from_nanos()`, `Time::to_nanos()`
- `Time::from_secs_f64()`, `Time::to_secs_f64()`
- `Duration::new()`, `Duration::from_nanos()`, `Duration::to_nanos()`
- `Duration::from_secs_f64()`, `Duration::to_secs_f64()`
- `Clock` with atomic counter fallback (no wall-clock time)

**Parameters (local only):**
- `ParameterServer` -- store and retrieve parameters
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
| `EmbeddedServiceServer::handle_request_boxed()`    | nros-node/handles.rs            | Returns `Box<Reply>` for large response types         |
| `param-services` feature (all of it)             | nros-node/parameter_services.rs | Service response types (~1MB) require heap allocation |

**Parameter services detail:** The `param-services` feature (which implies `alloc`)
provides ROS 2 parameter service handlers for `~/get_parameters`,
`~/set_parameters`, etc. Response types like `GetParametersResponse` contain
`heapless::Vec<ParameterValue, 64>` -- each `ParameterValue` is large, making the
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

## Typical Configurations decoupled the `nros` umbrella from concrete RMW crates.
A consuming `Cargo.toml` lists **three** path deps: `nros` (with
`rmw-cffi` + a `platform-*` feature), the chosen backend crate
(`nros-rmw-zenoh` / `nros-rmw-xrce-cffi`), and —
on POSIX — `nros-platform-cffi` with `posix-c-port` so the C
`nros_platform_*` symbols link into a pure-cargo build. The backend
crate's `#[ctor]` registers its vtable before `main`.

**Bare-metal / RTOS (no allocator):**
```toml
nros = { path = "…/nros", default-features = false, features = ["rmw-cffi", "platform-bare-metal"] }
nros-rmw-zenoh = { path = "…/nros-rmw-zenoh", features = ["platform-bare-metal"] }
```
Full pub/sub, services, actions, timers (fn pointers), parameters (local).
Async: `spin_async()`, `Promise`, `try_recv()`, `.await` -- all available without std or alloc.
Use `spin_once()` or `spin_period_polling()` in your main loop, or `spin_async()` with an async runtime (Embassy, RTIC v2).

**Embedded with allocator (e.g., Zephyr with heap):**
```toml
nros = { path = "…/nros", default-features = false, features = ["alloc", "rmw-cffi", "platform-zephyr"] }
nros-rmw-zenoh = { path = "…/nros-rmw-zenoh", features = ["platform-zephyr"] }
```
Adds boxed timer callbacks and `handle_request_boxed()` for large service replies.

**Desktop / Linux:**
```toml
nros = { path = "…/nros", default-features = false, features = ["std", "rmw-cffi", "platform-posix"] }
nros-rmw-zenoh = { path = "…/nros-rmw-zenoh", features = ["platform-posix", "link-tcp", "ros-humble"] }
nros-platform-cffi = { path = "…/nros-platform-cffi", features = ["posix-c-port"] }
```
Full API including `spin_blocking()`, `spin_period()`, `from_env()`, system clock.
For async, use an external runtime (tokio `current_thread` + `spawn_local` for background spin).

**Desktop with parameter services:** add `param-services` to the
`nros` feature list above. Adds `~/get_parameters`,
`~/set_parameters`, etc. for `ros2 param` CLI interop.

## C-Level Allocation

Both RMW backends compile and link C libraries that perform heap allocation
independently of Rust's `alloc` feature. Disabling the Rust `alloc` feature
eliminates Rust-side heap usage (`Box`, `Vec`, `String`) but does **not**
eliminate allocation in the C transport layer.

| Backend crate | C Library | Allocator by Platform |
|---|---|---|
| `nros-rmw-zenoh` | zenoh-pico 1.7.2 | POSIX: `malloc`; Zephyr: `k_malloc`; FreeRTOS: `pvPortMalloc`; bare-metal: custom bump allocator |
| `nros-rmw-xrce-cffi` | Micro-XRCE-DDS 3.0.1 | POSIX: `malloc`; Zephyr: `k_malloc`; FreeRTOS: `pvPortMalloc`; NuttX: `malloc` |

This is by design: the C libraries manage their own session state, stream
buffers, and protocol metadata using platform-provided allocators. The Rust
`alloc` feature controls only the Rust API surface (boxed callbacks, heap
containers, etc.) and is orthogonal to C-level memory management.
