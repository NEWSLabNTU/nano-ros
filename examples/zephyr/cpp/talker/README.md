# Zephyr C++ talker — typed component (RFC-0043)

The node is a stateful **component** rather than a hand-written imperative
`main.cpp`: `Talker::configure(node)` binds a `std_msgs/String` publisher on
`/chatter` plus a 500 ms `create_wall_timer<Talker, &Talker::on_tick>` member callback that publishes
`Hello World: N` (the official ROS 2 demo chatter). The **Zephyr typed
carrier** generates the entry:

- `nano_ros_add_node(TYPED … DEPLOY zephyr)` (with
  `NROS_PLATFORM=zephyr`) `configure_file`s
  the shared entry pack (the CLI's entry-node generator, which cmake calls) — a plain `int main(void)`
  that constructs the component, calls `configure(node)`, and runs
  `::rclcpp::board::ZephyrBoard::run_components(&setup)`.
- The carrier branch composes the generated entry + the component lib into the
  global `find_package(Zephyr)` `app` target
  (`target_sources`/`target_link_libraries`) — no second executable.

## Build

A single-node workspace: the node package is `src/zephyr_cpp_talker`, and
`src/zephyr_cpp_talker_bringup` holds the `system.toml` that says what to build. `nros build`
generates the west application from it.

```bash
cp -r examples/zephyr/cpp/talker ~/my-talker && cd ~/my-talker
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build zephyr_cpp_talker_bringup:zephyr_zenoh         # or zephyr_xrce, zephyr_cyclonedds
```

Each `[image.*]` is one build: `zephyr_<rmw>` targets `native_sim/native/64`
and `mps2_zenoh` targets `mps2/an385`. From the checkout,
`just zephyr build-one cpp/talker zenoh` does the same.

## Config

Board, RMW, domain and locator: `src/zephyr_cpp_talker_bringup/system.toml`
(`[image.*]` + `[system]`, RFC-0098 D3/D5). The base Zephyr `prj.conf` of each
board is `boards/<board>/prj.conf` in the bringup, and each image's
`os_config` names its RMW fragments. Switching board is choosing another
image; no build command names a board.

## History

This dir absorbed the former `examples/zephyr/cpp/talker-typed` (phase-240.8
typed-carrier draft): both were the same TYPED component modulo namespace
after phase-244.C2 converted this example to the component shape, so the
duplicate was removed in phase-277 W7.
