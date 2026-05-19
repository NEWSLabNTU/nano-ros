# First Node — Rust (Linux)

Build, run, and verify a single nano-ros publisher node on Linux in
about ten minutes. Uses the canonical Zenoh backend; no router needs
to be pre-installed (the repo ships `zenohd`).

> **Prereqs.** A clone with `just setup tier=default` already run.
> See [Install + first build (Linux)](./installation.md) if you
> haven't.

## Project layout

The talker is a **standalone Cargo package** that pulls nano-ros in
via a path dependency. Three files matter:

```text
examples/native/rust/zenoh/talker/
├── Cargo.toml          # path dep on `nros` + `nros-rmw-zenoh`
├── package.xml         # ROS-style manifest (drives codegen tooling)
├── config.toml         # runtime locator + domain id (optional)
└── src/
    └── main.rs         # 60-line talker
```

The `Cargo.toml` is the contract that wires nano-ros into your
package:

```toml
[package]
name    = "my-talker"
edition = "2024"

[[bin]]
name = "talker"
path = "src/main.rs"

[dependencies]
nros = { path = "<...>/packages/core/nros",
         default-features = false,
         features = ["std", "rmw-cffi", "platform-posix"] }
nros-rmw-zenoh = { path = "<...>/packages/zpico/nros-rmw-zenoh",
                   features = ["std", "platform-posix", "ros-humble"] }

[workspace]      # empty table — this package is intentionally
                 # standalone, no walking-up workspace.
```

## Configure

Three runtime knobs, each overridable at three layers (defaults →
`config.toml` → env vars):

| Knob | Default | Env override |
|---|---|---|
| Zenoh locator | `tcp/127.0.0.1:7447` | `ZENOH_LOCATOR` |
| ROS domain ID | `0` | `ROS_DOMAIN_ID` |
| Zenoh mode | client | `ZENOH_MODE` |

`config.toml` (optional, alongside `Cargo.toml`):

```toml
[zenoh]
locator   = "tcp/127.0.0.1:7447"
domain_id = 0
```

## Build

```bash
cd examples/native/rust/zenoh/talker
cargo build           # or: cargo build --release
```

First build pulls dependencies (~3 minutes). Re-builds finish in
seconds.

## Run

Three terminals.

```bash
# 1. Start the in-tree zenoh router (already built by `just setup`):
just zenohd                          # or: ./build/zenohd/zenohd

# 2. Run the talker:
cd examples/native/rust/zenoh/talker
cargo run
# Expected output:
#   nros Native Talker (Zenoh Transport)
#   Node created: talker
#   Publisher created for topic: /chatter
#   Published: 1
#   Published: 2
#   …

# 3. Verify from a stock ROS 2 install (any other terminal):
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

You should see the same counter values arriving on the ROS 2 side.

## GitHub source

Canonical, copy-out:
[`examples/native/rust/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/rust/zenoh/talker)

Copy the directory, rename the package, and your starter is ready to
modify.

## Next

- Add a subscription:
  [`examples/native/rust/zenoh/listener/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/rust/zenoh/listener)
- Generate bindings for custom `.msg` / `.srv` / `.action` files:
  [Message Generation](../user-guide/message-generation.md)
- Cross-compile for an RTOS: pick the right Embedded Starter from
  the next section (FreeRTOS / Zephyr / NuttX / ThreadX / ESP32 /
  Bare-metal Cortex-M3).
