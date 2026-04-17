# Platform Customization Guide

This guide classifies every nano-ros package by its role in the architecture and explains which packages you modify, which you leave alone, and where the trait boundary sits between them.

## Customization Boundary

nano-ros is split into a **core layer** that never changes and a **user layer** that you customize per target. The boundary between them is a set of Rust traits defined in `nros-platform` (`packages/core/nros-platform/src/traits.rs`).

```
+============================================================+
|                     YOUR APPLICATION                        |
+============================================================+
|              Board Crate (nros-<board>)                     |
|         Config, run(), HW init, network stack               |
+------------------------------------------------------------+
|          Platform Crate (nros-platform-<name>)              |
|    Implements: PlatformClock, PlatformAlloc, PlatformSleep, |
|    PlatformRandom, PlatformTime, PlatformThreading,         |
|    PlatformTcp, PlatformUdp, PlatformNetworkPoll, ...       |
+------------------------------------------------------------+
|          Driver Crate (lan9118-smoltcp, etc.)               |
|             smoltcp Device / lwIP / NetX Duo                |
+============================================================+
                    TRAIT BOUNDARY
              (nros-platform traits.rs)
+============================================================+
|          zpico-platform-shim / xrce-platform-shim           |
|      Maps z_* / uxr_* FFI symbols to ConcretePlatform       |
+------------------------------------------------------------+
|              RMW Backend (nros-rmw-zenoh, etc.)             |
+------------------------------------------------------------+
|          Core Library (nros-node, nros-core, nros-rmw, ...) |
+============================================================+
```

Everything above the trait boundary is yours to write or modify. Everything below it is fixed -- your platform crate implements the traits, and the core consumes them through the `ConcretePlatform` type alias resolved at compile time in `nros-platform/src/resolve.rs`.

## Package Classification

### Core (do not modify)

These crates form the stable core of nano-ros. They are generic over the platform and RMW backend, contain no hardware-specific code, and should never need modification for a new port.

| Package | Path | Purpose |
|---------|------|---------|
| `nros` | `packages/core/nros` | Facade crate: re-exports + feature-axis enforcement |
| `nros-core` | `packages/core/nros-core` | `RosMessage`, `RosService`, `RosAction` traits |
| `nros-serdes` | `packages/core/nros-serdes` | CDR serialization (`CdrWriter`, `CdrReader`) |
| `nros-macros` | `packages/core/nros-macros` | `#[derive(RosMessage)]` proc macro |
| `nros-node` | `packages/core/nros-node` | Executor, Node, pub/sub/service/action handles |
| `nros-params` | `packages/core/nros-params` | Parameter server |
| `nros-rmw` | `packages/core/nros-rmw` | RMW trait definitions (`Session`, `Publisher`, `Subscriber`) |
| `nros-platform` | `packages/core/nros-platform` | Platform trait definitions + `ConcretePlatform` alias |
| `zpico-sys` | `packages/zpico/zpico-sys` | zenoh-pico C build + Rust FFI bindings |
| `xrce-sys` | `packages/xrce/xrce-sys` | Micro-XRCE-DDS C build + Rust FFI bindings |
| `zpico-platform-shim` | inside `zpico-sys` | Maps 53 `z_*` FFI symbols to `ConcretePlatform` |
| `xrce-platform-shim` | inside `xrce-sys` | Maps 2-3 `uxr_*` FFI symbols to `ConcretePlatform` |
| `nros-c` | `packages/core/nros-c` | C API (thin FFI wrapper over `nros-node`) |
| `nros-cpp` | `packages/core/nros-cpp` | C++ API (header-only C++ + Rust FFI staticlib) |

### RMW Crates (one per middleware)

Each RMW crate implements the `nros-rmw` traits for a specific transport protocol. You select one via Cargo features. To add a new transport (e.g., DDS, MQTT), create a new `nros-rmw-<name>` crate that implements `Session`, `Publisher`, `Subscriber`, etc.

| Package | Path | Transport |
|---------|------|-----------|
| `nros-rmw-zenoh` | `packages/zpico/nros-rmw-zenoh` | zenoh-pico (Zenoh protocol) |
| `nros-rmw-xrce` | `packages/xrce/nros-rmw-xrce` | Micro-XRCE-DDS (DDS via agent) |
| `nros-rmw-cffi` | `packages/core/nros-rmw-cffi` | C vtable adapter (third-party transports) |

