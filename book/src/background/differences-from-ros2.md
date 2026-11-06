# How nano-ros differs from ROS 2 and micro-ROS

<!-- type: explanation · level: 3 -->

A ported rclcpp node compiles against nano-ros with the same includes and the
same names. One line it needs that upstream does not:

```cpp
rclcpp::Node node("talker");
if (!node.ok()) return 1;        // replaces upstream's try/catch
```

Upstream's `rclcpp::Node` constructor throws when the node cannot be
created. nano-ros is built with `-fno-exceptions`, so the constructor records
the failure instead, and the node answers `false` to `ok()`. The vocabulary
is the same; the reason underneath is not.

That is the pattern of this page. nano-ros keeps ROS 2's vocabulary and wire
format so existing nodes port and talk to ROS 2 peers. Where it differs, the
reason is almost always the same: it runs on a microcontroller with little
RAM to spare, no dynamic loader and often no threads. This page states each
difference plainly, then compares nano-ros with micro-ROS, its closest peer.
The tasks are [Port a C++ node](../guide/port/port-a-cpp-node.md) and
[Move a ROS 2 project over](../guide/port/move-a-ros2-project.md).

## What stays the same

- **The entity model.** A node owns publishers, subscriptions, services,
  clients, action servers, action clients, timers and parameters, with ROS 2's
  names (see the [ROS 2 concepts](https://docs.ros.org/en/humble/Concepts/Basic.html)).
- **Names and types.** Topic, service and action names follow ROS 2's
  conventions, message types are rosidl-shaped (`std_msgs/msg/Int32`), and
  CDR is the encoding on the wire.
- **Packages.** A workspace is a `src/` of packages with `package.xml`
  manifests.
- **The wire.** The zenoh backend is compatible with upstream `rmw_zenoh`: a
  nano-ros publisher and an rclcpp subscriber on the same router exchange
  messages with no bridge ([Talk to ROS 2](../tutorial/talk-to-ros2.md)).
- **The C and C++ spellings.** The C API uses rclc's names, argument order and
  arity; the C++ API uses rclcpp's.

## What changes in the workflow

- **One release, no package registry.** The `nros` CLI is a GitHub release,
  and the library ships as source inside it, compiled into your build.
  Nothing comes from crates.io or a ROS 2 apt repository.
- **Per-board provisioning, no `rosdep`.** `nros setup <board>` fetches the
  toolchains and SDKs for exactly that board and RMW into the user's store,
  with no system-wide install. The zenoh router is not provisioned: it comes
  from a ROS 2 install.
- **`nros build`, not `colcon build`.** It reads the same `package.xml` files
  and drives cargo, CMake or west, with no install space
  ([How a build becomes an image](how-a-build-becomes-an-image.md)).
- **Message code lands in your build.** There is no installed `_msgs`
  library. The message *definitions* still come from your ROS 2 install,
  through `AMENT_PREFIX_PATH`, so the layouts and type hashes match your ROS 2
  peers ([Message generation](../reference/message-generation.md)).
- **No `ros2 launch` on the device.** The launch file is read at build time,
  and the generated entry is the launched system.
- **Configuration is build-time on embedded.** On a host, `ROS_DOMAIN_ID` and
  the other environment variables work. An embedded image bakes the same
  values from `system.toml`.

## What changes in the API

### The executor is the entry point

ROS 2 starts with a global init (`rclcpp::init`, `rclc_support_init`), then
nodes, then optionally an executor. nano-ros inverts it: an `Executor` opens
the session, and every node, entity and timer is created from it, in storage
sized at build time. On a target with tens of kilobytes of RAM, a global
allocator behind every `create_publisher` is a cost the image cannot carry,
so the sizing moves to the build. [The client library](client-library.md)
explains the model.

### Polling and callbacks are both first-class

rclcpp is callback-only: every subscription has a callback the executor
dispatches. nano-ros also has polled entities, under a different name
(`create_polling_subscription`), so you choose per entity. A real-time loop
often wants the predictability of an explicit poll; an event-driven service
wants a callback ([The execution model](execution-model.md#two-ways-to-receive)).

### No async runtime and no multi-threaded executor

rclrs has an async spin and rclcpp has a `MultiThreadedExecutor`. nano-ros has
neither, on purpose. A future polled by tokio or Embassy runs when that
scheduler decides, so nano-ros could not state when a callback runs or at what
priority. Parallelism is a sched task instead: an executor on an RTOS thread
at a priority the build derives.

### The heap is optional

ROS 2 assumes `std` and a heap. The nano-ros core is `no_std` with
fixed-size pools, and `alloc` is opt-in
([Static memory and pools](static-memory-and-pools.md)). The C transport
libraries still allocate through the platform's allocator.

### The backend is chosen when you build

ROS 2 reads `RMW_IMPLEMENTATION` at process start and loads a shared library.
nano-ros links the backend when it builds the image, from the bringup's
`rmw`. Most embedded targets have no `dlopen`; linking only the chosen
backend keeps its C client out of an image that does not use it; and the
build knows what to link. The cost is that changing backends means
rebuilding. One binary can still link several backends and bridge between
them ([Bridge between backends](../guide/rmw/bridge-between-backends.md)).

### How each backend reaches ROS 2

The three backends differ less in what they can carry than in the shape of
the network around the device: what else has to run, who does discovery, and
what happens when a process goes away.

- **zenoh.** The device runs zenoh-pico, a small C client, in client mode,
  and connects to a zenoh router over serial, TCP or UDP. The router is ROS
  2's own `rmw_zenohd`; ROS 2 nodes running `rmw_zenoh_cpp` connect to the
  same router, which forwards messages without translating them. The device
  joins ROS 2 graph discovery through liveliness tokens, so `ros2 topic list`
  sees it. On a serial-only link the build leaves out the device's own graph
  cache, its view of the other nodes, to save RAM. Zenoh also defines a peer mode with no router, which
  nano-ros does not ship: its multicast transport and scouting cost flash,
  and every nano-ros deployment reaches its peers through a router.
  `NROS_SESSION_MODE=peer` is refused at session open. If the router stops,
  the device keeps running but reaches no one.
- **XRCE-DDS.** The device runs the Micro XRCE-DDS client, the same one
  micro-ROS uses, and asks an agent, `MicroXRCEAgent`, to create DDS
  entities on its behalf. The agent does discovery and bridges data to the
  DDS data space, so ROS 2 nodes on any DDS RMW reach the device through it.
  The client allocates when a session or entity is created, never per
  message. If the agent stops, the device loses all connectivity.
- **Cyclone DDS.** The image links Eclipse Cyclone DDS, the implementation
  ROS 2 itself uses through `rmw_cyclonedds_cpp`. The device is a full DDS
  participant: SPDP and SEDP discovery over UDP multicast (or a static
  unicast peer list), the same wire protocol and no key rewriting. Nothing
  runs beside it, and when a peer goes away only that peer's samples stop.
  Cyclone allocates with `malloc` and runs its own receive threads, so it
  needs a heap and an operating system with a network stack.

All three are linked through one RMW C ABI below the client library, which
is why a node package does not change when its image's backend does.

### No introspection on the device

`ros2 topic list`, `ros2 node info` and the `rmw_get_*` graph queries run on
a host. A nano-ros image answers what its backend's discovery exposes, and you
introspect it from a ROS 2 host over the interop path. Every byte of
introspection is a byte a microcontroller pays for, when a ROS 2 host is one
router hop away.

### QoS: the full profile, with support stated per backend

The QoS profile has the full DDS field set, and the standard profiles
(`_DEFAULT`, `_SENSOR_DATA`, `_SERVICES_DEFAULT`, `_PARAMETERS`,
`_SYSTEM_DEFAULT`) match upstream field for field. What differs is
enforcement. Each backend advertises which policies it can enforce
(`Session::supported_qos_policies()`), and creating an entity with a policy
the backend cannot honour fails at once with `IncompatibleQos`. A requested
policy is never dropped silently. Upstream reports a mismatch later, as a
run-time event; most mismatches are configuration errors visible at start-up.

Manual liveliness is the sharpest case. `assert_liveliness()` exists in every
language, but only Cyclone DDS puts it on the wire. zenoh-pico's liveliness is
a session keep-alive with no per-topic lease, so under a manual kind zenoh,
XRCE-DDS and uORB answer `UNSUPPORTED` rather than an `Ok` that would mean
nothing was sent. The run-time side is
[Use QoS, status events and discovery](../guide/rmw/qos-status-events.md),
and per-backend coverage is the [Per-RMW feature matrix](../reference/matrices/rmw-feature-matrix.md).

### Parameters: one store, a smaller surface

nano-ros keeps the vocabulary (`declare_parameter<T>`, `get_parameter<T>`,
`set_parameter<T>`, `has_parameter`) and the remote services, so
`ros2 param list` and `ros2 param set` reach a node with parameter services
on. Underneath there is one store, owned by the executor and keyed by node,
with a slot count fixed at build time; it is also the store the services read,
so what you declare is what `ros2 param get` returns. A node does not own its
parameters, because a store per node is a fixed arena per node on a target
that has one.

What is smaller than rclcpp, and why:

| rclcpp | nano-ros | Why |
| --- | --- | --- |
| `ParameterDescriptor` (ranges, read-only, description) | not exposed | metadata is a host concern; the type is enforced at declare time |
| three set callbacks (`pre_set`, `on_set`, `post_set`) | one validation callback per C server | each callback is an indirection slot per node; one covers range checks |
| `set_parameter` returns `SetParametersResult` with a reason | a return code | a reason string needs a heap or a fixed buffer |
| `set_parameters_atomically` | not exposed | needs a transaction log |
| `declare_parameters` with a `std::map` | C++ on a hosted build only, returning `Result` | the map needs an allocator |
| overrides from the command line, launch file or YAML | the launch file at build time; `~/set_parameters` over the wire; `-p` and `--params-file` on a hosted build | an embedded image has no command line |

An array value is `rclcpp::Seq<T, N>`, an array parameter, with no heap: the
store copies the elements into its own slot. When you need the full upstream
surface, run it on a host ROS 2 node and keep the embedded node a leaf. The
calls and `/parameter_events` are in the
[rclcpp compatibility](../reference/rclcpp-compatibility.md#parameters-and-parameter_events) reference.

### Where C++ code behaves differently

Most of the rclcpp surface either works or fails to compile. Three things
compile and behave differently, each for a reason:

- **`Node::ok()`.** With no exceptions, a failed constructor cannot throw, so
  you ask. A generated entry checks it and halts naming the node; a
  hand-written `main` must check, or a failed node goes on to create
  publishers that do nothing.
- **`Node::SharedPtr` observes; it does not own.** It is a copyable handle,
  not a `std::shared_ptr`, on every target, embedded included. A node is
  constructed by the entry and outlives everything it is handed to, so no
  ownership needs counting. `shared_from_this()` returns the handle and
  never throws `bad_weak_ptr`. nano-ros cannot derive from
  `std::enable_shared_from_this`, because its `std::weak_ptr` member exists
  only where `<memory>` does, and two translation units of one image could
  then disagree about the node's layout. Assigning a temporary
  `make_shared` result to a `Node::SharedPtr` is a compile error, since the
  handle would dangle; write `auto`.
- **`rclcpp::TimerBase` is a name, not a hierarchy.** It is an alias of the
  one flat `rclcpp::Timer`, with no vtable: the executor dispatches through a
  plain function pointer, so a polymorphic base would cost bytes with no
  caller. The clock is a run-time field and a second verb,
  `rclcpp::create_timer`, not a type parameter, so `WallTimer` and
  `GenericTimer` do not exist. A `SharedPtr` to a timer, subscription,
  service or client is a small handle too: dropping it leaves the entity
  running, and `reset()` on the handle releases it.

## Compared with micro-ROS

micro-ROS is the closest peer: a ROS 2 client stack for microcontrollers. The
two differ mostly in what they assume about the network and the language.

| | nano-ros | micro-ROS |
| --- | --- | --- |
| Application APIs | Rust, C (rclc's names), C++ (rclcpp's names) | C (rclc) |
| RMW | zenoh, XRCE-DDS, Cyclone DDS, chosen per image | XRCE-DDS |
| Network model | through a zenoh router, brokerless with Cyclone DDS, or agent-based with XRCE-DDS | agent-based |
| Process beside the device | a zenoh router, none, or an XRCE-DDS agent, by backend | always the Micro XRCE-DDS Agent |
| Core | `no_std`, fixed-size pools | C with a static memory configuration |
| Scheduling | sched tasks and derived levels, EDF and sporadic classes, budgets ([The execution model](execution-model.md)) | the rclc executor |
| Several backends in one binary | yes, with bridges | no |
| End-to-end integrity | CRC and sequence check ([Safety](safety.md)) | none built in |
| Formal checks | Kani harnesses and Verus proofs over serialization, scheduling and protocol logic | none published |
| Build | `nros build` driving cargo, CMake or west; or nano-ros inside your CMake, Zephyr or NuttX build | colcon and per-RTOS firmware scripts |
| License | MIT or Apache-2.0 | Apache-2.0 |

The boards nano-ros supports, and at what level, are generated in
[Supported boards](../reference/supported-boards.md).

Choose nano-ros when you want Rust as a first-class API, a zenoh router or
brokerless DDS instead of an agent, several backends in one image, or scheduling beyond
priorities. Choose micro-ROS when you already have rclc code and an agent
deployment, when your toolchain must stay C-only, or when you need its
ecosystem's commercial support.

Moving from micro-ROS is mostly a rename, and in C often not even that: the
C API keeps rclc's spellings, so `rclc_publisher_init_default`,
`rclc_node_init_default` (in rclc's argument order) and `rclc_executor_t` are
nano-ros's names too. An existing XRCE agent serves nano-ros's XRCE-DDS
backend unchanged. A custom transport is a platform-level transport rather
than `rmw_uros_set_custom_transport`
([Add a transport](../extending/add-a-transport.md)).

## Design background

- [RFC-0113](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0113-rt-unified-wait.md):
  why there is no async executor.
- [RFC-0089](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md):
  adopting the ROS 2 API, and the compile-or-conform rule: a ported line
  either behaves as upstream's or does not compile.
