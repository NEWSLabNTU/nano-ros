# Docker Test Environment

Run integration tests inside Docker containers without any host-side
network configuration (no `sudo`, no `setcap`, no manual veth setup).

## Quick Start

```bash
just docker-build                          # Build the image (once)
just docker-test test-freertos             # FreeRTOS QEMU E2E tests
just docker-test test                      # All non-privileged tests
just docker-test test-threadx-linux        # ThreadX Linux (see note below)
```

## How It Works

The `docker-test` recipe runs a container with:

1. **Root phase** (entrypoint): creates veth pairs, bridges, TAP devices
2. **User phase** (tests): drops to `HOST_UID:HOST_GID` via `capsh` with
   ambient `CAP_NET_RAW` + `CAP_NET_ADMIN` capabilities

Tests run as the host user so build artifacts on the bind-mounted workspace
have correct ownership. Ambient capabilities let ThreadX Linux binaries
open raw AF_PACKET sockets without `setcap` (which doesn't work on
Docker bind-mounted volumes).

## Container Capabilities

| Capability | Purpose |
|------------|---------|
| `CAP_NET_ADMIN` | Create veth pairs, bridges, set IP addresses |
| `CAP_NET_RAW` | ThreadX Linux AF_PACKET raw sockets |

Additionally `seccomp:unconfined` is set because the ThreadX Linux
network driver uses a userspace TCP/IP stack over raw Ethernet frames.

## Known Limitations

**ThreadX Linux E2E tests**: The NetX Duo userspace TCP/IP stack has
connectivity issues inside Docker containers. The raw socket opens
successfully but TCP handshakes to zenohd via the veth→bridge path
fail with `Transport(ConnectionFailed)`. This is under investigation.
ThreadX Linux E2E tests work on the host with `just setup-network`
and `just setup-threadx-caps`.

## Image Contents

- Debian bookworm-slim
- QEMU (ARM + RISC-V)
- GCC cross-compilers (ARM, RISC-V)
- Rust stable + nightly (with rust-src for build-std)
- cargo-nextest, just
- zenohd 1.7.2 (project-pinned version)
- kconfig-frontends (for NuttX kernel build)
- Networking tools (iproute2, tcpdump, etc.)
