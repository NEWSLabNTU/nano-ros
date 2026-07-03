# ws-safety-c — E2E-safety (CRC + sequence) demo (C)

## What it shows

`system.toml` declares `features = ["safety"]`: the talker's zenoh backend
auto-attaches a CRC-32 + sequence trailer to every `/chatter`
(`std_msgs/Int32`) sample; the listener registers a **validated**
subscription (`nros_cpp_subscription_register_validated`) that reports CRC
validity, sequence gaps and duplicates per message, republishing the
CRC-valid count on `/safe_ok`. Components `talker` + `safe_listener`, two
entries, cross-process (issue 0096).

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build -DNANO_ROS_SAFETY_E2E=ON && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_safety_listener_entry/native_safety_listener_entry &
./build/src/native_safety_talker_entry/native_safety_talker_entry
```

(`NANO_ROS_SAFETY_E2E=ON` is what the fixtures pass; the `features =
["safety"]` lowering into the entry codegen is issue #118.)

## Expected output

```
[TALKER] Published: 1
[LISTENER] CRC ok — count=1 gap=0 dup=no
```

## Copy-out notes

Standard workspace copy-out. Fixture ids
`workspace-c-native-safety-{talker,listener}`; e2e
`cpp_c_safety_integrity_e2e`.
