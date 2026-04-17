# Client Library Model: nros vs rclc / rclcpp / rclrs

The RMW layer decouples the transport from the client library. The *client library* is the user-facing API on top of RMW: nodes, executors, publishers, subscribers, services, clients, callbacks, futures. ROS 2 ships three: **rclc** (C, MCU-focused), **rclcpp** (C++, desktop), and **rclrs** (Rust, experimental). nano-ros adds a fourth -- the `nros-node` crate plus its `nros-c` / `nros-cpp` wrappers.

This page explains the shape of nros-node and how it differs from the three official client libraries. For trait signatures see [Rust API](../reference/rust-api.md) / [C API](../reference/c-api.md) / [C++ API](../reference/cpp-api.md).

## Side-by-side

| Concern | rclc | rclcpp | rclrs | nros |
|---|---|---|---|---|
| Target | MCU (Micro-XRCE-DDS) | Desktop / robot | Desktop (alpha) | MCU + RTOS + desktop |
| Language | C99 | C++17 | Rust (std) | Rust (`no_std`) + C + C++14 |
| Node ownership | App owns node + executor | `shared_ptr<Node>` + `Executor` | `Arc<Node>` + executor | Executor owns the session; node borrows from it |
| Executor model | Single-threaded, polled | Single-/multi-threaded, owned thread | Single-threaded, polled | Single-threaded, polled |
| Spin entry point | `rclc_executor_spin_some(&exec, timeout)` | `rclcpp::spin(node)` (blocks, owns thread) | `executor.spin()` (blocks current thread) | `executor.spin_once(timeout_ms)` |
| Blocking primitive | None (callback-only) | `std::shared_future::wait_for(timeout)` | `Promise::wait(timeout)` (Tokio-style) | `Promise::wait(&mut executor, timeout_ms)` |
| Async primitive | None | `std::shared_future` + `spin_until_future_complete` | `impl Future` | `Promise<T>` (impls `Future` + manual poll + executor-driven blocking wait) |
| Heap requirement | Optional (static allocator) | Required | Required | Optional (caller buffers; only zenoh-pico transport heap) |
| Threading requirement | None | Required (`std::thread`) | Required (`std::thread`) | None (single-threaded valid) |

## Future / Promise as the unifying primitive

The same `Promise<T>` value serves three usage patterns. None of the other client libraries achieves this with a single type.

**Pattern 1 -- callback-driven (no blocking call):**

```rust,ignore
let promise = client.call(&request)?;
// ... continue running other work ...
executor.spin_once(10);
if let Some(reply) = promise.try_recv()? {
    handle(reply);
}
```

**Pattern 2 -- blocking with timeout:**

```rust,ignore
let mut promise = client.call(&request)?;
let reply = promise.wait(&mut executor, 5000)?;  // blocks up to 5s, drives I/O
```

**Pattern 3 -- async runtime (Tokio, Embassy, smol):**

```rust,ignore
let reply = client.call(&request)?.await;  // Promise: Future<Output = T>
```

Contrast this with the alternatives:

- **rclcpp** uses `std::shared_future<Response>`, which is bound to `std::thread` and `std::condition_variable`. There is no way to drive it on a cooperative single-threaded MCU. The blocking helper `rclcpp::spin_until_future_complete()` *requires* an owned thread.
- **rclc** has no future or promise concept. The only way to receive a service reply is to register a callback that fires from `rclc_executor_spin_some()`. Building request/response chains becomes an explicit state machine.
- **rclrs** has `impl Future` but no callback-poll path; you must `.await` from a Tokio task. No blocking-with-timeout helper that drives I/O internally.

In nano-ros, the same `Promise` carries a borrow of the standalone service client and a sequence number; `try_recv()` polls it, `wait()` polls it in a loop while spinning the executor, and the `Future` impl integrates with any executor that calls `register_waker(&Waker)` underneath.

## No internal spin

`rclcpp::spin(node)` blocks the calling thread inside the library and owns the dispatch loop. This works on Linux but cannot work on bare-metal: there is no thread to give up, and the application loop must remain in user code (for power management, interrupt servicing, smoltcp polling, RTIC integration).

nano-ros never owns the spin loop. The user *always* writes the loop:

```rust,ignore
loop {
    executor.spin_once(10);
    // user can also: poll smoltcp, service interrupts, run other tasks, sleep
}
```

