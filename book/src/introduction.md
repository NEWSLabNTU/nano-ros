# Introduction

nano-ros is a lightweight ROS 2 client library for embedded real-time systems.
It runs on bare-metal microcontrollers, FreeRTOS, NuttX, ThreadX, and Zephyr,
as well as Linux and *BSD. The entire core stack is `no_std` compatible.

```mermaid
flowchart TB
    App["<b>Application</b><br/>Rust / C / C++ node code"]
    Core["<b>nano-ros core</b><br/>Executor · Node · pub/sub · services · actions · CDR"]
    RMW["<b>RMW backend</b><br/>Zenoh · XRCE-DDS · Cyclone DDS · custom"]
    Plat["<b>Platform</b><br/>clock · heap · threads · sleep · sockets · libc"]
    Wire(["ROS 2 / DDS / Zenoh wire"])
    App --> Core --> RMW --> Plat
    RMW -. wire-compatible .-> Wire
```

Four layers, swappable independently: the same node code runs over any RMW
backend on any platform. Here is a complete Linux publisher — register a
backend, open the executor, publish `std_msgs/String` (`Hello World: N`)
on `/chatter` once a second:

```rust
use core::fmt::Write as _;
use nros::prelude::*;
use std_msgs::msg::String as StringMsg;

fn main() {
    // Pick the backend at compile time; this one line registers it.
    nros_rmw_zenoh::register().unwrap();

    let config = ExecutorConfig::new("tcp/127.0.0.1:7447").node_name("talker");
    let mut executor: Executor = Executor::open(&config).unwrap();

    let mut node = executor.create_node("talker").unwrap();
    let publisher = node.create_publisher::<StringMsg>("/chatter").unwrap();

    let mut count = 0i32;
    executor
        .register_timer(nros::TimerDuration::from_millis(1000), move || {
            count += 1;
            let mut msg = StringMsg::default();
            let _ = write!(msg.data, "Hello World: {count}");
            publisher.publish(&msg).unwrap();
        })
        .unwrap();

    executor.spin_blocking(SpinOptions::default()).unwrap();
    // This is the imperative Layer-2 API; the starters use the
    // declarative `nros::node!` / `nros::main!` form — both are
    // first-class (see Concepts → Two-Layer API).
}
```

The same program in C and C++ is in the First Node guides:
[Rust](./getting-started/first-node-rust.md) ·
[C](./getting-started/first-node-c.md) ·
[C++](./getting-started/first-node-cpp.md).
When a project grows beyond one node, continue with
[Multi-Node Project Layout](./getting-started/workspace-from-app-node.md).

## Key Features

- **Minimal stack** — three software layers (application, nano-ros,
  transport). Lean dependency tree, fast compile times.
- **Pluggable middleware** — choose Zenoh (agent-less, direct peer
  communication), XRCE-DDS (agent-based), or Cyclone DDS (RTPS
  wire-compatible with stock ROS 2) at compile time. Same application
  code regardless of backend.
- **The same source runs on every supported target** — a node body written for
  one platform compiles unchanged on the others; what differs is build
  configuration (`Cargo.toml`, `.cargo/config.toml`, `CMakeLists.txt`), not the
  code you write. This is asserted, not asserted-about: the
  `example_portability` test normalizes every platform's copy of each example
  and fails if any two differ, so the claim is checked on every run rather than
  maintained by hope. Two execution models are declared exceptions with written
  reasons — bare-metal deferred dispatch, and Zephyr's component shape — and the
  test names them explicitly rather than hiding them.
- **Rust-first with C API** — the core is written in Rust for memory safety
  and ergonomics, with a thin C FFI (Foreign Function Interface) layer
  following rclc conventions.
- **True `no_std`** — runs on bare-metal Cortex-M3; the `alloc` and `std`
  features are opt-in. Whether the IMAGE needs an allocator is the backend's
  call, not the core's: XRCE is fully static, zenoh-pico wants one (a bump
  allocator suffices on bare-metal), and Cyclone DDS requires a real heap.
- **Standalone tooling** — `nros generate-rust` produces message
  bindings without a ROS 2 installation (bundled interface definitions).
- **Formally verified** — 148 Kani bounded model checking harnesses and 83
  Verus deductive proofs cover CDR serialization, scheduling, and protocol
  correctness.
- **ROS 2 compatible** — interoperates with standard ROS 2 nodes via
  `rmw_zenoh_cpp`, or directly over RTPS with `rmw_cyclonedds_cpp` (same
  wire protocol, no key rewriting). Topics, services, and actions work
  across the boundary.

## Quick board check — does it work on the board I have today?

