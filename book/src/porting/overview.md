# Porting Overview

nano-ros is designed for customization at three independent levels: **RMW** (transport protocol), **Platform** (OS or RTOS), and **Board** (hardware). The core crates define stable interfaces through Rust traits. You extend nano-ros by implementing those traits for your target -- you never modify the core.

## Which chapter do I need?

| I want to... | Chapter |
|---|---|
| Add a new transport protocol (MQTT, DDS, custom) | [Custom RMW Backend](custom-rmw.md) |
| Port to a new RTOS or bare-metal target | [Custom Platform](custom-platform.md) |
| Bring up nano-ros on a new MCU board | [Custom Board Package](custom-board.md) |

Most porting work falls into the second or third category. Adding a new RMW backend is rare and substantially more involved.

## Quickstart — 2 crates + 2 tomls (RFC-0049)

A port never edits a central file in the nano-ros tree. Scaffold the two
packages, fill in the TODOs, done:

```sh
nros new platform myrtos                       # nros-platform-myrtos/ + nros-platform.toml
nros new board myboard --for-platform myrtos   # nros-board-myboard/  + nros-board.toml
```

- **`nros-platform.toml`** (platform package) — software-stack facts
  (`[capabilities]`), knob defaults (`[knobs.*]`, only flip what you
  measured), and the zenoh-pico system-layer build block (`[build.zenoh]`).
  An empty file is valid: built-in defaults always produce a working build.
- **`nros-board.toml`** (board package) — the RFC-0042 hardware descriptor
  plus per-board `[capabilities]`/`[knobs]` deltas.
- Values resolve through a fixed ladder — `builtin < platform < board <
  env/Kconfig/-D` — and an explicit build-time setting (including an
  explicit `0`/`n`) always wins. Debug any knob with:

```sh
nros config explain --platform myrtos [--board-toml path/to/nros-board.toml]
# knob                     value      set by
# zenoh.tx.batch           false      builtin
```

Kconfig appears only where the host framework is Kconfig-native
(Zephyr / NuttX / ESP-IDF packagings) — a hand-wired fragment whose
defaults mirror the platform toml (drift-tested). A port to a
non-Kconfig RTOS never touches it.

## What stays untouched

The following core packages define the interfaces you implement. They compile and work without modification for any new target.

| Package | Role |
|---|---|
| `nros` | Facade crate: re-exports and feature-axis enforcement |
| `nros-core` | Message, service, and action type traits |
| `nros-serdes` | CDR serialization |
| `nros-node` | Executor, Node, pub/sub/service/action handles |
| `nros-rmw` | RMW trait definitions (Session, Publisher, Subscriber, etc.) |
| `nros-platform` | Platform trait definitions and `ConcretePlatform` type alias |
| `zpico-sys` alias TU | Maps zenoh-pico `z_*` C symbols to `nros_platform_*` (default-on `platform-aliases`) |
| `nros-rmw-xrce` alias TU | Maps XRCE-DDS `uxr_*` C symbols to `nros_platform_*` |
| `nros-c`, `nros-cpp` | C and C++ API wrappers |

These define the interfaces. You implement them; you do not modify them.

## The customization boundary

Everything in nano-ros sits on one side of a trait boundary defined in `nros-platform/src/traits.rs`.

**Above the boundary** (yours to write): board crates, platform crates, peripheral drivers, and application code.

**Below the boundary** (fixed): RMW backends, shim crates, core library, executor, and serialization.

Your platform crate implements traits as inherent methods on a zero-sized type. The shim crates automatically forward RMW-layer C symbols to your implementation through the `ConcretePlatform` type alias -- no dynamic dispatch, no generics propagation.

## Platform trait requirements by RMW backend

Not every trait is required. The set depends on which RMW backend the application uses.

| Trait | zenoh-pico | XRCE-DDS |
|---|---|---|
| `PlatformClock` | Required | Required |
| `PlatformAlloc` | Required (~64 KB heap) | Not needed |
| `PlatformSleep` | Required | Not needed |
| `PlatformRandom` | Required | Not needed |
| `PlatformTime` | Required | Not needed |
| `PlatformThreading` | Required (multi-threaded platforms) | Not needed |
| `PlatformTcp` | Required | Not needed |
| `PlatformUdp` | Required | Not needed |
| `PlatformSocketHelpers` | Required | Not needed |
| `PlatformNetworkPoll` | Bare-metal only | Not needed |
| `PlatformUdpMulticast` | Desktop platforms only | Not needed |
| `PlatformLibc` | Bare-metal only | Not needed |

XRCE-DDS is significantly simpler to port: it is single-threaded, heap-free, and uses user-provided transport callbacks rather than a full socket API. A minimal XRCE-DDS port requires only `PlatformClock` and four C function pointers (open, close, read, write).

zenoh-pico requires a complete platform implementation but provides richer functionality (peer-to-peer, scouting, zero-copy receive, actions).

## Registration

After implementing the required traits, you register your platform with two changes:

1. Add a `platform-<name>` feature to `nros-platform/Cargo.toml` that pulls in your crate as an optional dependency.
2. Add a `ConcretePlatform` type alias in `nros-platform/src/resolve.rs` gated by that feature.

The shim crates pick up the new platform automatically. No changes to RMW backends or core crates are needed.

## Existing platform implementations

These serve as reference when writing a new port.

| Platform crate | Target | Threading | Networking |
|---|---|---|---|
| `nros-platform-posix` | Linux, *BSD | pthreads | libc BSD sockets |
| `nros-platform-freertos` | FreeRTOS | FreeRTOS tasks | lwIP |
| `nros-platform-nuttx` | NuttX | pthreads | POSIX sockets |
| `nros-platform-threadx` | ThreadX | ThreadX threads | NetX Duo |
| `nros-platform-zephyr` | Zephyr | Zephyr POSIX | Zephyr sockets |
| `nros-platform-mps2-an385` | Cortex-M3 bare-metal | Single-threaded | smoltcp |
| `nros-platform-stm32f4` | STM32F4 bare-metal | Single-threaded | smoltcp |
| `nros-platform-esp32-qemu` | ESP32-C3 (QEMU) bare-metal | Single-threaded | smoltcp + OpenETH |

## Further reading

- [Custom RMW Backend](custom-rmw.md) -- implementing a new transport protocol
- [Custom Platform](custom-platform.md) -- porting to a new RTOS or bare-metal target
- [Custom Board Package](custom-board.md) -- bringing up a new MCU board
- [Platform API Reference](../reference/platform-api.md) -- complete trait signatures and method documentation
- [RMW API Reference](../reference/rmw-api.md) -- RMW trait hierarchy and backend details
- [Architecture Overview](../concepts/architecture.md) -- concise layer map
- [Platform Model](../concepts/platform-model.md) -- conceptual overview of the three feature axes
