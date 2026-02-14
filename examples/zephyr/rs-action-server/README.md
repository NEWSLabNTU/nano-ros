# nros Zephyr Action Server Example (Rust)

A ROS 2 compatible action server running on Zephyr RTOS using nros.
Implements the Fibonacci action - computing Fibonacci sequences with progress feedback.

## Architecture

```
Rust Application (src/lib.rs)
    └── nros-rmw-zenoh (Rust wrapper)
        └── zenoh_shim.c (C shim, compiled by Zephyr)
            └── zenoh-pico (C library)
                └── Zephyr network stack
```

## Action Channels

The Fibonacci action server uses 5 communication channels:

| Channel | Type | Purpose |
|---------|------|---------|
| `demo/fibonacci/_action/send_goal` | Queryable | Accept goal requests |
| `demo/fibonacci/_action/cancel_goal` | Queryable | Handle cancellation |
| `demo/fibonacci/_action/get_result` | Queryable | Return results |
| `demo/fibonacci/_action/feedback` | Publisher | Send progress updates |
| `demo/fibonacci/_action/status` | Publisher | Send status updates |

## Prerequisites

1. Zephyr workspace set up with nros module
2. zenoh router running on host
3. TAP interface configured for Zephyr networking

## Build

```bash
# Source Zephyr environment
source ~/nano-ros-workspace/env.sh

# Build for native_sim
west build -b native_sim/native/64 nros/examples/zephyr-rs-action-server
```

## Run

```bash
# Terminal 1: Set up TAP interface (one-time)
sudo ./nros/scripts/setup-zephyr-network.sh

# Terminal 2: Start zenoh router
zenohd --listen tcp/0.0.0.0:7447

# Terminal 3: Run Zephyr action server
./build/zephyr/zephyr.exe

# Terminal 4: Run action client (native or Zephyr)
cargo run -p native-rs-action-client --features zenoh
```

## Testing

The server will:
1. Accept Fibonacci goals
2. Compute the sequence step by step
3. Send feedback after each computation (500ms interval)
4. Complete the goal with the final sequence

Expected output:
```
[00:00:03.000,000] <inf> rustapp: nros Zephyr Action Server Starting
[00:00:03.000,000] <inf> rustapp: Action: Fibonacci
[00:00:03.100,000] <inf> rustapp: Session opened
[00:00:03.200,000] <inf> rustapp: Action server ready: /demo/fibonacci
[00:00:05.000,000] <inf> rustapp: Goal request: order=10
[00:00:05.100,000] <inf> rustapp: Goal accepted (slot 0)
[00:00:05.200,000] <inf> rustapp: Executing goal: order=10
[00:00:05.700,000] <inf> rustapp: Feedback: [0]
[00:00:06.200,000] <inf> rustapp: Feedback: [0, 1]
...
```

## Memory Requirements

- Main stack: 16KB
- Heap: 64KB
- Network buffers: ~8KB

## License

Apache-2.0
