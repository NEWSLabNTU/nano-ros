# ws-lifecycle-c — managed (lifecycle) node with baked autostart (C)

## What it shows

`src/demo_bringup/system.toml` declares `[lifecycle] autostart = "active"`;
the generated entry emits `nros_cpp_lifecycle_autostart(...)`, which registers
the five REP-2002 lifecycle services for the node and drives
Configure → Activate at boot. The node itself
(`c_lifecycle_talker_pkg::Talker`) just publishes a monotonic counter on
`/chatter` (`std_msgs/Int32`) every second.

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_entry/native_entry
```

Inspect from ROS 2: `ros2 lifecycle get /talker` → `active`
(e2e `cpp_c_lifecycle_autostart_e2e`).

## Expected output

```
Published: 1
Published: 2
```

## Copy-out notes

Standard workspace copy-out. Fixture id `workspace-c-native-lifecycle`.
