# FreeRTOS Development Setup

Guide for developing and testing nano-ros on FreeRTOS + lwIP using QEMU
MPS2-AN385 (ARM Cortex-M3).

## Prerequisites

- `qemu-system-arm` (QEMU with ARM system emulation)
- `arm-none-eabi-gcc` (ARM bare-metal cross-compiler)
- Rust nightly toolchain with `thumbv7m-none-eabi` target

```bash
# Install cross-compiler (Ubuntu/Debian)
sudo apt install qemu-system-arm gcc-arm-none-eabi

# Install Rust target
rustup target add thumbv7m-none-eabi
rustup component add --toolchain nightly rust-src
```

Or use `just setup` which installs Rust targets automatically.

## FreeRTOS + lwIP Sources

Download the FreeRTOS kernel and lwIP:

```bash
just freertos setup
```

This shallow-clones:
- FreeRTOS kernel (V11.2.0) → `third-party/freertos/kernel/`
- lwIP (STABLE-2_2_1_RELEASE) → `third-party/freertos/lwip/`

Override paths if sources are elsewhere:

| Variable              | Default                    | Description                          |
|-----------------------|----------------------------|--------------------------------------|
| `FREERTOS_DIR`        | `third-party/freertos/kernel` | FreeRTOS kernel source               |
| `FREERTOS_PORT`       | `GCC/ARM_CM3`              | FreeRTOS portable layer              |
| `LWIP_DIR`            | `third-party/freertos/lwip`            | lwIP source                          |
| `FREERTOS_CONFIG_DIR` | Board crate's `config/`    | `FreeRTOSConfig.h` + `lwipopts.h`   |

## Building Examples

```bash
just build-examples-freertos
```

This cross-compiles all FreeRTOS examples for `thumbv7m-none-eabi`. The board
crate's `build.rs` compiles FreeRTOS kernel, lwIP, and the LAN9118 lwIP netif
driver via the `cc` crate.

### Available Examples

All examples are in `examples/qemu-arm-freertos/rust/zenoh/`:

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
just test-freertos
```

This runs QEMU-based integration tests with TAP networking. Each QEMU instance
connects to the host via a TAP device, communicating through a bridge running
zenohd:

```
┌─────────────────────────────────────────────────┐
│  Host (Linux)                                    │
│  zenohd (192.0.3.1)  QEMU talker  QEMU listener │
│       │                   │              │        │
│  ─────┴───────────────────┴──────────────┴────── │
│       br-qemu         tap-qemu0      tap-qemu1   │
└─────────────────────────────────────────────────┘
```

### Network Setup

TAP bridge setup requires root. The test infrastructure handles this
automatically via `scripts/qemu/setup-network.sh`.

| Role             | IP Address  | TAP Device |
|------------------|-------------|------------|
| zenohd (host)    | 192.0.3.1   | br-qemu    |
| Talker/Publisher  | 192.0.3.10  | tap-qemu0  |
| Listener/Sub     | 192.0.3.11  | tap-qemu1  |
| Service Server   | 192.0.3.12  | tap-qemu0  |
| Service Client   | 192.0.3.13  | tap-qemu1  |

## Architecture

The FreeRTOS platform stack:

```
User Application (Executor + Node + Pub/Sub)
        │
nros-node (Executor)
        │
nros-rmw-zenoh → zpico-sys (zenoh-pico + C shim)
        │                       │
        │          zenoh-pico FreeRTOS platform
        │          (system.c + lwip/network.c)
        │
Board Crate (nros-board-mps2-an385-freertos)
├── FreeRTOS kernel (tasks, mutexes, semaphores)
├── lwIP TCP/IP stack (BSD sockets, threaded mode)
└── LAN9118 lwIP netif driver (Ethernet on QEMU)
```

### Key Design Points

- **Multi-threaded**: FreeRTOS provides real tasks/mutexes. zenoh-pico uses
  background read/lease tasks.
- **lwIP socket API**: POSIX-compatible `socket()`/`connect()`/`select()` —
  same code path as zenoh-pico's POSIX platform.
- **Build via `cc` crate**: FreeRTOS kernel + lwIP compiled in the board
  crate's `build.rs` (no external CMake needed).
- **`no_std` target**: `thumbv7m-none-eabi` with semihosting output.

### Task Priorities

| Priority | Task             | Role                                    |
|----------|------------------|-----------------------------------------|
| 4        | tcpip_thread     | lwIP TCP/IP processing                  |
| 4        | poll task        | LAN9118 RX FIFO → lwIP                  |
| 4        | zenoh read/lease | zenoh-pico background I/O               |
| 3        | app task         | nros application (Executor + Node)      |
| 2        | timer task       | FreeRTOS software timers                |
| 0        | idle             | WFI (mandatory for QEMU networking)     |

## Debugging

See [FreeRTOS LAN9118 Debugging Guide](freertos-lan9118-debugging.md) for
detailed register-level debugging, common pitfalls, and QEMU networking
internals.

## Porting to Real Hardware

The QEMU board crate (`nros-board-mps2-an385-freertos`) validates the FreeRTOS
integration pattern. To port to a real board (e.g., STM32F767 Nucleo):

1. Create a new board crate (e.g., `nros-stm32f767-freertos`)
2. Replace LAN9118 netif with the vendor Ethernet driver (STM32 HAL ETH)
3. Update `FreeRTOSConfig.h` for the target MCU (clock, heap, priorities)
4. Update `lwipopts.h` for available RAM
5. Update the linker script for the target's memory map
6. Keep the same `run()` pattern and `Config` builder

The nros Executor, Node, and zenoh-pico integration are unchanged — only the
board-level HAL and Ethernet driver differ.
