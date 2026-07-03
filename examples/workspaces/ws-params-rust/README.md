# ws-params-rust — parameterised node: baked initial + live re-read (Rust)

## What it shows

All three parameter surfaces at once (`param_talker_pkg::ParamTalker`):

- **Baked initial** — `register()` reads `ctx.param("publish_period_ms")`
  (launch bakes 250) to set the timer rate; the rate stays fixed.
- **Live re-read** — the callback re-reads
  `ctx.parameter::<i64>("publish_period_ms")` each tick and publishes that
  value on `/chatter` (`std_msgs/Int32`).
- **Param services** — `[param_services]` in `system.toml` registers the six
  ROS 2 parameter services + volatile store, so `ros2 param get/set` works.

So after `ros2 param set`, the published *value* tracks the change while the
publish *rate* keeps the baked initial — showing which reads are baked vs live.

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cargo build -p native_entry
zenohd --listen tcp/127.0.0.1:7447 &
cargo run -p native_entry
```

## Expected observation (the node is silent — use ROS 2 tools)

```sh
ros2 topic echo /chatter                            # data: 250, 250, ...
ros2 param set /param_talker publish_period_ms 500  # -> data: 500 (same rate)
```

## Copy-out notes

Standard workspace copy-out. Fixture id `workspace-rust-native-params`.
