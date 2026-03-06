# POSIX (Linux / macOS)

The POSIX platform is the simplest way to develop with nano-ros. It runs
natively on Linux and macOS using OS-provided threads, sockets, and memory
allocation. No board crate or special hardware is needed.

## Features

Enable these Cargo features for a POSIX build:

```toml
[dependencies]
nros-node = { version = "0.1", features = ["std", "rmw-zenoh", "platform-posix"] }
```

- `std` -- use the standard library (threads, heap, I/O)
- `rmw-zenoh` -- Zenoh middleware backend (or `rmw-xrce` for XRCE-DDS)
- `platform-posix` -- use OS sockets and pthreads for transport

## Environment Variables

Configure the Zenoh connection at runtime:

| Variable        | Default               | Description                             |
|-----------------|-----------------------|-----------------------------------------|
| `ZENOH_LOCATOR` | `tcp/127.0.0.1:7447`  | Zenoh router address                    |
| `ZENOH_MODE`    | `client`              | Session mode (`client` or `peer`)       |
| `ROS_DOMAIN_ID` | `0`                   | ROS 2 domain ID for topic key prefixes  |

## Building and Running

POSIX examples live under `examples/native/`:

```bash
# Start a zenoh router
just build-zenohd
./build/zenohd/zenohd

# In another terminal, run a native example
cd examples/native/rust/zenoh/talker
cargo run
```

Or build all native examples at once:

```bash
just build-examples-native
```

## When to Use POSIX

POSIX is the right choice for:

- **Development and debugging** -- fast iteration on x86-64 before deploying
  to embedded targets.
- **Integration testing** -- `just test-integration` runs Rust tests against a
  local zenoh router using the POSIX platform.
- **ROS 2 interop testing** -- connect to a ROS 2 graph via `rmw_zenoh_cpp`
  on the same machine.
- **Linux-based embedded systems** -- Raspberry Pi, NVIDIA Jetson, or any
  Linux SBC where `std` is available.

For microcontroller targets, see the platform-specific chapters.
