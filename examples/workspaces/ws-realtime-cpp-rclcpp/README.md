# ws-realtime-cpp-rclcpp — the two-tier demo in rclcpp shape (RFC-0047)

The same 2-node / 2-tier realtime demo as
[`ws-realtime-cpp`](../ws-realtime-cpp/), but the components are
**`::nros::ComponentNode` subclasses** — the IS-A-node shape familiar from
`rclcpp::Node` — instead of configure-shape components.

## What it shows

Instead of `Result configure(nros::Node&)`, `Ctrl`/`Telem` wire everything in
the constructor:

```cpp
Ctrl::Ctrl(::nros::NodeHandle h) : ComponentNode(h, "ctrl_node") {
    create_publisher<std_msgs::msg::Int32>("/ctrl");
    NROS_CREATE_TIMER(10, on_tick);
}
```

Tiers and bindings are identical to the base (`[tiers.high]` 10 ms / prio 80,
`[tiers.low]` 100 ms / prio 10; `ctrl` → high, `telem` → low in
`src/demo_bringup/system.toml`).

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_entry/native_entry
```

(Fixture id `workspace-cpp-native-realtime-rclcpp`; e2e
`realtime_tiers_cpp_rclcpp_e2e`.)

## Expected output

```
[rclcpp_ctrl] tick=N      # ~10 per one
[rclcpp_telem] tick=N
```
