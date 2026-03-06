# Introduction

nano-ros is a lightweight ROS 2 client library for embedded real-time systems.
It runs on bare-metal microcontrollers, FreeRTOS, NuttX, ThreadX, and Zephyr,
as well as Linux and macOS. The entire core stack is `no_std` compatible.

## Key Features

- **Minimal stack** — three software layers (application, nano-ros,
  transport). Lean dependency tree, fast compile times.
- **Dual middleware** — choose Zenoh (agent-less, direct peer
  communication) or XRCE-DDS (agent-based) at compile time. Same
  application code either way.
- **Rust-first with C API** — the core is written in Rust for memory safety
  and ergonomics, with a thin C FFI layer following rclc conventions.
- **True `no_std`** — runs on bare-metal Cortex-M3 with no heap allocator.
  The `alloc` and `std` features are opt-in.
- **Standalone tooling** — `cargo nano-ros generate` produces message
  bindings without a ROS 2 installation (bundled interface definitions).
- **Formally verified** — 160 Kani bounded model checking harnesses and 102
  Verus deductive proofs cover CDR serialization, scheduling, and protocol
  correctness.
- **ROS 2 compatible** — interoperates with standard ROS 2 nodes via
  `rmw_zenoh_cpp`. Topics, services, and actions work across the boundary.

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
