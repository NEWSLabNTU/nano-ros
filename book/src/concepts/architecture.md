# Architecture Overview

nano-ros is a ROS 2 client stack for embedded and RTOS targets. The
important architectural idea is separation by responsibility: user code
talks to a ROS-shaped API, the core runtime owns entities and
serialization, the RMW layer moves bytes, and the platform layer supplies
OS or hardware primitives.

## Layers

```text
Application
  Rust / C / C++ node code

Board package (optional)
  Hardware init, network drivers, config loading, entry point

Core runtime
  Executor, Node, pub/sub/service/action handles, parameters,
  message traits, CDR serialization

RMW backend
  Zenoh, XRCE-DDS, DDS, Cyclone DDS, or a custom backend

Platform
  Clock, allocation, threading, sleep, random, sockets, libc,
  and bare-metal network polling
```

POSIX applications usually depend directly on `nros`. Embedded
applications often depend on a board package that initializes hardware,
networking, and platform glue before running user code.

## Core Runtime

The core runtime is middleware-agnostic. `nros-node` owns the
`Executor`, node creation, entity handles, timers, and the two API
styles:

- `Node::create_*` returns handles that the caller polls or awaits.
- `Executor::register_*` installs callbacks dispatched by `spin_once`.

Message and service types are ordinary generated Rust, C, or C++ types
with CDR serialization. The RMW backend receives serialized bytes; it
does not own rosidl typesupport.

For the API split, see [Execution Model and Two-Layer API](two-layer-api.md).

## RMW Layer

The RMW layer is the transport boundary. It creates sessions,
publishers, subscribers, services, clients, and action channels, then
moves serialized samples over the selected wire protocol.

Only one backend is selected for a build. That compile-time selection
replaces standard ROS 2's runtime `RMW_IMPLEMENTATION` plugin loader,
which is not available on many embedded targets.

For user-facing backend selection, see
[Choosing an RMW Backend](../user-guide/rmw-backends.md). For design
rationale, see [RMW API Design](../design/rmw.md).

## Platform Layer

The platform layer supplies the primitives that desktop ROS 2 normally
gets from the operating system: monotonic time, memory allocation,
threading, sleep, random IDs, TCP/UDP sockets, multicast, and libc
helpers. Bare-metal ports may also expose a `network_poll()` hook so the
runtime can advance smoltcp while waiting.

The platform is selected at compile time together with the RMW backend
and ROS edition. See [Platform Model](platform-model.md) for the feature
axes and [Custom Platform](../porting/custom-platform.md) for porting.

## Board Packages

A board package combines a platform implementation with hardware setup
and drivers. It typically provides:

- a `Config` type loaded from `config.toml` or target-specific build
  settings,
- network and clock initialization,
- driver setup for Ethernet, UART, WiFi, or simulator I/O,
- a `run()` entry point that starts the scheduler or main loop.

Board packages are optional for POSIX but useful for RTOS and
bare-metal targets. See [Custom Board Package](../porting/custom-board.md).

## Data Flow

Publishing follows this path:

```text
user message -> CDR serializer -> RMW publish bytes -> transport
```

Receiving reverses it:

```text
transport -> RMW receive bytes -> CDR deserializer -> user handle/callback
```

This boundary keeps the transport layer small and lets the same message
types work across Rust, C, and C++ APIs.

## Where to Go Next

- New user: [Setup Compared to Standard ROS 2](../start-here/setup-compared-to-ros2.md).
- Application author: [Configuration](../user-guide/configuration.md).
- Platform porter: [Porting Overview](../porting/overview.md).
- Contributor changing internals: [Design Overview](../design/overview.md).
