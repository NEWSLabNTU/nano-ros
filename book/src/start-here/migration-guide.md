# Migration Guide for ROS 2 Users

This scaffold maps standard ROS 2 concepts to nano-ros pages. It is not
an API reference; use it as a checklist when moving an existing
`rclcpp`, `rclc`, or `rclrs` node toward nano-ros.

## Setup

Standard ROS 2 usually starts from a distro install and a runtime RMW
choice. nano-ros starts from a source checkout + a compile-time target
tuple:

```bash
git clone --branch=v<X.Y.Z> https://github.com/NEWSLabNTU/nano-ros.git
cd nano-ros
just setup tier=default
```

Read [Setup Compared to Standard ROS 2](setup-compared-to-ros2.md)
before changing package code.

## Node Lifecycle

ROS 2 applications typically call `rclcpp::init()`, create nodes, then
spin. nano-ros opens an executor first; the executor owns the session,
arena, and runtime budget. Nodes and entities are created from it.

See [Differences from Standard ROS 2](../concepts/ros2-comparison.md)
and [Execution Model and Two-Layer API](../concepts/two-layer-api.md).

## Publishers and Subscriptions

Topic names, message names, and CDR wire encoding stay ROS-shaped.
The main porting decision is whether to use:

- polling handles from `Node::create_*`, or
- callback registration through `Executor::register_*`.

Use polling for RTIC, Embassy, or tight RT loops. Use callbacks for
desktop-style event-driven nodes.

## Services and Actions

Service and action names map cleanly, but nano-ros exposes request /
reply and goal / feedback / result paths through explicit handles and
promises. Manual-poll paths may require explicit result handling in
RTOS loops.

Start with the native examples, then check platform-specific examples
for FreeRTOS, Zephyr, or bare-metal timing constraints.

## QoS and Events

nano-ros keeps DDS-shaped QoS profile fields, but each backend advertises
the policies it can enforce. Unsupported QoS is reported at entity
creation instead of being silently downgraded.

See [QoS, Status Events, and Discovery](../concepts/status-events.md)
and [Choosing an RMW Backend](../user-guide/rmw-backends.md).

## Message Generation

Standard ROS 2 builds generated message libraries as sibling packages.
nano-ros generates Rust, C, or C++ bindings into the workspace or build
tree and can use a shared generation cache.

See [Message Binding Generation](../user-guide/message-generation.md).

## Backend Selection

Replace runtime `RMW_IMPLEMENTATION=...` with a compile-time selection:

- `posix-zenoh` for early ROS 2 interop through `rmw_zenoh_cpp`.
- `*-xrce` for agent-based micro-ROS-style deployments.
- `*-dds` or `*-cyclonedds` for direct DDS/RTPS deployments where the
  platform supports the required networking and memory model.

## Common Porting Traps

- Assuming the backend can change without rebuilding.
- Creating heap-heavy callbacks on `no_std` targets without `alloc`.
- Expecting ROS 2 graph introspection APIs on constrained targets.
- Forgetting to match ROS domain ID, QoS reliability, or zenoh router
  mode during interop tests.
- Treating a platform guide as optional when cross-compiling for RTOS
  or bare-metal targets.
