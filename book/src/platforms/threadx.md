# ThreadX

nano-ros runs on Eclipse ThreadX with NetX Duo networking. Two targets are
supported: a Linux simulation environment and a RISC-V 64-bit QEMU machine.

## Overview

The ThreadX platform uses:

- **ThreadX** (Eclipse ThreadX) — pre-emptive RTOS kernel with deterministic scheduling
- **NetX Duo** — TCP/IP network stack with BSD socket compatibility layer
- **zenoh-pico** — Zenoh transport over NetX Duo BSD sockets

Board crates:
- `nros-threadx-qemu-riscv64` — QEMU RISC-V 64-bit virt machine
- `nros-threadx-linux` — Linux simulation (ThreadX Linux port)

## Safety Certifications

ThreadX holds the highest level of safety certifications across multiple
standards:

- **IEC 61508 SIL 4** — functional safety for industrial systems
- **IEC 62304 Class C** — medical device software
- **ISO 26262 ASIL D** — automotive functional safety

NetX Duo is certified to the same IEC 61508 SIL 4 standard. Combined with
nano-ros's Kani/Verus formal verification, this creates a uniquely strong
safety argument for safety-critical deployments.

## Prerequisites

### Linux Simulation

- Linux host with `CAP_NET_RAW` capability
- Rust nightly toolchain

### QEMU RISC-V 64-bit

| Tool | Purpose |
|------|---------|
| `qemu-system-riscv64` | RISC-V system emulation |
| `riscv64-unknown-elf-gcc` | RISC-V bare-metal cross-compiler |
| Rust nightly + `riscv64gc-unknown-none-elf` | Rust cross-compilation |

```bash
sudo apt install qemu-system-misc gcc-riscv64-unknown-elf
rustup target add riscv64gc-unknown-none-elf
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `THREADX_DIR` | `third-party/threadx/kernel` | ThreadX kernel source |
| `THREADX_CONFIG_DIR` | Board crate's `config/` | ThreadX config (`tx_user.h`) |
| `NETX_DIR` | `third-party/threadx/netxduo` | NetX Duo source |
| `NETX_CONFIG_DIR` | Board crate's `config/` | NetX Duo config (`nx_user.h`) |

## Building

```bash
# Download ThreadX + NetX Duo
just setup-threadx

# Build Linux simulation examples
just build-examples-threadx-linux

# Build QEMU RISC-V examples
just build-examples-threadx-riscv64
```

### Available Examples

All examples are in `examples/threadx-linux/rust/zenoh/` and
`examples/qemu-riscv64-threadx/rust/zenoh/`:

| Example | Description |
|---------|-------------|
| `talker` | Publishes `std_msgs/Int32` on `/chatter` |
| `listener` | Subscribes to `std_msgs/Int32` on `/chatter` |
| `service-server` | Serves `AddTwoInts` on `/add_two_ints` |
| `service-client` | Calls `AddTwoInts` on `/add_two_ints` |
| `action-server` | Serves `Fibonacci` action on `/fibonacci` |
| `action-client` | Sends `Fibonacci` goal on `/fibonacci` |

## Testing

```bash
just test-threadx          # Both Linux sim + QEMU RISC-V
just test-threadx-linux    # Linux simulation only
just test-threadx-riscv64  # QEMU RISC-V only
```

### Network Configuration (QEMU RISC-V)

Tests use TAP networking with virtio-net:

| Role | IP Address | TAP Device |
|------|-----------|------------|
| zenohd (host) | 192.0.3.1 | br-qemu |
| Talker/Publisher | 192.0.3.10 | tap-qemu0 |
| Listener/Sub | 192.0.3.11 | tap-qemu1 |

### Linux Simulation

Linux simulation tests use AF_PACKET raw sockets. Binaries need
`CAP_NET_RAW` capability:

```bash
just setup-threadx-caps    # Build + apply capabilities (one-time)
```

## Architecture

### Linux Simulation Board Crate

The `nros-threadx-linux` board crate runs the full ThreadX kernel as
pthreads on a Linux host. NetX Duo uses a TAP network driver
(`tap-netx` in `packages/drivers/`) for Ethernet I/O via Linux TAP
devices. This provides the fastest iteration cycle for ThreadX-specific code.

### QEMU RISC-V Board Crate

The `nros-threadx-qemu-riscv64` board crate runs ThreadX's RISC-V port on
QEMU virt machine with real preemptive scheduling. NetX Duo uses a
virtio-net driver (`virtio-net-netx` in `packages/drivers/`) for Ethernet
I/O over QEMU's virtio MMIO interface.

```
User Application (Executor + Node + Pub/Sub)
        │
nros-node (Executor)
        │
nros-rmw-zenoh → zpico-sys (zenoh-pico + C shim)
        │                       │
        │          zenoh-pico ThreadX platform
        │          (tx_thread, tx_mutex, BSD sockets)
        │
Board Crate (nros-threadx-qemu-riscv64)
├── ThreadX kernel (RISC-V port)
├── NetX Duo (BSD sockets over virtio-net)
└── virtio-net NetX Duo driver (virtio MMIO)
```

### Key Design Points

- **Multi-threaded**: ThreadX provides real threads/mutexes. zenoh-pico uses
  background read/lease tasks.
- **NetX Duo BSD sockets**: POSIX-compatible `socket()`/`connect()`/`select()`
  — same code path as zenoh-pico's POSIX platform.
- **Build via `cc` crate**: ThreadX kernel + NetX Duo compiled in the board
  crate's `build.rs` (no external CMake needed).
- **`no_std` target**: `riscv64gc-unknown-none-elf` for QEMU; Linux simulation
  uses the host toolchain.

## TSN Support

NetX Duo provides Time-Sensitive Networking (TSN) capabilities (CBS, TAS,
FPE, PTP), enabling deterministic, low-latency communication for industrial
and automotive use cases. TSN support requires TSN-capable hardware.

## Status

ThreadX platform support is tracked in Phase 58. See
[Phase 58 roadmap](../../docs/roadmap/phase-58-threadx-platform.md) for
details.
