# ThreadX

nano-ros runs on ThreadX (Azure RTOS) with NetX Duo networking. The primary
targets are a RISC-V 64-bit QEMU machine and a Linux simulation environment.

## Overview

The ThreadX platform uses:

- **ThreadX** (Azure RTOS) -- pre-emptive RTOS kernel with deterministic scheduling
- **NetX Duo** -- TCP/IP network stack with BSD socket compatibility layer
- **zenoh-pico** -- Zenoh transport over NetX Duo BSD sockets

Board crate: `nros-threadx-qemu-riscv64` (in `packages/boards/`). A Linux
simulation board crate (`nros-threadx-linux`) is also available for
host-side development.

## Safety Certifications

ThreadX holds the highest level of safety certifications across multiple
standards:

- **IEC 61508 SIL 4** -- functional safety for industrial systems
- **IEC 62304 Class C** -- medical device software
- **ISO 26262 ASIL D** -- automotive functional safety

These certifications make the ThreadX platform suitable for safety-critical
nano-ros deployments where regulatory compliance is required.

## TSN Support

NetX Duo provides Time-Sensitive Networking (TSN) capabilities, enabling
deterministic, low-latency communication for industrial and automotive use
cases. nano-ros can leverage TSN-aware scheduling when running on ThreadX with
NetX Duo on TSN-capable hardware.

## Building

```bash
just build-examples-threadx
```

This builds the ThreadX examples for both the RISC-V 64-bit QEMU target and
the Linux simulation.

## Testing

```bash
just test-threadx
```

Tests run under `qemu-system-riscv64` with TAP networking for the RISC-V
target. The Linux simulation examples run natively on the host.

## Example Targets

ThreadX examples live under two directories:

- `examples/threadx-qemu-riscv64/` -- cross-compiled for RISC-V 64-bit,
  runs under QEMU
- `examples/threadx-linux/` -- Linux simulation using ThreadX's POSIX
  port, useful for development and debugging without QEMU

Both targets support the same nano-ros API. The Linux simulation is the
fastest path to iterate on ThreadX-specific code.

## Status

ThreadX platform support is tracked in Phase 58. Items 58.1 through 58.7 are
complete, with remaining work in progress.
