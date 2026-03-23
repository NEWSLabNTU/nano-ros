# ThreadX Development Setup

Guide for developing and testing nano-ros on ThreadX (Eclipse ThreadX) + NetX
Duo, with two targets: Linux simulation and QEMU RISC-V 64-bit virt machine.

## Prerequisites

### Linux Simulation

- Linux host with CAP_NET_RAW capability (for AF_PACKET raw sockets)
- Rust nightly toolchain

### QEMU RISC-V 64-bit

- `qemu-system-riscv64` (QEMU with RISC-V system emulation)
- `riscv64-unknown-elf-gcc` (RISC-V bare-metal cross-compiler)
- Rust nightly toolchain with `riscv64gc-unknown-none-elf` target

```bash
# Install cross-compiler (Ubuntu/Debian)
sudo apt install qemu-system-misc gcc-riscv64-unknown-elf

# Install Rust target
rustup target add riscv64gc-unknown-none-elf
rustup component add --toolchain nightly rust-src
```

Or use `just setup` which installs Rust targets automatically.

## ThreadX + NetX Duo Sources

Download ThreadX, NetX Duo, and the Linux simulation samples:

```bash
just setup-threadx
```

This shallow-clones:
- ThreadX kernel → `third-party/threadx/kernel/`
- NetX Duo → `third-party/threadx/netxduo/`
- ThreadX learn samples (Linux network driver) → `third-party/threadx/learn-samples/`

Override paths if sources are elsewhere:

| Variable             | Default                       | Description                    |
|----------------------|-------------------------------|--------------------------------|
| `THREADX_DIR`        | `third-party/threadx/kernel`            | ThreadX kernel source          |
| `THREADX_CONFIG_DIR` | Board crate's `config/`       | ThreadX config (`tx_user.h`)   |
| `NETX_DIR`           | `third-party/threadx/netxduo`            | NetX Duo source                |
| `NETX_CONFIG_DIR`    | Board crate's `config/`       | NetX Duo config (`nx_user.h`)  |

## Building Examples

### Linux Simulation

```bash
just build-examples-threadx-linux
```

Builds all Linux simulation examples natively. The board crate's `build.rs`
compiles ThreadX kernel (Linux port), NetX Duo, and the Linux raw socket
network driver via the `cc` crate.

### QEMU RISC-V 64-bit

```bash
just build-examples-threadx-riscv64
```

Cross-compiles all QEMU RISC-V examples for `riscv64gc-unknown-none-elf`. The
board crate's `build.rs` compiles ThreadX kernel (RISC-V port), NetX Duo, and
the virtio-net NetX Duo driver.

### Available Examples

Examples are in `examples/threadx-linux/rust/zenoh/` and
`examples/qemu-riscv64-threadx/rust/zenoh/`:

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
just test-threadx          # Both Linux sim + QEMU RISC-V
just test-threadx-linux    # Linux simulation only
just test-threadx-riscv64  # QEMU RISC-V only
```

### Linux Simulation Tests

Linux simulation tests use TAP networking with AF_PACKET raw sockets. The
ThreadX Linux port runs the full kernel as pthreads on the host. Binaries
need `CAP_NET_RAW` capability:

```bash
just setup-threadx-caps    # Build + apply capabilities (one-time)
just test-threadx-linux    # Run tests
```

### QEMU RISC-V Tests

QEMU tests use TAP networking with virtio-net. Each QEMU instance connects
to the host via a TAP device, communicating through a bridge running zenohd:

```
┌─────────────────────────────────────────────────┐
│  Host (Linux)                                    │
│  zenohd (192.0.3.1)  QEMU talker  QEMU listener │
│       │                   │              │        │
│  ─────┴───────────────────┴──────────────┴────── │
│       br-qemu         tap-qemu0      tap-qemu1   │
└─────────────────────────────────────────────────┘
```

## Architecture

### Linux Simulation

```
User Application (Executor + Node + Pub/Sub)
        │
nros-node (Executor)
        │
nros-rmw-zenoh → zpico-sys (zenoh-pico + C shim)
        │                       │
        │          zenoh-pico POSIX platform
        │          (pthreads, BSD sockets)
        │
Board Crate (nros-threadx-linux)
├── ThreadX kernel (Linux port — tx_thread via pthreads)
├── NetX Duo (BSD sockets via AF_PACKET raw socket driver)
└── nx_linux_network_driver (from threadx-learn-samples)
```

### QEMU RISC-V 64-bit

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
├── ThreadX kernel (RISC-V port — real preemptive scheduling)
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
- **Two targets**: Linux simulation for fast iteration, QEMU RISC-V for real
  ThreadX scheduling validation.

## Safety Certifications

ThreadX holds the highest level of safety certifications:

- **IEC 61508 SIL 4** — functional safety for industrial systems
- **IEC 62304 Class C** — medical device software
- **ISO 26262 ASIL D** — automotive functional safety

NetX Duo is certified to the same IEC 61508 SIL 4 standard. Combined with
nano-ros's Kani/Verus formal verification, this creates a layered safety
argument for safety-critical deployments.
