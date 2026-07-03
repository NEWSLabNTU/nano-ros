# ws-qos-cpp — per-entity QoS contract in code (C++)

## What it shows

The C++ projection of [`ws-qos-c`](../ws-qos-c/): both endpoints build the
same non-default profile with the fluent builder —

```cpp
auto qos = ::nros::QoS::default_profile().reliable().transient_local().keep_last(10);
node.create_publisher(pub_, "/chatter", qos);
```

— on `/chatter` (`std_msgs/Int32`). Matching profiles are required to
connect; transient-local delivers buffered history to late joiners.
Two entries, cross-process (issue 0096).

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

Standard workspace copy-out. Fixture ids
`workspace-cpp-native-qos-{talker,listener}`.