Convenience wrappers (`spin(count)`, `spin_blocking(opts)`, `spin_period(duration)`) exist for desktop-style use cases, but each is implemented as a `spin_once()` loop that the user could write by hand. On `no_std` targets they aren't available -- the user writes the loop.

This is the reason every blocking API in nros takes `&mut Executor`. `Promise::wait`, `Stream::wait_next`, `Client::call_blocking`, the C++ `Future::wait(executor.handle(), ...)`, and the C `nros_call_service(..., timeout_ms)` all internally call `spin_once()` to keep I/O moving while waiting. They cannot rely on a background thread doing it for them, because there is none.

## Executor is the session owner, not a singleton

In rclcpp, the `Executor` is a *separate* object that you attach nodes to (`exec.add_node(node)`). The node owns its own RMW context. Multiple executor classes (`SingleThreadedExecutor`, `MultiThreadedExecutor`, `StaticSingleThreadedExecutor`) exist as parallel implementations.

In nano-ros, the `Executor` *is* the RMW session owner. Calling `Executor::open(&config)` opens the transport; nodes are derived from the executor:

```rust,ignore
let mut executor = Executor::open(&config)?;
let mut node = executor.create_node("my_node")?;
let pub_ = node.create_publisher::<Int32>("/topic")?;
```

There is exactly one executor type. There is no separate "context" object, no add/remove-node lifecycle, no executor selection at runtime. The single-threaded model is baked in -- adding a multi-threaded variant would require redesigning the callback dispatch, which we deliberately avoid to keep the bare-metal path viable.

The trade-off: you cannot share one transport session between two executors. In practice no embedded application wants this, and on desktop you can run multiple processes if you need it.

## Explicit spin in blocking ops -- why pass the executor?

Every blocking operation takes `&mut Executor` (Rust) or an executor handle (C/C++). This looks redundant when the executor is also the entity that created the operation. The reason is borrow-checker hygiene plus single-threaded I/O.

The standalone communication handles (`StandaloneClient`, `StandaloneSubscription`, the `Promise` returned from `call`) borrow from the session. They cannot also borrow the executor at the same time, because the executor *owns* the session. Passing `&mut executor` at the wait call -- after the promise has been created and the borrow released -- is the only way to get a mutable executor reference while a promise is in flight.

The deeper reason: there is no other thread that can drive I/O. If `Promise::wait()` did not have the executor, it could not call `spin_once()` -- the network would freeze and the wait would always time out. By forcing the executor parameter, the API makes the I/O dependency explicit and impossible to forget.

## Language parity

The same Future/Promise + explicit-executor model is preserved in C and C++ wrappers.

**Rust** uses `Promise<'_, T>` directly. It implements `core::future::Future` and exposes `try_recv()` + `wait(&mut executor, ms)`.

**C++** wraps the promise as `nros::Future<T>`:

```cpp
auto fut = client.send_request(req);
ResponseType resp;
NROS_TRY(fut.wait(executor.handle(), 5000, resp));
```

`Future::wait()` takes `void* executor_handle` (from `executor.handle()` or `nros::global_handle()`) for the same reason the Rust API takes `&mut executor`. There is no global executor singleton; the handle is explicit.

**C** uses paired `_async` and blocking entry points:

```c
// Blocking: drives the executor internally, returns reply or timeout.
nros_call_service(client, &req, sizeof(req), &reply, sizeof(reply), 5000);

// Async: returns immediately, result via callback registered ahead of time.
nros_action_send_goal_async(client, &goal, sizeof(goal));
nros_action_client_set_result_callback(client, on_result);
// User keeps calling nros_spin_once() to drive callbacks.
```

The C API has no Future/Promise type because C lacks generics, but the *pattern* is the same: an `_async` send-without-wait paired with a callback (or polled status), and a blocking call that drives the executor for you.

## Summary

The four design choices that shape nano-ros's client library:

1. **Single executor type** that owns the session -- no separate context object, no executor variants.
2. **No internal spin** -- the user always owns the spin loop; blocking helpers exist but are wrappers.
3. **Explicit `&mut Executor` on every blocking op** -- the API enforces the I/O dependency at the call site.
4. **Future/Promise as the unifying primitive** -- one type for callback-drive, blocking-with-timeout, and `.await`.

These choices are what make the same client library viable on a Cortex-M3 and a Linux workstation without a separate "MCU client library" like rclc.
