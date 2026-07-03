# ws-params-c — parameterised node with live re-read (C)

## What it shows

The launch file bakes `<param name="publish_period_ms" value="250"/>` and
`system.toml` enables `[param_services]` (the six ROS 2 parameter services +
a volatile store). Each tick, `param_talker`
(`c_param_talker_pkg::Talker`) re-reads the **live** value via
`nros_cpp_get_param_integer(executor, "publish_period_ms", &live)` and
publishes it on `/chatter` (`std_msgs/Int32`) — so a runtime
`ros2 param set` visibly changes the published value.

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_entry/native_entry
```

Then: `ros2 param get /param_talker publish_period_ms`,
`ros2 param set /param_talker publish_period_ms 500`.

## Expected output

```
Published: 250
Published: 500        # after ros2 param set
```

## Copy-out notes

Standard workspace copy-out. Fixture id `workspace-c-native-params`.
