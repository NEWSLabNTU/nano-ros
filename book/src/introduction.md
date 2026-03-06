# Introduction

nano-ros is a lightweight ROS 2 client library for embedded real-time systems.
It runs on bare-metal microcontrollers, FreeRTOS, NuttX, ThreadX, and Zephyr,
as well as Linux and macOS. The entire core stack is `no_std` compatible.

## Why nano-ros?

ROS 2 is the standard middleware for robotics, but its standard client
libraries (rclcpp, rclpy) require a full operating system and significant
memory. [micro-ROS](https://micro.ros.org/) addresses microcontrollers but
depends on a 6+ layer C software stack (rclc, rcl, rmw, XRCE-DDS, Micro-CDR,
transport) and an external agent process.

nano-ros takes a different approach:

- **3 layers instead of 6+** — application code talks to nano-ros, which
  talks to the transport. No intermediate rcl/rmw C shim layers.
- **Agent-less option** — the Zenoh backend communicates directly via
  zenoh-pico, with only a lightweight router (zenohd) instead of a
  protocol-translating agent.
- **Rust-first with C API** — the core is written in Rust for memory safety
  and ergonomics, with a thin C FFI layer following rclc conventions.
- **True `no_std`** — runs on bare-metal Cortex-M3 with no heap allocator.
  The `alloc` and `std` features are opt-in.
- **Formally verified** — 160 Kani bounded model checking harnesses and 102
  Verus deductive proofs cover CDR serialization, scheduling, and protocol
  correctness.

## Supported Platforms

| Platform   | RTOS          | Network Stack  | Targets                      |
|------------|---------------|----------------|------------------------------|
| POSIX      | Linux / macOS | OS sockets     | x86-64, aarch64              |
| Bare-metal | None          | smoltcp        | Cortex-M3, ESP32-C3, STM32F4 |
| FreeRTOS   | FreeRTOS      | lwIP           | Cortex-M3 (QEMU)             |
| NuttX      | NuttX         | BSD sockets    | Cortex-A7 (QEMU)             |
| ThreadX    | ThreadX       | NetX Duo       | RISC-V 64 (QEMU), Linux sim  |
| Zephyr     | Zephyr        | Zephyr sockets | Various boards               |

## RMW Backends

nano-ros supports two middleware backends, selectable at compile time:

- **Zenoh** (`rmw-zenoh`) — peer-to-peer via zenoh-pico. No agent process.
  Compatible with ROS 2 `rmw_zenoh_cpp`.
- **XRCE-DDS** (`rmw-xrce`) — agent-based via Micro-XRCE-DDS. Compatible
  with micro-ROS agent.

Application code is identical regardless of backend — switch with a single
Cargo feature flag or Zephyr Kconfig option.

## Comparison with micro-ROS

| Aspect              | micro-ROS          | nano-ros                               |
|---------------------|--------------------|----------------------------------------|
| Language            | C (rclc)           | Rust + C                               |
| Software layers     | 6+                 | 3                                      |
| Middleware          | XRCE-DDS only      | Zenoh + XRCE-DDS                       |
| Agent required      | Yes (always)       | No (Zenoh) / Yes (XRCE)                |
| `no_std` bare-metal | Via RTOS           | Native                                 |
| Message codegen     | Requires ROS 2 env | Standalone (`cargo nano-ros generate`) |
| Formal verification | None               | Kani + Verus + Miri                    |
| Memory model        | Static pools       | Const generics + arena allocator       |

## Project Status

nano-ros is under active development. The Rust API, C API, Zenoh backend,
XRCE-DDS backend, and all listed platforms are functional. See the platform
chapters for current status of each target.

## How This Book Is Organized

- **Getting Started** — install toolchains, build your first app, connect
  to ROS 2.
- **Concepts** — understand the architecture, feature system, and backend
  model.
- **Guides** — step-by-step walkthroughs for message generation, QEMU
  testing, and ESP32 development.
- **Platforms** — per-RTOS setup and configuration.
- **Reference** — API details, environment variables, build commands, and
  wire protocol.
- **Advanced** — formal verification, real-time analysis, safety features,
  and contributing.
