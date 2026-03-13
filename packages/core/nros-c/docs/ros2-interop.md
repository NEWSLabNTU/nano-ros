# ROS 2 Interoperability {#ros2_interop}

nros communicates with standard ROS 2 nodes via `rmw_zenoh_cpp`. Both
sides connect to the same zenohd router (or peer directly).

## Quick Start (3 Terminals)

```bash
# 1. Start zenoh router
zenohd --listen tcp/127.0.0.1:7447

# 2. Run the nros C talker (see Getting Started for build steps)
./my_c_talker

# 3. Run a ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

## Requirements

- **RMW implementation**: set `RMW_IMPLEMENTATION=rmw_zenoh_cpp` on
  the ROS 2 side.
- **QoS**: nros defaults to BEST_EFFORT reliability. Pass
  `--qos-reliability best_effort` on the ROS 2 subscriber, or configure
  both sides to use RELIABLE.
- **zenohd version**: both zenoh-pico (used by nros) and zenohd must be
  the same version. Version mismatches cause transport failures.

## Common Issues

- **Topic not visible** in `ros2 topic list` — nros must declare
  liveliness tokens (handled automatically by the transport layer).
  Verify zenohd is reachable from both sides.
- **No messages received** — check that the data key expression uses
  `TypeHashNotSupported` (Humble) not `RIHS01_…` in the data topic.
  This is handled automatically when using the `ros-humble` feature.
- **QoS mismatch** — nros defaults to BEST_EFFORT; pass
  `--qos-reliability best_effort` on the ROS 2 subscriber.
- **rmw_zenoh not connecting** — force client mode on the ROS 2 side:
  ```bash
  export ZENOH_CONFIG_OVERRIDE='mode="client";connect/endpoints=["tcp/127.0.0.1:7447"]'
  ```
- **Humble vs Iron** — Humble uses `TypeHashNotSupported`; Iron+ uses
  `RIHS01_<sha256>` (requires building with the `ros-iron` feature).
