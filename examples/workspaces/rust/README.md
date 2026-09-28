# Rust Workspace

This workspace demonstrates the nano-ros Node / Bringup / Entry split with
pure Rust packages.

```text
rust/
├── .colcon_workspace     # the tracked marker that this dir IS a workspace root
└── src/
    ├── talker_pkg/       # Node pkg: publishes std_msgs/Int32 on /chatter
    ├── listener_pkg/     # Node pkg: subscribes std_msgs/Int32 on /chatter
    ├── demo_bringup/     # Bringup pkg: package.xml + system.toml + launch/,
    │   └── boards/       #   plus the per-board Kconfig each Zephyr image names
    └── zephyr_entry_robot1/   # this workspace's last hand-written west app
```

Every `[image.*]` here is generated except `zephyr_robot1`, and that one is
KEPT deliberately (phase-470 W5.b1). Its package name is not `<image>_entry`,
so `[image.zephyr_robot1] entry = "zephyr_entry_robot1"` is the only place in
the tree where that key CHANGES the answer: `cmd::build`'s discriminator checks
`entry` first and a hand-written `src/<id>_entry` second, and every other
surviving `entry =` names exactly `<image>_entry`, so deleting it there would
resolve the same application by the second rung. Nothing else — no unit test —
exercises the first rung. Migrating this one is safe only once something does.

`[image.zephyr]`'s
west application — `CMakeLists.txt`, `build.rs`, the staticlib manifest and
`src/lib.rs` — is emitted by `nros build` into `build/zephyr-zenoh/zephyr_entry/`
(phase-470 W5.a); its Kconfig is authored once, in
`src/demo_bringup/boards/native_sim_native_64/`, which is what `nros build`
passes as `APPLICATION_CONFIG_DIR`.

The Node packages use generated `std_msgs::msg::Int32` directly.

From the repository root:

```bash
source ./activate.sh
cd examples/workspaces/rust
nros setup native
nros sync            # once per workspace: generated message bindings
nros build native
```

`nros build` walks the packages, resolves the image, checks the
toolchain, generates what the build system needs, and hands off to
cargo/cmake/west — so compiler errors are the compiler's, unchanged
(RFC-0065). `nros build` with no image lists what this workspace
declares. The old `codegen-system` / `check` steps still work; they are
just no longer something you have to type — but `nros sync` still is.
A workspace that has never been synced has no generated message crates,
so `nros build` refuses at preflight and names `nros sync` as the remedy.

Run the native entry with a Zenoh router available:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd
nros build native && nros run native
```
