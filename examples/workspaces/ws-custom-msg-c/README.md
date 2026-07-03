# ws-custom-msg-c — in-workspace custom message (C)

## What it shows

A workspace-local interface package `src/custom_msgs/` defining
`msg/Reading.msg` (`float64 temperature`, `float64 humidity`,
`int32 sequence`). `reading_talker` publishes it on `/reading`
(type `custom_msgs::msg::dds_::Reading_`, raw-CDR path); `reading_listener`
decodes and prints. Two entries (`native_talker_entry` /
`native_listener_entry`) — cross-process delivery (issue 0096: no same-process
loopback).

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_listener_entry/native_listener_entry &
./build/src/native_talker_entry/native_talker_entry
```

## Expected output

```
[reading_talker] sent seq=1 temp=21.5
reading seq=1 temp=21.5          # listener
```

## Copy-out notes

Standard workspace copy-out: `nros ws sync` regenerates the interface crates;
CMake resolves the nano-ros root via `-DNANO_ROS_ROOT` / `NROS_REPO_DIR`.
Fixture ids `workspace-c-native-custom-msg-{talker,listener}`.