See [Adding an RMW Backend](./adding-rmw-backend.md) for details on creating a new RMW crate.

### Platform Crates (one per RTOS)

Platform crates implement the traits from `nros-platform/src/traits.rs`. Each crate provides clock, memory, sleep, random, and (for multi-threaded platforms) threading primitives for a specific OS or bare-metal environment.

**RTOS-level platform crates** (implement traits using RTOS APIs):

| Package | Path | Target |
|---------|------|--------|
| `nros-platform-posix` | `packages/core/nros-platform-posix` | Linux, macOS (pthreads, BSD sockets) |
| `nros-platform-freertos` | `packages/core/nros-platform-freertos` | FreeRTOS (xTaskCreate, xSemaphore, lwIP) |
| `nros-platform-nuttx` | `packages/core/nros-platform-nuttx` | NuttX (POSIX-like API) |
| `nros-platform-threadx` | `packages/core/nros-platform-threadx` | ThreadX (tx_thread, tx_mutex, NetX Duo) |
| `nros-platform-zephyr` | `packages/core/nros-platform-zephyr` | Zephyr (k_thread, k_mutex, Zephyr sockets) |
| `nros-platform-cffi` | `packages/core/nros-platform-cffi` | C function-pointer table (vendor-provided) |

**Board-level platform crates** (implement traits using bare-metal drivers):

| Package | Path | Target |
|---------|------|--------|
| `nros-platform-mps2-an385` | `packages/boards/nros-platform-mps2-an385` | QEMU Cortex-M3 (DWT, smoltcp) |
| `nros-platform-stm32f4` | `packages/boards/nros-platform-stm32f4` | STM32F4 (SysTick, smoltcp) |
| `nros-platform-esp32` | `packages/boards/nros-platform-esp32` | ESP32-C3 (esp-hal timers, WiFi) |
| `nros-platform-esp32-qemu` | `packages/boards/nros-platform-esp32-qemu` | QEMU ESP32-C3 (openeth, smoltcp) |

The distinction: RTOS-level crates are hardware-agnostic (FreeRTOS runs on many MCUs), while board-level crates are tied to specific hardware (bare-metal needs direct register access for clocks, Ethernet, etc.).

### Board Crates (one per hardware + RTOS combination)

Board crates are the user-facing entry point. They provide `Config`, `run()`, hardware initialization, and network stack setup. You write one for each target you want to support.

| Package | Path | RTOS | Network |
|---------|------|------|---------|
| `nros-mps2-an385` | `packages/boards/nros-mps2-an385` | Bare-metal | smoltcp (LAN9118) |
| `nros-mps2-an385-freertos` | `packages/boards/nros-mps2-an385-freertos` | FreeRTOS | lwIP (LAN9118) |
| `nros-stm32f4` | `packages/boards/nros-stm32f4` | Bare-metal | smoltcp (STM32 Ethernet) |
| `nros-esp32` | `packages/boards/nros-esp32` | Bare-metal | WiFi (esp-hal) |
| `nros-esp32-qemu` | `packages/boards/nros-esp32-qemu` | Bare-metal | smoltcp (openeth) |
| `nros-nuttx-qemu-arm` | `packages/boards/nros-nuttx-qemu-arm` | NuttX | BSD sockets |
| `nros-threadx-qemu-riscv64` | `packages/boards/nros-threadx-qemu-riscv64` | ThreadX | NetX Duo (VirtIO) |
| `nros-threadx-linux` | `packages/boards/nros-threadx-linux` | ThreadX | veth sockets |

See [Board Crate Implementation](./board-crate.md) for the full guide on creating a new board crate.

### Driver Crates

Peripheral drivers for network hardware. These implement smoltcp's `Device` trait, lwIP's driver interface, or NetX Duo's driver callbacks.

