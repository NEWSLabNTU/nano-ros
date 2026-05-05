# NuttX

nano-ros runs on NuttX, targeting QEMU ARM virt (Cortex-A7 + virtio-net).
NuttX provides POSIX-compatible BSD sockets, which makes it the most
straightforward RTOS port -- zenoh-pico uses the same socket API as on Linux.

## Overview

The NuttX platform uses:

- **NuttX RTOS** -- POSIX-compliant real-time OS with BSD socket support
- **BSD sockets** -- standard socket API provided by NuttX's network stack
- **zenoh-pico** -- Zenoh transport over NuttX sockets (same code path as POSIX)
- **virtio-net** -- NuttX built-in Ethernet driver (no custom driver needed)

Board crate: `nros-board-nuttx-qemu-arm` (in `packages/boards/`).

### Why NuttX Is Simpler Than FreeRTOS

NuttX is the simplest RTOS platform to port because of its strong POSIX
compliance (POSIX.1-2008: pthreads, BSD sockets, `select()`,
`clock_gettime()`):

| Aspect           | NuttX                          | FreeRTOS                             |
|------------------|--------------------------------|--------------------------------------|
| zenoh-pico layer | Reuses `unix/` platform        | Dedicated `freertos/` platform       |
| Networking       | Built-in BSD sockets           | External lwIP                        |
| Ethernet driver  | NuttX virtio-net (built-in)    | Custom LAN9118 lwIP netif            |
| Rust target      | `armv7a-nuttx-eabi` with `std` | `thumbv7m-none-eabi` (`no_std`)      |
| Build integration| NuttX build system + cargo     | cc crate compiles FreeRTOS + lwIP    |
| QEMU machine     | `virt` (Cortex-A7)             | `mps2-an385` (Cortex-M3)            |

Because NuttX supports Rust `std`, examples use standard `fn main()` entry
points and `println!` -- no semihosting or custom panic handlers needed.

## Setup

Download the NuttX source and apps:

```bash
just nuttx setup
```

This places the sources in `third-party/nuttx/nuttx/` and `third-party/nuttx/nuttx-apps/`.
Override the paths with environment variables if your sources are elsewhere:

| Variable         | Default              | Description          |
|------------------|----------------------|----------------------|
| `NUTTX_DIR`      | `third-party/nuttx/nuttx`     | NuttX RTOS source    |
| `NUTTX_APPS_DIR` | `third-party/nuttx/nuttx-apps`| NuttX apps source    |

### Prerequisites

- `qemu-system-arm` (for running tests)
- Rust nightly toolchain (NuttX targets are Tier 3, require `-Z build-std`)
- `arm-none-eabi-gcc` (for NuttX kernel compilation)

## Building

```bash
just nuttx build
```

This cross-compiles all NuttX examples for `armv7a-nuttx-eabi` using
`cargo +nightly build --release`. The examples link against NuttX's POSIX
layer, which provides sockets, pthreads, and standard I/O.

### Available Examples

All examples are in `examples/qemu-arm-nuttx/rust/zenoh/`:

| Example          | Description                                      |
|------------------|--------------------------------------------------|
| `talker`         | Publishes `std_msgs/Int32` on `/chatter`         |
| `listener`       | Subscribes to `std_msgs/Int32` on `/chatter`     |
| `service-server` | Serves `AddTwoInts` on `/add_two_ints`           |
| `service-client` | Calls `AddTwoInts` on `/add_two_ints`            |
| `action-server`  | Serves `Fibonacci` action on `/fibonacci`        |
| `action-client`  | Sends `Fibonacci` goal on `/fibonacci`           |

## Testing

```bash
just test-nuttx
```

Tests run under `qemu-system-arm -M virt` with TAP networking. Each QEMU
instance connects to the host bridge (`br-qemu`) via TAP devices for
zenohd communication. The test infrastructure builds a NuttX kernel image
with the example app embedded, boots it in QEMU, and verifies message
exchange.

### Network Configuration

NuttX QEMU instances use the same IP scheme as other QEMU board crates:

| Role             | IP Address    | TAP Device  |
|------------------|---------------|-------------|
| Talker/Publisher | 192.0.3.10    | tap-qemu0   |
| Listener/Sub     | 192.0.3.11    | tap-qemu1   |
| Service Server   | 192.0.3.12    | tap-qemu0   |
| Service Client   | 192.0.3.13    | tap-qemu1   |
| zenohd (host)    | 192.0.3.1     | br-qemu     |

## Architecture

### Board Crate

The `nros-board-nuttx-qemu-arm` board crate follows the standard `Config` / `run()` pattern documented in the [Board Crate Guide](../internals/board-crate.md). It provides network and node configuration presets (`talker()`, `listener()`, `server()`, `client()`).

Unlike bare-metal and FreeRTOS board crates, there is no custom hardware
initialization, no network stack setup, and no task creation. NuttX's kernel
boots the hardware, initializes virtio-net, and starts the application
before `main()` runs. Because NuttX supports Rust `std`, examples use standard
`fn main()` entry points.

### NuttX Defconfig

The QEMU board configuration lives in
`packages/boards/nros-board-nuttx-qemu-arm/nuttx-config/` and enables:

- `CONFIG_NET` -- networking subsystem
- `CONFIG_NET_TCP` / `CONFIG_NET_UDP` -- TCP/UDP protocols
- `CONFIG_DRIVERS_VIRTIO` + `CONFIG_DRIVERS_VIRTIO_NET` -- virtio Ethernet
- `CONFIG_PTHREAD_MUTEX_TYPES` -- POSIX mutex types (for zenoh-pico)
- `CONFIG_DEV_URANDOM` -- `/dev/urandom` for session ID generation
- `CONFIG_DEFAULT_TASK_STACKSIZE=8192` -- adequate stack for nros apps
- `CONFIG_BUILD_FLAT` -- flat memory model (no MMU protection)

## Status

NuttX platform support is complete: feature flags, build integration,
six Rust examples, E2E network tests, and documentation.
