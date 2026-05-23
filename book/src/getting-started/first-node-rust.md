# First Node — Rust (Linux)

Build, run, and verify a single nano-ros publisher node on Linux in
about ten minutes. Uses the canonical Zenoh backend; no router needs
to be pre-installed (the repo ships `zenohd`).

> **Stuck?** See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md) for the common first-build errors.
>
> **Prereqs.** A clone with `just setup base` already run, followed by
> `source ./setup.bash`.
> See [Install + first build (Linux)](./installation.md) if you
> haven't.

## Project layout

The talker is a **standalone Cargo package** that pulls nano-ros in
via a path dependency. Three files matter:

```text
examples/native/rust/zenoh/talker/
├── Cargo.toml          # path dep on `nros` + `nros-rmw-zenoh`
├── package.xml         # ROS-style manifest (drives codegen tooling)
├── generated/          # auto-generated message bindings (gitignored)
└── src/
    └── main.rs         # 60-line talker
```

POSIX talkers read the locator / domain from environment variables
(`ZENOH_LOCATOR`, `ROS_DOMAIN_ID`) — no `config.toml` is needed.
The `config.toml` shape used by embedded targets shows up under
the Embedded Starters section.

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

Three terminals (each command below blocks; keep them open):

```bash
# Each terminal:  source ./setup.bash    (first command after cd-ing in)

# Terminal 1 — in-tree zenoh router. Blocks the shell until Ctrl-C.
zenohd                               # provided by setup.bash; equivalent
                                     # to `just zenohd run` or
                                     # ./build/zenohd/zenohd directly

# Terminal 2 — the talker.
cd examples/native/rust/zenoh/talker
cargo run
# Expected output:
#   nros Native Talker (Zenoh Transport)
#   Node created: talker
#   Publisher created for topic: /chatter
#   Published: 1
#   Published: 2
#   …
```

That's the nano-ros side fully working. **Optional step:** verify
interop with stock ROS 2.

```bash
# Terminal 3 — stock ROS 2 with rmw_zenoh_cpp. NOTE: rmw_zenoh_cpp
# uses its OWN router daemon (`ros2 run rmw_zenoh_cpp rmw_zenohd`),
# NOT the in-tree zenohd from terminal 1. They need to peer with
# each other, or both clients need to point at the same router.
# Simplest: stop terminal 1 and run only `rmw_zenohd` instead, then
# launch the talker pointing at rmw_zenohd's port (default 7447).
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 run rmw_zenoh_cpp rmw_zenohd &       # in its own subshell
ros2 topic echo /chatter std_msgs/msg/Int32
```

If `ros2 topic echo` shows no output despite the talker printing
`Published:`, the routers aren't peering — confirm both processes
point at the same port (default `tcp/127.0.0.1:7447`).

**Readiness signal.** Within 5 seconds of `cargo run`, the talker
should print `Published: 1`. If no `Published:` line in 30 seconds:

1. Confirm `zenohd` is running (terminal 1). Without it, the talker
   blocks on `Executor::open` indefinitely.
2. Re-run with `RUST_LOG=debug cargo run` and look for "Failed to
   open session" — usually a wrong locator or wrong port.
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

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
