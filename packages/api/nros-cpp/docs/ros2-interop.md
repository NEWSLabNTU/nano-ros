# ROS 2 Interoperability {#ros2_interop}

nros C++ communicates with standard ROS 2 nodes via `rmw_zenoh_cpp`.
Both sides connect to the same zenohd router (or peer directly).

## Quick Start (3 Terminals)

```bash
# 1. Start zenoh router
zenohd --listen tcp/127.0.0.1:7447

# 2. Run the nros C++ talker (see Getting Started for build steps)
./my_cpp_talker

# 3. Run a ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

## Requirements

- **RMW implementation**: set `RMW_IMPLEMENTATION=rmw_zenoh_cpp` on the
  ROS 2 side.
- **QoS**: nros defaults to BEST_EFFORT reliability. Pass
  `--qos-reliability best_effort` on the ROS 2 subscriber, or set both
  sides to RELIABLE.
- **zenohd version**: zenoh-pico (used by nros) and zenohd must match
  versions exactly. Mismatches cause silent transport failures.
- **Type hash**: Humble uses `TypeHashNotSupported`; Iron+ uses
  `RIHS01_<sha256>`. Build with the matching `ros-humble` /
  `ros-iron` feature.

## Common Issues

- **Topic not visible** in `ros2 topic list` — verify zenohd is
  reachable from both sides; nros declares liveliness tokens
  automatically.
- **No messages received** — check that the data key expression uses
  the correct type-hash variant for your ROS 2 distro.
- **QoS mismatch** — pass `--qos-reliability best_effort` on the ROS 2
  subscriber, or set both sides to RELIABLE explicitly.
- **rmw_zenoh not connecting** — force client mode on the ROS 2 side:
  ```bash
  export ZENOH_CONFIG_OVERRIDE='mode="client";connect/endpoints=["tcp/127.0.0.1:7447"]'
  ```

## See Also

- @ref ros2_interop on the C-API side (same protocol, same router)
- `docs/reference/rmw_zenoh_interop.md` in the nano-ros source tree
  for the full key-expression and protocol reference