| Vendor / form factor      | Chip          | RTOS / no-RTOS  | Languages | Example in repo                                   | ROS 2 interop |
|---------------------------|---------------|-----------------|-----------|---------------------------------------------------|---------------|
| ARM MPS2-AN385 (QEMU)     | Cortex-M3     | FreeRTOS / bare | Rust C C++ ¹ | `examples/qemu-arm-{freertos,baremetal}/`         | Verified      |
| ST STM32F4-Discovery      | Cortex-M4F    | bare            | Rust ²    | out-of-tree — [worked example](porting/stm32f4-out-of-tree.md) | Untested ⁴ |
| Espressif ESP32-C3        | RISC-V (RV32) | ESP-IDF         | Rust C C++ | `integrations/nano-ros/`                          | Ready         |
| Espressif ESP32-C3 (QEMU) | RISC-V        | bare            | Rust      | `examples/qemu-esp32-baremetal/`                  | Verified      |
| QEMU `virt` RISC-V64      | RV64GC        | ThreadX         | Rust C C++ | `examples/qemu-riscv64-threadx/`                  | Verified      |
| Linux host                | x86-64 / aarch64 | ThreadX sim  | Rust C C++ | `examples/threadx-linux/`                         | Verified      |
| QEMU virt / Cortex-A9     | Cortex-A7 / A9 | NuttX / Zephyr | Rust C C++ | `examples/qemu-arm-nuttx/`, `examples/zephyr/`    | Verified      |
| Pixhawk 4 / 6X            | STM32F7 / H7  | NuttX (PX4)     | C++       | `integrations/px4/module-template/`               | Ready ³       |
| Generic Cortex-M0+/M4/M7  | ≥ 64 KB SRAM  | RTOS of choice  | Rust C C++ | Use your board's vendor BSP + integrations shells | Pattern shown |

**Legend:** *Verified* = booted + tested in CI. *Ready* = builds but no
in-CI gate yet — where the row names an `examples/<plat>/` directory there
is an app to compile and try; where it names an `integrations/` shell there
is build glue but no worked reference app yet.

Footnotes — ¹ MPS2-AN385 bare-metal is Rust-only (`nros-c` / `nros-cpp`
need an RTOS for libc / heap). ² STM32F4 is reached through the
customization ladder, not an in-tree crate; a FreeRTOS variant sits on the
shared `nros-board-freertos` glue. ³ PX4 path is via the
external-module template in `integrations/px4/` — C++ only because
PX4's uORB binding is C++-only. ⁴ *Untested* = the code path exists and the
port is documented, but no lane in this repo boots it — the hardware is not in
the test rack and QEMU models no STM32 MAC. phase-337 W7.a moved the two
STM32F4 board crates out of the tree for exactly that reason; keeping a row
that says "Verified" for a board nothing verifies is the failure mode the tier
registry exists to prevent.

## Supported platforms (by RTOS)

| Platform   | RTOS          | Network Stack  | Targets                      |
|------------|---------------|----------------|------------------------------|
| POSIX      | Linux / *BSD | OS sockets     | x86-64, aarch64              |
| Bare-metal | None          | smoltcp        | Cortex-M3, ESP32-C3          |
| FreeRTOS   | FreeRTOS      | lwIP           | Cortex-M3 (QEMU)             |
| NuttX      | NuttX         | BSD sockets    | Cortex-A7 (QEMU), RISC-V32 (QEMU rv-virt) |
| ThreadX    | ThreadX       | NetX Duo       | RISC-V 64 (QEMU), Linux sim  |
| Zephyr     | Zephyr        | Zephyr sockets | Various boards               |

## RMW Backends

nano-ros supports several middleware backends, selected at compile
time by adding the backend crate as a dependency:

- **Zenoh** (`nros-rmw-zenoh`) — peer-to-peer via zenoh-pico. No agent
  process. Compatible with ROS 2 `rmw_zenoh_cpp`.
- **XRCE-DDS** (`nros-rmw-xrce-cffi`) — agent-based via Micro-XRCE-DDS.
  Compatible with micro-ROS agent.
- **Cyclone DDS** (`nros-rmw-cyclonedds`) — C++ shim; full RTPS wire-compat
  with stock `rmw_cyclonedds_cpp`.

Application code is identical regardless of backend — switch with a single
Cargo feature flag or Zephyr Kconfig option.

## Project Status

nano-ros is under active development. Core capabilities are functional and
exercised in CI; see the platform chapters for per-target detail.

| Capability       | Status   |
|------------------|----------|
| Pub/Sub          | Complete |
| Services         | Complete |
| Actions          | Complete |
| Parameters       | Complete |
| ROS 2 interop    | Complete |
| Zenoh backend    | Complete |
| XRCE-DDS backend | Complete |
| Cyclone DDS backend | Complete (native + embedded; some embedded action paths in progress) |
| Zephyr support   | Complete |
| QEMU bare-metal  | Complete |
| C API            | Complete |
| C++ API          | Complete |
| Message codegen  | Complete |

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
