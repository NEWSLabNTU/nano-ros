# Quick Reference

## Manual Testing

```bash
# Build zenohd first (one-time)
just build-zenohd

# Terminal 1: Router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Talker
cd examples/native/rust/zenoh/talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: Listener
cd examples/native/rust/zenoh/listener && RUST_LOG=info cargo run --features zenoh
```

## UDP Transport

On native/POSIX, zenoh-pico has built-in UDP support via OS sockets:

```bash
# Use UDP instead of TCP for the zenoh locator
ZENOH_LOCATOR=udp/127.0.0.1:7447 cargo run --features zenoh
```

On bare-metal, enable the `link-udp-unicast` feature to use UDP over smoltcp:

```toml
nros = { features = ["rmw-zenoh", "platform-bare-metal", "link-tcp", "link-udp-unicast"] }
```

## TLS Transport

TLS layers on top of TCP using mbedTLS. Requires a self-signed certificate (or real CA cert) and the `link-tls` Cargo feature.

**Generate a test certificate:**
```bash
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
  -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost"
```

**Native/POSIX** (requires `libmbedtls-dev` -- installed by `just setup`):

```bash
# Terminal 1: Router with TLS
./build/zenohd/zenohd --no-multicast-scouting --listen tls/localhost:7447 \
  --cfg 'transport/link/tls/listen_certificate:"cert.pem"' \
  --cfg 'transport/link/tls/listen_private_key:"key.pem"'

# Terminal 2: Talker with TLS
ZENOH_LOCATOR=tls/localhost:7447 \
  ZENOH_TLS_ROOT_CA_CERTIFICATE=cert.pem \
  cargo run -p native-rs-talker --features link-tls
```

**Bare-metal** (QEMU ARM):

On bare-metal, only base64-encoded certificates are supported (no filesystem).
Build with `--features link-tls`:

```bash
# Build TLS-enabled examples
cd examples/qemu-arm-baremetal/rust/zenoh/talker
cargo build --release --features link-tls
```

The CA certificate must be passed via `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` at runtime
(through the zenoh config), or embedded in the binary at build time.

## ROS 2 Interop

```bash
# Terminal 1: Router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: nros talker
cd examples/native/rust/zenoh/talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

## Actions

```bash
# Terminal 1: Router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Action server (Fibonacci example)
cd examples/native/rust/zenoh/action-server && cargo run

# Terminal 3: Action client
cd examples/native/rust/zenoh/action-client && cargo run
```

**Zephyr action tests:**
```bash
just build-zephyr-actions      # Build server and client
just test-rust-zephyr-actions  # Run E2E tests (requires TAP setup)
```

## Docker Development Environment

Docker provides QEMU 7.2 (from Debian bookworm) which fixes TAP networking issues present in Ubuntu 22.04's QEMU 6.2.

```bash
# One-time setup: add yourself to docker group
sudo usermod -aG docker $USER
# Log out and back in, or run: newgrp docker

# Build and use Docker environment
just docker-build              # Build nano-ros-qemu image
just docker-shell              # Interactive shell
just docker-test-qemu          # Run QEMU tests in container
just docker-help               # Show all Docker commands
```

## QEMU Bare-Metal Testing

Run bare-metal Cortex-M3 examples on QEMU (MPS2-AN385 machine with LAN9118 Ethernet).

```bash
# Build prerequisites
just build-zenoh-pico-arm     # Build zenoh-pico for ARM Cortex-M3
just build-examples-qemu      # Build all QEMU examples

# Non-networked tests (no setup required)
just test-qemu-basic          # Run serialization test
just test-qemu-lan9118        # Run Ethernet driver test

# Networked talker/listener test (Docker Compose - recommended)
just docker-qemu-test         # Runs zenohd, talker, listener in separate containers
```

**Docker Compose Architecture:**
```
┌─────────────────────────────────────────────────────────────┐
│              Docker Network: 172.20.0.0/24                  │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   zenohd    │  │   talker    │  │      listener       │  │
│  │ 172.20.0.2  │  │ 172.20.0.10 │  │    172.20.0.11      │  │
│  │             │  │  ┌───────┐  │  │  ┌───────────────┐  │  │
│  │             │  │  │ QEMU  │  │  │  │     QEMU      │  │  │
│  │             │  │  │ ARM   │──┼──┼──│     ARM       │  │  │
│  │             │  │  │ TAP   │  │  │  │     TAP       │  │  │
│  │             │  │  └───────┘  │  │  └───────────────┘  │  │
│  └──────▲──────┘  └──────┼──────┘  └─────────┼───────────┘  │
│         └────────────────┴───────────────────┘              │
│                    NAT to zenohd                            │
└─────────────────────────────────────────────────────────────┘
```

**Manual networked test (3 terminals, requires host TAP setup):**
```bash
# Terminal 1: Setup network + start router
just setup-qemu-network                    # Requires sudo
./build/zenohd/zenohd --listen tcp/0.0.0.0:7447

# Terminal 2: Talker (192.0.2.10)
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 \
    --binary examples/qemu-arm-baremetal/rust/zenoh/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker

# Terminal 3: Listener (192.0.2.11)
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 \
    --binary examples/qemu-arm-baremetal/rust/zenoh/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener
```

Run `just qemu-help` for more options.

## Zephyr Setup

```bash
./scripts/zephyr/setup.sh              # Initialize workspace + create symlink
sudo ./scripts/zephyr/setup-network.sh # Configure bridge network (zeth-br)
just test-zephyr                       # Run zenoh tests
just test-zephyr-xrce                  # Run XRCE tests
```

The `zephyr-workspace` symlink points to the actual workspace (default: `../nano-ros-workspace/`). For custom workspace locations, update the symlink:
```bash
ln -sfn /path/to/custom-workspace zephyr-workspace
```

See [Zephyr](../platforms/zephyr.md) for full details.