| Package | Path | Hardware | Stack |
|---------|------|----------|-------|
| `nros-smoltcp` | `packages/zpico/nros-smoltcp` | Generic smoltcp utilities | smoltcp |
| `zpico-smoltcp` | `packages/zpico/zpico-smoltcp` | smoltcp TCP/UDP bridge for zenoh-pico | smoltcp |
| `xrce-smoltcp` | `packages/xrce/xrce-smoltcp` | smoltcp UDP transport for XRCE-DDS | smoltcp |
| `lan9118-smoltcp` | `packages/drivers/lan9118-smoltcp` | LAN9118 Ethernet (QEMU, STM32F4) | smoltcp |
| `lan9118-lwip` | `packages/drivers/lan9118-lwip` | LAN9118 Ethernet (FreeRTOS) | lwIP |
| `openeth-smoltcp` | `packages/drivers/openeth-smoltcp` | Open Ethernet (QEMU ESP32) | smoltcp |
| `virtio-net-netx` | `packages/drivers/virtio-net-netx` | VirtIO-Net (QEMU RISC-V) | NetX Duo |
| `zpico-serial` | `packages/zpico/zpico-serial` | UART serial transport | N/A |
| `zpico-alloc` | `packages/zpico/zpico-alloc` | Custom allocator for zenoh-pico | N/A |

## The Pattern

The customization pattern is always the same:

1. **Implement platform traits** in an `nros-platform-<name>` crate. Your struct (e.g., `FreeRtosPlatform`) implements whichever traits the RMW backend requires: `PlatformClock`, `PlatformAlloc`, `PlatformSleep`, `PlatformRandom`, `PlatformTime`, `PlatformThreading`, and optionally the networking traits (`PlatformTcp`, `PlatformUdp`).

2. **Register via Cargo features.** Add a `platform-<name>` feature to `nros-platform/Cargo.toml` that pulls in your crate as an optional dependency. Add a `ConcretePlatform` type alias in `resolve.rs` gated by that feature. The shim crates (`zpico-platform-shim`, `xrce-platform-shim`) automatically dispatch to your implementation through this alias.

3. **Write a board crate** that calls `init_hardware()`, sets up the network stack, and exposes `Config` + `run()`. This is where smoltcp/lwIP/NetX initialization lives.

4. **Core crates are untouched.** `nros-node`, `nros-core`, `nros-rmw-zenoh`, etc. compile and work without modification. They interact with your platform solely through the trait boundary.

### Trait Requirements by RMW Backend

Not every trait is needed. The requirements depend on which RMW backend you use:

| Trait | zenoh-pico | XRCE-DDS |
|-------|-----------|----------|
| `PlatformClock` | Required | Required |
| `PlatformAlloc` | Required (~64 KB heap) | Not needed |
| `PlatformSleep` | Required | Not needed |
| `PlatformRandom` | Required | Not needed |
| `PlatformTime` | Required (logging) | Not needed |
| `PlatformThreading` | Required (multi-threaded) | Not needed |
| `PlatformTcp` | Required (TCP transport) | Not needed |
| `PlatformUdp` | Required (UDP transport) | Custom callbacks |
| `PlatformNetworkPoll` | Bare-metal only | Not needed |
| `PlatformLibc` | Bare-metal only | Not needed |

XRCE-DDS is significantly simpler to port because it is single-threaded, heap-free, and uses user-provided transport callbacks rather than a socket API.

## Common Customization Scenarios

**Adding a new RTOS:** Create `nros-platform-<rtos>` implementing the traits using your RTOS APIs. Register the feature in `nros-platform/Cargo.toml` and `resolve.rs`. Then create a board crate for your target hardware. See [Porting to a New Platform](./porting-platform/README.md).

**Adding a new board on an existing RTOS:** Create a board crate that depends on the existing `nros-platform-<rtos>`. Write the hardware init (clocks, Ethernet driver, network stack). See [Board Crate Implementation](./board-crate.md).

**Adding a new Ethernet driver:** Create a driver crate in `packages/drivers/` implementing smoltcp's `Device` trait (or lwIP/NetX driver interface). Use it from your board crate.

**Adding a new RMW transport:** Create `nros-rmw-<name>` implementing the `Session` trait from `nros-rmw`. Add a feature to the `nros` facade. See [Adding an RMW Backend](./adding-rmw-backend.md).

## Related Documentation

- [Architecture](../concepts/architecture.md) -- full crate dependency graph and layer diagram
- [Platform Model](../concepts/platform-model.md) -- conceptual overview of the three feature axes
- [Porting to a New Platform](./porting-platform/README.md) -- step-by-step guide for new RTOS ports
- [Board Crate Implementation](./board-crate.md) -- how to write `Config`, `run()`, and hardware init
- [Platform API Reference](../reference/platform-api.md) -- complete trait signatures and method documentation
