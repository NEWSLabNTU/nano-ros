# Differences from standard ROS 2

Coming from `rclcpp`, `rclrs`, or `rclc`? This page calls out where
nano-ros looks the same, where it diverges, and the reason behind each
choice. It is an *orientation* page — the per-language API references
([Rust](../reference/rust-api.md) / [C](../reference/c-api.md) /
[C++](../reference/cpp-api.md)) cover the surface itself.

The short version: nano-ros keeps ROS 2's vocabulary (Node, Publisher,
Subscription, Service, Client, ActionServer, ActionClient, Timer,
Executor, QoS, message codegen) so existing nodes port cleanly. The
trade-offs that come from running on a Cortex-M3 with 64 KB of heap
shape every place where the surface diverges.

## Same vocabulary, same wire format

- ROS 2 entity model unchanged. A node owns publishers, subscriptions,
  services, clients, action servers, action clients, timers, and
  parameters.
- Topic / service / action names follow ROS 2 conventions
  (`/talker_node/chatter`, `/add_two_ints`, `/fibonacci`).
- Message types stay rosidl-shaped (`std_msgs/msg/Int32`,
  `geometry_msgs/msg/Twist`, …). CDR encoding on the wire.
- Default backend (`rmw-zenoh`) is bit-compatible with the upstream
  [`rmw_zenoh`](https://github.com/ros2/rmw_zenoh) ROS 2 RMW. A
  nano-ros publisher and an `rclcpp` subscriber on the same zenohd
  router exchange messages without a bridge — see
  [ROS 2 Interoperability](../getting-started/ros2-interop.md).

## Where it diverges, and why

### 1. The executor is the entry point

Standard ROS 2 starts with a global init (`rclcpp::init` /
`rclrs::Context::default_from_env` / `rclc_support_init`) followed by
node creation, then optionally an executor.

nano-ros inverts this: an `Executor` opens the RMW session and owns the
runtime budget. Every node, publisher, subscription, service, client,
action handle, and timer is allocated *out of the executor's arena*.

```text
Executor::open(&config) → Node → Publisher / Subscription / Service / Client / Timer
```

**Why.** The arena is fixed-size and known at compile time
(`NROS_EXECUTOR_ARENA_SIZE` / `NROS_EXECUTOR_MAX_CBS`). On a 64 KB
heap MCU we cannot afford the indirection of a global allocator behind
every `create_publisher` call. The executor-as-arena pattern moves the
size negotiation up to the application's startup code, where it
belongs.

### 2. Both manual-poll and callback paths are first-class

`rclcpp` is callback-only — every subscription needs a callback, the
executor dispatches them. `rclrs` <0.7 was manual-poll only. `rclc`
exposes both but treats manual-poll as the second-class path.

nano-ros treats them as equals. A subscription created via
`node.create_subscription(...)` exposes `try_recv()`. A subscription
registered via `executor.add_subscription(callback)` runs the callback
during `spin_once`. Pick whichever fits the control loop.

**Why.** Embedded apps often want the predictability of an explicit
poll inside a real-time loop. Callback dispatch is great for
event-driven services but adds indirection that real-time engineers
have to bound by hand. Offering both means the application picks.

### 3. Async pub/sub/service/action

`rclcpp` has no real `async/await`. `rclrs` 0.7+ added async but it
sits next to the synchronous executor, not in it. `rclc` has none.

nano-ros has it as a first-class path: `Executor::spin_async()` wakes
on RMW I/O, `Subscription::recv().await`, `Client::call().await`,
`ActionClient::send_goal().await`, etc. Runs on tokio (POSIX),
Embassy (FreeRTOS / RTIC), or any external `Future`-driver.

**Why.** Robotic control loops are stitched together from many
concurrent waits — sensor data, service replies, action feedback,
parameter updates. Async lets you write them as one straight-line
function instead of hand-rolling state machines on top of a
poll-based executor.

### 4. Heap is optional

ROS 2 RMW implementations all assume `std` + a heap. Even rclrs's
`no_std` story is "soon".

nano-ros runs in three modes that map onto target capability:

| Mode | Cargo features | What works |
|------|----------------|------------|
| `std` | `std` (default on POSIX) | Everything. POSIX threading, full async runtime. |
| `no_std + alloc` | `alloc` + a `#[global_allocator]` | Everything except features that need `std::sync::Mutex`. Used by FreeRTOS / NuttX / ThreadX / Zephyr / ESP32. |
| `no_std + nostd-runtime` (cooperative) | `nostd-runtime` on dust-DDS, RTIC apps | Cooperative single-task — no threading at all. Used by bare-metal MPS2-AN385, single-core RTIC. |

**Why.** Heap presence is not a binary "embedded yes/no" — it is a
spectrum. Stm32-class boards have a heap; Cortex-M0+-class might not.
The feature axis lets the same application code target both.

### 5. Backend selection at compile time, not runtime

Standard ROS 2 uses an `RMW_IMPLEMENTATION` env var read at process
start. The plugin loader pulls a shared library, dispatches calls
through C function pointers.

nano-ros bakes the backend in at compile time. Cargo features
(`rmw-zenoh` / `rmw-xrce` / `rmw-dds` / `rmw-uorb`) or CMake options
(`-DNROS_RMW=zenoh`) decide it.

**Why.**

- **Dead-code elimination.** A 32 KB Flash budget cannot afford to
  link every backend's C client and pick at runtime. Linking only the
  selected backend cuts the binary by 60–80 %.
- **No plugin loader.** Most embedded targets have no `dlopen`. The
  cost of the plugin abstraction is a permanent overhead with no
  payoff there.
- **Cross-compile sanity.** `RMW_IMPLEMENTATION` baked into the binary
  means the build system already knows which backend's C client to
  link — no separate "find shared library at runtime" step.

The trade-off is real: changing backends requires a rebuild. This is
the right trade-off for the embedded use case; it would be the wrong
trade-off for desktop ROS 2.

### 6. Message codegen lands inside your build, not a sibling library

Standard ROS 2 uses `ament` + `rosidl` to compile message packages
(`std_msgs`, `geometry_msgs`, …) into separate shared libraries that
your application links against.

nano-ros's `cargo nano-ros generate-rust` (Rust) and
`nano_ros_generate_interfaces()` (C / C++ via CMake) write message
type definitions *into your build tree*. No `_msgs` library, no ament
overlay, no colcon workspace required.

**Why.** Embedded cross-builds without a hosted ROS 2 install need to
generate message types from `package.xml` + `.msg` files alone. The
codegen tool ships its own bundled rosidl-flavoured files
(`packages/codegen/interfaces/`) so you don't even need the upstream
message packages on disk.

### 7. QoS is a minimal subset, not full DDS profiles

Standard ROS 2 supports the full DDS QoS profile family
(`reliability`, `durability`, `history`, `depth`, `deadline`,
`lifespan`, `liveliness`, `lifespan_*`, partition, ownership, …) and
performs profile *matching* between endpoints.

nano-ros only enforces the QoS subset its backends actually implement:

| Backend | Reliability | Durability | History | Depth |
|---------|-------------|-----------|---------|-------|
| zenoh-pico | reliable / best-effort | volatile / transient-local | keep-last | configurable |
| XRCE-DDS | reliable / best-effort | volatile / transient-local | keep-last | configurable |
| dust-DDS | reliable / best-effort | volatile / transient-local | keep-last / keep-all | configurable |
| uORB | reliable (in-process) | n/a | last-N (uORB queue size) | configurable |

No profile matching, no deadline / lifespan / liveliness watchdogs.

**Why.** The QoS family is a DDS-shaped abstraction; not every
backend can honour all of it (zenoh-pico has no concept of
"liveliness lease" the way DDS does, for example). Promising QoS
features that a backend can't enforce is worse than not promising
them.

### 8. No runtime backend swap, no runtime introspection

Standard ROS 2 ships `ros2 topic list`, `ros2 node info`, dynamic
endpoints discovery, `rmw_get_*` introspection.

nano-ros has none of that at runtime. The backend is fixed at compile
time, the wire-protocol introspection is whatever the backend natively
exposes (zenoh-pico's `z_query` for SPDP, dust-DDS's discovery DB,
…). Use the host-side ROS 2 tools for introspection and connect via
the rmw_zenoh interop path.

**Why.** Every byte of "introspect what's running" is overhead a
microcontroller can't justify when a host-side ROS 2 environment is
one router-hop away.

## What this means in practice

If you are coming from `rclcpp`:

- Open an [`Executor`](../api/cpp/classnros_1_1Executor.html), then
  create the node from it.
- Decide poll vs. callback per subscription, not globally.
- If the platform has `std`, `nros::init()` looks identical; if it is
  RTOS / bare-metal, plan the executor arena up front.
- Pick `rmw-zenoh` for ROS 2 interop; everything else is a different
  trade-off.

If you are coming from `rclrs`:

- The umbrella crate is [`nros`](../reference/rust-api.md), not split
  into `rclrs_*`. `nros::prelude` gives you everything.
- `Executor::open(&config)` is the equivalent of
  `Context::default_from_env()` + `Executor::new(...)`.
- The async surface is in `nros::dds_async` (re-exported at the crate
  root). Compatible with tokio out of the box.

If you are coming from `rclc`:

- Same C names where they map (`nros_node_init`, `nros_publisher_init`,
  `nros_subscription_init`). Memory ownership rules are the same — the
  caller owns storage, the API initialises it.
- See the [C API reference](../reference/c-api.md) for the full
  surface.

## Going deeper

- API surface, type by type → per-language references at
  [Rust](../reference/rust-api.md) / [C](../reference/c-api.md) /
  [C++](../reference/cpp-api.md).
- Why the executor / RMW / platform layers split this way →
  [Architecture Overview](architecture.md).
- Cooperative `no_std + nostd-runtime` model → [no_std Support](no-std.md).
