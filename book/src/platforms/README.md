# Platforms Overview

nano-ros supports six platform targets, spanning hosted operating systems,
real-time kernels, and bare-metal environments. Each platform provides the OS
primitives that nano-ros needs -- clocks, memory allocation, networking, and
random number generation -- while the application code remains the same.

## Platform Architecture

Every embedded platform follows a three-crate pattern:

- **`nros-platform-*`** -- platform primitives crate that provides clock,
  memory, sleep, random, and threading. Has zero nros dependencies.
- **`zpico-platform-shim` / `xrce-platform-shim`** -- thin FFI shim layers
  inside `zpico-sys` / `xrce-sys` that map transport-specific symbols
  (`z_*`, `uxr_*`) to the unified `ConcretePlatform` type alias from
  `nros-platform`.
- **`nros-*`** (board crate) -- user-facing crate that depends on the
  platform crate, initializes hardware, sets up the network stack, and
  provides a convenience `run()` API for application startup.

POSIX is the exception: it uses the host OS directly and needs no board crate.

## Platform Selection

Select a platform via Cargo feature flags on `nros-node` (or the board crate):

```toml
[dependencies]
nros-node = { version = "0.1", features = ["platform-posix", "rmw-zenoh", "std"] }
```

On Zephyr, selection is via Kconfig (`CONFIG_NROS=y`). The three orthogonal
axes -- platform, RMW backend, and ROS edition -- are enforced to be mutually
exclusive at compile time.

## Supported Platforms

| Platform   | RTOS          | Network Stack  | Targets                       | Board Crate                  |
|------------|---------------|----------------|-------------------------------|------------------------------|
| POSIX      | Linux / macOS | OS sockets     | x86-64, aarch64               | *(none needed)*              |
| Bare-metal | None          | smoltcp        | Cortex-M3, ESP32-C3, STM32F4 | `nros-mps2-an385`, `nros-esp32`, etc. |
| FreeRTOS   | FreeRTOS      | lwIP           | Cortex-M3 (QEMU)             | `nros-mps2-an385-freertos`   |
| NuttX      | NuttX         | BSD sockets    | Cortex-A7 (QEMU)             | `nros-nuttx-qemu-arm`        |
| ThreadX    | ThreadX       | NetX Duo       | RISC-V 64 (QEMU), Linux sim  | `nros-threadx-qemu-riscv64`  |
| Zephyr     | Zephyr        | Zephyr sockets | Various boards                | *(Zephyr module)*            |

## Common Environment Variables

All platforms share these runtime settings:

| Variable        | Default               | Description                      |
|-----------------|-----------------------|----------------------------------|
| `ROS_DOMAIN_ID` | `0`                   | ROS 2 domain isolation           |
| `ZENOH_LOCATOR` | `tcp/127.0.0.1:7447`  | Zenoh router address             |
| `ZENOH_MODE`    | `client`              | Zenoh session mode (`client` or `peer`) |

On embedded platforms these are typically set at compile time via board crate
configuration or Kconfig options rather than environment variables.

The following chapters cover each platform in detail.
