# ws-qos-c — per-entity QoS contract in code (C)

## What it shows

Both endpoints declare an identical **non-default QoS profile in code**
(reliable + transient-local + keep-last-10, built in `qos_profile()` with
`NROS_C_QOS_RELIABLE` etc.) on `/chatter` (`std_msgs/Int32`). The profiles
must match to connect; transient-local durability means a late-joining
subscriber still receives buffered history. Components `qos_talker` /
`qos_listener`, two entries, cross-process (issue 0096).

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
Published: 1        # talker
Received: 1         # listener
```

## Copy-out notes

Standard workspace copy-out. QoS here is a code-level contract — the planner's
baked `qos_overrides` launch table is a separate surface. Fixture ids
`workspace-c-native-qos-{talker,listener}`.
