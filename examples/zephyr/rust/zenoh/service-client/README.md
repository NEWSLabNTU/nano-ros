# nros Zephyr Service Client (Rust)

A ROS 2 compatible service client running on Zephyr RTOS using nros.

## Overview

This example demonstrates:
- Service client using zenoh-pico query (z_get)
- AddTwoInts service (from example_interfaces)
- Blocking request/response calls
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
3. Start a service server (see below)

## Build

```bash
source ~/nano-ros-workspace/env.sh
west build -b native_sim/native/64 nros/examples/zephyr/rust/zenoh/service-client
```

## Run

First, start a service server:

```bash
# Option 1: Native service server
cd nros/examples/native/rust/zenoh/service-server
cargo run --features zenoh

# Option 2: Zephyr service server (in another terminal)
west build -b native_sim/native/64 nros/examples/zephyr/rust/zenoh/service-server
./build/zephyr/zephyr.exe
```

Then run the client:

```bash
./build/zephyr/zephyr.exe
```

The service client will:
1. Connect to the zenoh router at tcp/127.0.0.1:7456 (via NSOS loopback)
2. Send AddTwoInts requests every 2 seconds
3. Print the responses

## Network Configuration

`native_sim` uses NSOS (offloaded host sockets). No TAP bridge or `sudo` is
required — everything talks to the host on `127.0.0.1`.

## Notes

- The client uses a 5-second timeout for service calls
- If no server is running, the client will report timeout errors
- Both client and server must be connected to the same zenoh router
