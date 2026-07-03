# ws-lifecycle-rust — managed (lifecycle) node with baked autostart (Rust)

## What it shows

`src/demo_bringup/system.toml` declares `[lifecycle] autostart = "active"`;
`nros::main!()` registers the five REP-2002 lifecycle services and drives
Configure → Activate at boot. The node (`talker_pkg::Talker`) publishes an
`std_msgs/Int32` counter on `/chatter` via
`ctx.publish_to_topic::<Int32, 8>("/chatter", …)` — it prints nothing itself.

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cargo build -p native_entry
zenohd --listen tcp/127.0.0.1:7447 &
cargo run -p native_entry
```

## Expected observation (via ROS 2 tools — the node is silent)

```sh
ros2 lifecycle get /talker      # -> active
ros2 topic echo /chatter        # -> data: 1, 2, ...
```

## Copy-out notes

Standard workspace copy-out (`nros ws sync` regenerates `generated/` crates +
the `# nros-managed` patch block). Fixture id
`workspace-rust-native-lifecycle`.
