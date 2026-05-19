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

## Quick board check — does it work on the board I have today?

| Vendor / form factor      | Chip          | RTOS / no-RTOS  | Languages | Example in repo                                   | ROS 2 interop |
|---------------------------|---------------|-----------------|-----------|---------------------------------------------------|---------------|
| ARM MPS2-AN385 (QEMU)     | Cortex-M3     | FreeRTOS / bare | Rust C C++ ¹ | `examples/qemu-arm-{freertos,baremetal}/`         | Verified      |
| ST STM32F4-Discovery      | Cortex-M4F    | FreeRTOS / bare | Rust ²    | board crate `nros-board-stm32f4-nucleo`           | Verified      |
| Espressif ESP32-C3        | RISC-V (RV32) | bare / ESP-IDF  | Rust C C++ | `examples/esp32/rust/`, `integrations/esp-idf/`   | Verified      |
| Espressif ESP32-S3        | Xtensa        | bare / ESP-IDF  | Rust ³    | board crate `nros-board-esp32` (Xtensa toolchain) | Ready         |
| Espressif ESP32-C3 (QEMU) | RISC-V        | bare            | Rust      | `examples/esp32-qemu/`                            | Verified      |
| QEMU `virt` RISC-V64      | RV64GC        | ThreadX         | Rust C    | `examples/threadx-riscv64/`                       | Verified      |
| Linux host                | x86-64 / aarch64 | ThreadX sim  | Rust C    | `examples/threadx-linux/`                         | Verified      |
| QEMU Cortex-A9 / virt     | Cortex-A9     | NuttX / Zephyr  | Rust C C++ | `examples/nuttx/`, Zephyr `samples/`              | Verified      |
| Pixhawk 4 / 6X            | STM32F7 / H7  | NuttX (PX4)     | C++       | `integrations/px4/module-template/`               | Ready ⁴       |
| Generic Cortex-M0+/M4/M7  | ≥ 64 KB SRAM  | RTOS of choice  | Rust C C++ | Use your board's vendor BSP + integrations shells | Pattern shown |

**Legend:** *Verified* = booted + tested in CI. *Ready* = builds and
runs but no in-CI gate yet — drop into the matching `examples/<plat>/`
to compile and try.

Footnotes — ¹ MPS2-AN385 bare-metal is Rust-only (`nros-c` / `nros-cpp`
need an RTOS for libc / heap). ² STM32F4 Rust path is the canonical
target for the bare-metal board crate; FreeRTOS variant uses the
shared `nros-board-freertos` glue. ³ ESP32-S3 needs the `xtensa-esp32s3-none-elf`
Rust target (`rustup target add` via esp-rs). ⁴ PX4 path is via the
external-module template in `integrations/px4/` — C++ only because
PX4's uORB binding is C++-only.

## Supported platforms (by RTOS)

| Platform   | RTOS          | Network Stack  | Targets                      |
|------------|---------------|----------------|------------------------------|
| POSIX      | Linux / macOS | OS sockets     | x86-64, aarch64              |
| Bare-metal | None          | smoltcp        | Cortex-M3, ESP32-C3, STM32F4 |
| FreeRTOS   | FreeRTOS      | lwIP           | Cortex-M3 (QEMU)             |
| NuttX      | NuttX         | BSD sockets    | Cortex-A7 (QEMU)             |
| ThreadX    | ThreadX       | NetX Duo       | RISC-V 64 (QEMU), Linux sim  |
| Zephyr     | Zephyr        | Zephyr sockets | Various boards               |

## RMW Backends

nano-ros supports several middleware backends, selected at compile
time by adding the backend crate as a dependency:

- **Zenoh** (`nros-rmw-zenoh`) — peer-to-peer via zenoh-pico. No agent
  process. Compatible with ROS 2 `rmw_zenoh_cpp`.
- **XRCE-DDS** (`nros-rmw-xrce-cffi`) — agent-based via Micro-XRCE-DDS.
  Compatible with micro-ROS agent.
- **dust-DDS** (`nros-rmw-dds`) — pure-Rust DDS.
- **Cyclone DDS** (`nros-rmw-cyclonedds`) — C++ shim.

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
