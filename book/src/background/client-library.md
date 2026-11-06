# The client library

<!-- type: explanation · level: 3 -->

A nano-ros service client, from the Linux examples, sends one request and
then keeps the loop in its own hands:

```rust,ignore
let mut executor = Executor::open(&config)?;
let mut node = executor.create_node("add_two_ints_client")?;
let mut client = node.create_client::<AddTwoInts>("/add_two_ints")?;

let mut promise = client.call(&AddTwoIntsRequest { a: 2, b: 3 })?;
loop {
    executor.spin_once(core::time::Duration::from_millis(0));
    if let Ok(Some(reply)) = promise.take() {
        log_info!(&LOGGER, "Result of add_two_ints: {}", reply.sum);
        break;
    }
}
```

Three things here differ from rclcpp. The node comes from the executor, not
the other way round. The call returns a `Promise`, not a
`std::shared_future`. And the loop that drives I/O is the application's,
not the library's. This page explains why the nano-ros client library, the
API above the RMW layer in Rust, C and C++, has this shape. The signatures
are in the [Rust](../reference/rust-api.md), [C](../reference/c-api.md) and
[C++](../reference/cpp-api.md) API references.

## Why a fourth client library

ROS 2 ships rclcpp for C++, rclc for C on microcontrollers, and rclrs for
Rust. Each assumes something an RTOS target may lack. rclcpp blocks a thread
inside `rclcpp::spin(node)` and waits on `std::shared_future`, which needs
`std::thread` and a condition variable. rclc runs on a microcontroller, but
it has no future type, so a request and its reply become an explicit state
machine of callbacks. rclrs targets hosted Rust with `std`.

nano-ros needs one library that runs on a Cortex-M3 with no threads and on a
Linux workstation, from the same node code. Its core is `no_std` Rust, and
the C and C++ APIs are thin layers over it, spelled like rclc and rclcpp so
ported code reads familiar.

## The executor owns the session

In rclcpp the executor is a separate object you add nodes to
(`exec.add_node(node)`), there are several executor classes, and each node
holds its own RMW context. In nano-ros the executor is the session's owner:

```rust,ignore
let mut executor = Executor::open(&config)?;           // opens the transport
let mut node = executor.create_node("my_node")?;      // borrows the session
let publisher = node.create_publisher::<Int32>("/topic")?;
```

There is one executor type, no separate context object, and no add or remove
node lifecycle. The handles a node creates are owned, so they outlive the
node; the usual pattern scopes the node to the creation calls. The entry
that `nros build` generates goes further: it opens the session once and runs
each sched task's executor over it, and an executor can also hold several
sessions at once, for a bridge between backends
([Bridge between backends](../guide/rmw/bridge-between-backends.md)).

## No hidden spin

`rclcpp::spin(node)` blocks the calling thread inside the library and owns
the dispatch loop. That cannot work on bare metal: there is no thread to give
up, and the application's loop has to stay in user code, to service
interrupts, poll the network stack and manage power.

nano-ros never runs a loop you did not ask for. The building block is
`spin_once(timeout)`, one pass of I/O and callbacks, and every other verb is
a loop over it that you could write yourself:

| Verb | Shape | Needs `alloc` |
| --- | --- | --- |
| `spin_once(timeout)` | one pass | no |
| `spin_some(max)` | rclcpp's drain: run what is ready, then return | no |
| `spin_forever(options)` | never returns; the body of an RTOS task | no |
| `spin(options)` | rclrs's name and shape: until cancelled, a timeout, or a callback count | yes |
| `spin_period(period)` | fixed rate, drift-compensated | yes |

So the verbs that reach a `no_std` no-alloc target are `spin_once`,
`spin_some` and `spin_forever`. In a generated entry you rarely call any of
them: each sched task's loop is generated, and your code is callbacks and
handles.

## One promise, two ways to wait

A service call or an action goal returns a `Promise<T>`. The same value
serves two patterns:

- **Poll it.** Keep running other work, spin, and call `promise.take()`,
  which returns `None` until the reply has arrived. The example at the top
  of this page does this.
- **Block on it.** `promise.wait(&mut executor, timeout)` spins the executor
  in a loop until the reply arrives or the timeout passes.

A `Promise` borrows its client's reply slot and a sequence number. It needs
no allocation, and it is not a `Future`: there is deliberately no third
pattern of `.await` on a foreign runtime. A `Future` completes only while
something else spins the executor, and then that runtime's scheduler, not
nano-ros, decides when the reply is handled (see
[What nano-ros does not run on](execution-model.md#what-nano-ros-does-not-run-on)).

## Why every blocking call takes the executor

`Promise::wait`, a subscription's `wait_next`, the C++ `Future::wait` and
C's `rclc_client_call` all take the executor, or a handle to it, although
the executor created the thing they wait on. There are two reasons.

The first is that no other thread drives I/O. If `wait` did not have the
executor, it could not spin, the network would stand still, and the wait
would always time out. Passing the executor makes that dependency explicit
at the call site, where it cannot be forgotten.

The second is borrowing. A promise borrows the session through its client,
and the executor owns the session, so the promise cannot also hold the
executor. Passing `&mut executor` at the call, while the promise is a
separate object, is how a wait gets a mutable executor while a request is in
flight.

## The same model in C and C++

C++ wraps the promise as `rclcpp::Future<T>`, and its wait takes the
executor's handle for the same reason the Rust API takes `&mut executor`:

```cpp
auto fut = client.send_request(req);
ResponseType resp;
RCLCPP_TRY(fut.wait(executor.handle(), 5000, resp));
```

The handle comes from `executor.handle()`, or `rclcpp::global_handle()` for
the free functions. There is no hidden global executor.

C has no generics, so it has no promise type, but the pattern is the same: a
blocking call that drives the executor for you, beside an asynchronous send
whose result arrives by callback while you keep spinning:

```c
/* Blocking: spins the attached executor until the reply or the timeout. */
size_t reply_len = 0;
rclc_client_call(&client, req_buf, req_len, reply_buf, sizeof(reply_buf), &reply_len);

/* Asynchronous: returns at once; the result comes to a callback. */
rclc_action_send_goal_async(client, &goal, sizeof(goal));
rclc_action_client_set_result_callback(client, on_result);
/* keep calling rclc_executor_spin_some() */
```

## Where nano-ros differs from ROS 2

- **Nodes come from the executor.** There is no `rclcpp::Node::make_shared`
  with a context of its own, and no `add_node`.
- **No `rclcpp::spin(node)` that owns a thread.** The loop is yours, or the
  generated entry's.
- **A `Promise`, not a `std::shared_future` or a Rust `Future`.** You poll
  it or block on it with the executor.
- **One executor type.** No `MultiThreadedExecutor` or
  `StaticSingleThreadedExecutor`; parallelism is a sched task
  ([The execution model](execution-model.md)).

For the rest of the C++ surface (what `rclcpp` names exist, and how they
behave), see [How nano-ros differs from ROS 2 and micro-ROS](differences-from-ros2.md)
and the [rclcpp documentation](https://docs.ros.org/en/humble/p/rclcpp/).

## Design background

- [RFC-0113](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0113-rt-unified-wait.md):
  one wait serves every platform and RMW; why there is no `Future` path.
