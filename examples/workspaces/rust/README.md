# Rust Workspace

This workspace demonstrates the nano-ros Node / Bringup / Entry split with
pure Rust packages.

```text
rust/
├── .colcon_workspace     # the tracked marker that this dir IS a workspace root
└── src/
    ├── talker_pkg/       # Node pkg: publishes std_msgs/Int32 on /chatter
    ├── listener_pkg/     # Node pkg: subscribes std_msgs/Int32 on /chatter
    └── demo_bringup/     # Bringup pkg: package.xml + system.toml + launch/,
        └── boards/       #   plus the per-board Kconfig each Zephyr image names
```

Every `[image.*]` here is generated — `zephyr_robot1` too, since phase-477 W1
(issue 1288). It was kept hand-written for a while as the only evidence that an
explicit `entry =` key outranks the `src/<id>_entry` rung of `cmd::build`'s
discriminator; that precedence is now a unit test there
(`an_explicit_entry_outranks_the_id_entry_package`), so the leaf went.

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
