# nros Zephyr Service Server (Rust)

A ROS 2 compatible service server running on Zephyr RTOS using nros.

## Overview

This example demonstrates:
- Service server using zenoh-pico queryable
- AddTwoInts service (from example_interfaces)
- Callback-based request handling
- CDR serialization for ROS 2 compatibility

## Architecture

```
Rust Application (src/lib.rs)
    └── nros-rmw-zenoh (Rust wrapper)
        └── zenoh_shim.c (C shim, compiled by Zephyr)
            └── zenoh-pico (C library)
                └── Zephyr network stack
```

## Prerequisites

1. Set up the Zephyr workspace (see main README)
2. Start zenoh router on the host loopback: `zenohd --listen tcp/127.0.0.1:7456`

## Build

```bash
source ~/nano-ros-workspace/env.sh
west build -b native_sim/native/64 nros/examples/zephyr/rust/zenoh/service-server
```

## Run

```bash
./build/zephyr/zephyr.exe
```

The service server will:
1. Connect to the zenoh router at tcp/127.0.0.1:7456 (via NSOS loopback)
2. Declare service server for `demo/add_two_ints`
3. Wait for and process service requests

## Testing

From another terminal, run the native service client:

```bash
cd nros/examples/native/rust/zenoh/service-client
cargo run --features zenoh
```

Or use a zenoh-based query:

```bash
# Using zenoh CLI tools
z_get -k "demo/add_two_ints" --payload "<CDR-encoded-request>"
```

## Network Configuration

`native_sim` uses NSOS (offloaded host sockets). No TAP bridge or `sudo` is
required — everything talks to the host on `127.0.0.1`.
