# Rust Workspace

This workspace demonstrates the nano-ros Node / Bringup / Entry split with
pure Rust packages.

```text
rust/
├── Cargo.toml
└── src/
    ├── talker_pkg/      # Node pkg: publishes std_msgs/Int32 on /chatter
    ├── listener_pkg/    # Node pkg: subscribes std_msgs/Int32 on /chatter
    ├── demo_bringup/           # Bringup pkg: package.xml + system.toml + launch/
    └── native_entry/           # Entry pkg: native main()
```

The Node packages use generated `std_msgs::msg::Int32` directly.

From the repository root:

```bash
source ./activate.sh
cd examples/workspaces/rust
nros setup native
nros ws sync
nros codegen-system --bringup demo_bringup
nros check --bringup src/demo_bringup
nros check --workspace .
cargo build -p native_entry
```

Run the native entry with a Zenoh router available:

```bash
zenohd --listen tcp/127.0.0.1:7447
cargo run -p native_entry
```
