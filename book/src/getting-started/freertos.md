# FreeRTOS

nano-ros runs on FreeRTOS with lwIP networking, targeting QEMU
MPS2-AN385 (Cortex-M3 + LAN9118 Ethernet). Use this guide when you
want a preemptive RTOS target with Rust, C, and C++ examples.

## When to Use It

- You need a FreeRTOS + lwIP integration path.
- You want QEMU coverage before moving to STM32, NXP, Renesas, TI, or
  another lwIP-based board.
- You want to exercise nano-ros under a fixed-priority preemptive RTOS.

## Prerequisites

- `qemu-system-arm` for running tests.
- `arm-none-eabi-gcc` for compiling FreeRTOS and lwIP C code.
- Rust nightly with the `thumbv7m-none-eabi` target.

Install platform sources:

```bash
just freertos setup
```

This fetches FreeRTOS and lwIP into `third-party/freertos/`. Override
the defaults only if you use external source trees:

| Variable | Default | Purpose |
|---|---|---|
| `FREERTOS_DIR` | `third-party/freertos/kernel` | FreeRTOS kernel |
| `FREERTOS_PORT` | `GCC/ARM_CM3` | FreeRTOS portable layer |
| `LWIP_DIR` | `third-party/freertos/lwip` | lwIP source |
| `FREERTOS_CONFIG_DIR` | board crate `config/` | `FreeRTOSConfig.h` and `lwipopts.h` |

## Build and Run

```bash
just freertos build
```

This cross-compiles the FreeRTOS examples for `thumbv7m-none-eabi`.
The board crate build script compiles the FreeRTOS kernel, lwIP, and
LAN9118 network driver.

Rust examples live under `examples/qemu-arm-freertos/rust/zenoh/`:
`talker`, `listener`, `service-server`, `service-client`,
`action-server`, and `action-client`.

C examples live under `examples/qemu-arm-freertos/c/zenoh/`; C++
examples live under `examples/qemu-arm-freertos/cpp/zenoh/`.

## Testing

```bash
just freertos test
```

Tests boot QEMU MPS2-AN385 images and verify message exchange through a
host TAP bridge and `zenohd`.

| Role | IP address | TAP device |
|---|---|---|
| Talker / publisher | `192.0.3.10` | `tap-qemu0` |
| Listener / subscriber | `192.0.3.11` | `tap-qemu1` |
| Service server | `192.0.3.12` | `tap-qemu0` |
| Service client | `192.0.3.13` | `tap-qemu1` |
| Host `zenohd` | `192.0.3.1` | `br-qemu` |

## Architecture

The `nros-board-mps2-an385-freertos` board package follows the
standard `Config` / `run()` pattern described in
[Custom Board Package](../porting/custom-board.md). It initializes
FreeRTOS, lwIP, LAN9118 Ethernet, and semihosting output before
running the user closure as a FreeRTOS task.

The platform uses:

- FreeRTOS for tasks, mutexes, semaphores, and scheduling.
- lwIP in threaded mode (`NO_SYS=0`) with BSD sockets.
- zenoh-pico over lwIP sockets.
- LAN9118 as the QEMU Ethernet device.

## Scheduling Configuration

Task priorities and stack sizes are configured through `config.toml`:

```toml
[scheduling]
app_priority = 12
app_stack_bytes = 65536
zenoh_read_priority = 16
zenoh_read_stack_bytes = 5120
zenoh_lease_priority = 16
zenoh_lease_stack_bytes = 5120
poll_priority = 16
poll_interval_ms = 5
```

The normalized 0-31 priority scale maps to FreeRTOS priorities 0-7.
Keep these constraints for reliable networking:

- `poll_priority >= zenoh_read_priority`
- `zenoh_read_priority >= app_priority`
- `app_stack_bytes >= 16384`; 64 KB is recommended for services and
  actions.

## Tracing

Task scheduling can be visualized with Tonbandgeraet / Perfetto:

```bash
just freertos trace talker
```

Tracing is opt-in through `NROS_TRACE=1`; it uses a 16 KB RAM ring
buffer and has no overhead when disabled.

## Troubleshooting

Common FreeRTOS failures are usually caused by task priority inversion,
small application stacks, deterministic session IDs in QEMU, or missing
WFI in the idle hook. See
[FreeRTOS LAN9118 Debugging](../internals/freertos-lan9118-debugging.md)
for the detailed QEMU/LAN9118 reference.

## Status

FreeRTOS platform support includes Rust, C, and C++ examples, CMake
cross-compilation, QEMU integration tests, and shared CMake platform
modules. C++ action examples are pending alloc-free action module
support.
