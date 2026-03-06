# NuttX

nano-ros runs on NuttX, targeting QEMU Cortex-A7 (the `sabre-lite` machine).
NuttX provides POSIX-compatible BSD sockets, which makes it the most
straightforward RTOS port -- zenoh-pico uses the same socket API as on Linux.

## Overview

The NuttX platform uses:

- **NuttX RTOS** -- POSIX-compliant real-time OS with BSD socket support
- **BSD sockets** -- standard socket API provided by NuttX's network stack
- **zenoh-pico** -- Zenoh transport over NuttX sockets (same code path as POSIX)

Board crate: `nros-nuttx-qemu-arm` (in `packages/boards/`).

## Setup

Download the NuttX source and apps:

```bash
just setup-nuttx
```

This places the sources in `external/nuttx/` and `external/nuttx-apps/`.
Override the paths with the `NUTTX_DIR` environment variable if your sources
are elsewhere.

## Building

```bash
just build-examples-nuttx
```

This configures NuttX for the `sabre-6sx:nsh` board profile, builds the NuttX
kernel with networking enabled, and cross-compiles the nano-ros examples as
NuttX applications.

## Testing

```bash
just test-nuttx
```

Tests run under `qemu-system-arm` with the Cortex-A7 sabre-lite machine. TAP
networking connects the NuttX guest to a host bridge for zenohd communication.

## Why NuttX

NuttX's main advantage for nano-ros is its POSIX compatibility. Because NuttX
provides standard `socket()`, `connect()`, `send()`, and `recv()` calls,
zenoh-pico's POSIX transport layer works without modification. This avoids the
need for a custom network integration layer (unlike FreeRTOS/lwIP or
ThreadX/NetX Duo).

## Status

NuttX platform support is tracked in Phase 55. Items 55.1 through 55.10 and
55.12 are complete. Item 55.11 (remaining work) is in progress.
