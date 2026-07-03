# ws-lifecycle-cpp — two lifecycle flavors (C++)

## What it shows

TWO bringup/entry pairs demonstrating both lifecycle surfaces:

1. **Baked autostart** — `src/demo_bringup/` + `native_entry`:
   `[lifecycle] autostart = "active"` in `system.toml` makes the generated
   entry drive Configure → Activate for `LifecycleTalker`, which publishes
   `/chatter` (`std_msgs/Int32`).
2. **Self-managed wrapper** — `src/managed_bringup/` + `native_managed_entry`
   (no `[lifecycle]` block): `ManagedTalker` wraps `nros::LifecycleNode`,
   calls `register_services()` itself and autostarts to Active, printing
   its transitions; publishing is gated on the Active state.

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup       # or: --bringup managed_bringup
cmake -S . -B build && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_entry/native_entry            # or native_managed_entry
```

`ros2 lifecycle get /talker` → `active`.

## Expected output

```
Published: 1                  # demo_bringup flavor
LC:on_configure               # managed flavor adds transition lines
LC:on_activate
Published: 1
```

## Copy-out notes

Standard workspace copy-out. Fixture ids `workspace-cpp-native-lifecycle`
and `workspace-cpp-native-lifecycle-managed`.
