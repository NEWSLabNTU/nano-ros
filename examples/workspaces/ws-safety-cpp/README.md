# ws-safety-cpp — E2E-safety (CRC + sequence) demo (C++)

## What it shows

The C++ projection of [`ws-safety-c`](../ws-safety-c/): the talker publishes
`/chatter` (`std_msgs/Int32`) with the auto-attached CRC-32 + sequence
trailer; the listener uses the typed safety API —

```cpp
node.create_subscription_with_safety<std_msgs::msg::Int32>(...)
```

— whose handler receives the decoded message **plus** a
`nros_cpp_integrity_status_t` (CRC validity, gap, duplicate), republishing the
CRC-valid count on `/safe_ok`. Two entries, cross-process (issue 0096).

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build -DNANO_ROS_SAFETY_E2E=ON && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_safety_listener_entry/native_safety_listener_entry &
./build/src/native_safety_talker_entry/native_safety_talker_entry
```

## Expected output

```
[TALKER] Published: 1
[LISTENER] CRC ok — data=1 count=1 gap=0 dup=no
```

## Copy-out notes

Standard workspace copy-out. Fixture ids
`workspace-cpp-native-safety-{talker,listener}`. Rust sibling:
[`ws-safety-rust`](../ws-safety-rust/).
