# ws-params-cpp — parameterised node with live re-read (C++)

## What it shows

The C++ projection of [`ws-params-c`](../ws-params-c/): launch bakes
`publish_period_ms = 250`, `[param_services]` exposes the ROS 2 parameter
services, and `ParamTalker` saves `node.executor_handle()` at configure to
re-read the **live** value each tick via `nros_cpp_get_param_integer(…)`,
publishing it on `/chatter` (`std_msgs/Int32`).

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_entry/native_entry
```

Then: `ros2 param set /param_talker publish_period_ms 500`.

## Expected output

```
Published: 250
Published: 500        # after ros2 param set
```

## Copy-out notes

Standard workspace copy-out. Fixture id `workspace-cpp-native-params`.
