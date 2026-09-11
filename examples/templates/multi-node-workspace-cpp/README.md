# `multi-node-workspace-cpp` — the canonical 3-role template (C++)

The C++ sibling of [`multi-node-workspace`](../multi-node-workspace/) (whose
README documents the role split and the ROS 2 ↔ nano-ros mapping in full):
the same Node pkg / Bringup pkg shape with a generated entry, built with CMake.

## Layout

- `src/talker_pkg/`, `src/listener_pkg/` — typed components (RFC-0043):
  `Result configure(rclcpp::Node&)`; the talker binds a 500 ms `create_wall_timer<Talker, &Talker::on_tick>`
  publishing `std_msgs/Int32` on `/chatter`; the listener uses
  `bind_subscription` (typed member callback on the generated `std_msgs::msg::Int32`).
- `src/demo_bringup/` — `package.xml` + `system.toml` + `launch/`
  (the launch XML also shows a per-topic QoS override:
  `qos_overrides./chatter.publisher.reliability = best_effort`).
There is no root `CMakeLists.txt` and no entry package (RFC-0098 D9, RFC-0065
D4): `nros build` generates the cmake root and the image's C++ entry
(`native_entry`) under `build/`, resolving the bringup's launch file into a
SystemModel there (SystemModels are build artifacts — never committed).

## Build

```sh
export NROS_REPO_DIR=/path/to/nano-ros
nros sync
nros build native
```

## Run

```sh
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd &
./build/posix-native/cmake/native_entry
```

## Expected output

```
Published: 1
Received: 1
Published: 2
...
```
