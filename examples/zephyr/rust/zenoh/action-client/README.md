# nros Zephyr Action Client Example (Rust)

A ROS 2 compatible action client running on Zephyr RTOS using nros.
Sends Fibonacci action goals and receives feedback as the sequence is computed.

## Architecture

```
Rust Application (src/lib.rs)
    └── nros-rmw-zenoh (Rust wrapper)
        └── zenoh_shim.c (C shim, compiled by Zephyr)
            └── zenoh-pico (C library)
                └── Zephyr network stack
```

## Action Channels Used

| Channel | Type | Purpose |
|---------|------|---------|
| `demo/fibonacci/_action/send_goal` | Query | Submit goal and get acceptance |
| `demo/fibonacci/_action/feedback` | Subscriber | Receive progress updates |

## Prerequisites

1. Zephyr workspace set up with nros module
2. zenoh router running on host
3. TAP interface configured for Zephyr networking
4. Action server running (native or Zephyr)

## Build

```bash
# Source Zephyr environment
source ~/nano-ros-workspace/env.sh

# Build for native_sim
west build -b native_sim/native/64 nros/examples/zephyr/rust/zenoh/action-client
```

## Run

```bash
# Terminal 1: Set up TAP interface (one-time)
sudo ./nros/scripts/setup-zephyr-network.sh

# Terminal 2: Start zenoh router
zenohd --listen tcp/0.0.0.0:7447

# Terminal 3: Run action server
cargo run -p native-rs-action-server --features zenoh

# Terminal 4: Run Zephyr action client
./build/zephyr/zephyr.exe
```

## Testing

The client will:
1. Connect to zenoh router
2. Send a Fibonacci goal (order=10)
3. Receive feedback messages as the sequence is computed
4. Report when the action completes

Expected output:
```
[00:00:03.000,000] <inf> rustapp: nros Zephyr Action Client Starting
[00:00:03.000,000] <inf> rustapp: Action: Fibonacci
[00:00:03.100,000] <inf> rustapp: Session opened
[00:00:06.000,000] <inf> rustapp: Sending goal: order=10
[00:00:06.200,000] <inf> rustapp: Goal accepted!
[00:00:06.700,000] <inf> rustapp: Feedback #1: [0]
[00:00:07.200,000] <inf> rustapp: Feedback #2: [0, 1]
[00:00:07.700,000] <inf> rustapp: Feedback #3: [0, 1, 1]
...
[00:00:11.700,000] <inf> rustapp: Received all feedback, action completed!
[00:00:11.700,000] <inf> rustapp: Final sequence: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
```

## Memory Requirements

- Main stack: 16KB
- Heap: 64KB
- Network buffers: ~8KB

## License

Apache-2.0
