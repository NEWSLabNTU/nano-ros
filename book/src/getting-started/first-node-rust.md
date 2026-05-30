# First Node — Rust (Linux)

Build, run, and verify a single nano-ros publisher node on Linux in
about ten minutes. Uses the canonical Zenoh backend; no router needs
to be pre-installed (the repo ships `zenohd`).

> **Stuck?** See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md) for the common first-build errors.
>
> **Prereqs.** Install the `nros` CLI and provision the native host.
> `nros setup native` installs the zenoh router (`zenohd`) into a
> shared store — no ROS 2 needed.
>
> ```bash
> # Install the nros CLI once per machine:
> curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install-nros.sh | sh
> export PATH="$HOME/.nros/bin:$PATH"
>
> # Provision the native host for the zenoh RMW:
> nros setup native --rmw zenoh
> ```
>
> See [Install + first build (Linux)](./installation.md) for more.

## Project layout

The talker is a **standalone Cargo package** that pulls nano-ros in
via a path dependency. Three files matter:

```text
examples/native/rust/talker/
├── Cargo.toml          # path dep on `nros` + `nros-rmw-zenoh`
├── package.xml         # ROS-style manifest (drives codegen tooling)
├── generated/          # auto-generated message bindings (gitignored)
└── src/
    └── main.rs         # 60-line talker
```

POSIX talkers read the locator / domain from environment variables
(`NROS_LOCATOR` — legacy alias `ZENOH_LOCATOR` — and `ROS_DOMAIN_ID`)
— no `nros.toml` is needed. The `nros.toml` shape used by embedded
targets shows up under the Embedded Starters section.

The `Cargo.toml` is the contract that wires nano-ros into your
package. The in-tree talker is a **member of the nano-ros workspace**,
so it does NOT carry a `[workspace]` table — `cargo` walks up and
picks up the root `Cargo.toml`. Verbatim from
[`examples/native/rust/talker/Cargo.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/native/rust/talker/Cargo.toml)
(trimmed to the docs-relevant fields — the in-tree file also exposes
`rmw-cyclonedds` / `rmw-xrce` features for the multi-RMW build path):

```toml
[package]
name    = "native-rs-talker"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
publish = false

[[bin]]
name = "talker"
path = "src/main.rs"

[features]
default   = ["rmw-zenoh"]
rmw-zenoh = ["dep:nros-rmw-zenoh"]

[dependencies]
nros = { path = "../../../../packages/core/nros",
         default-features = false,
         features = ["std", "rmw-cffi", "platform-posix"] }
nros-platform-cffi = { path = "../../../../packages/core/nros-platform-cffi",
                       features = ["posix-c-port"] }
nros-rmw-zenoh = { path = "../../../../packages/zpico/nros-rmw-zenoh",
                   features = ["std", "platform-posix", "ros-humble"],
                   optional = true }
std_msgs = { version = "*", default-features = false }
log = "0.4"
env_logger = "0.11"
```

**Copying this out of the workspace?** Once you move the directory
elsewhere on disk, the path deps no longer resolve and there is no
parent workspace to inherit from. Two options:

1. Replace each `path = "../../../../packages/..."` with an absolute
   path to your nano-ros checkout, AND add an empty `[workspace]`
   table to stop cargo walking further up the filesystem.
2. Keep it inside `examples/` in your own fork of nano-ros — the
   simpler path while you're learning the API.

## Configure

Three runtime knobs, each overridable at three layers (defaults →
`config.toml` → env vars):

| Knob | Default | Env override |
|---|---|---|
| Zenoh locator | `tcp/127.0.0.1:7447` | `NROS_LOCATOR` (legacy alias: `ZENOH_LOCATOR`) |
| ROS domain ID | `0` | `ROS_DOMAIN_ID` |
| Zenoh mode | client | `NROS_SESSION_MODE` (legacy alias: `ZENOH_MODE`) |

`config.toml` (optional, alongside `Cargo.toml`):

```toml
[zenoh]
locator   = "tcp/127.0.0.1:7447"
domain_id = 0
```

## Build

```bash
cd examples/native/rust/talker
cargo build           # or: cargo build --release
```

First build pulls dependencies (~3 minutes). Re-builds finish in
seconds.

## Run

Three terminals (each command below blocks; keep them open):

```bash
# Terminal 1 — zenoh router. Blocks the shell until Ctrl-C.
zenohd                               # installed by `nros setup native`

# Terminal 2 — the talker. The talker logs via `log::info!`, so set
# RUST_LOG=info — without it `env_logger` only shows errors and the
# `Published: N` lines stay hidden.
cd examples/native/rust/talker
RUST_LOG=info cargo run
# Expected output (on stderr):
#   [INFO  native_rs_talker] nros Native Talker (Zenoh Transport)
#   [INFO  native_rs_talker] =========================================
#   [INFO  native_rs_talker] Published: 0
#   [INFO  native_rs_talker] Published: 1
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

**Readiness signal.** Within 5 seconds of `RUST_LOG=info cargo run`,
the talker should print `Published: 0` (the Rust talker pre-publishes
`0` before the counter advances). If no `Published:` line in 30
seconds:

1. Confirm `RUST_LOG` is set. Without `RUST_LOG=info` (or `debug`),
   `env_logger` filters out the `Published:` lines and the run looks
   silent even when it's working.
2. Confirm `zenohd` is running (terminal 1). Without it, the talker
   blocks on `Executor::open` indefinitely.
3. Re-run with `RUST_LOG=debug cargo run` and look for "Failed to
   open session" — usually a wrong locator or wrong port.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

Canonical, copy-out:
[`examples/native/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/rust/talker)

Copy the directory, rename the package, and your starter is ready to
modify.

## Next

- Add a subscription:
  [`examples/native/rust/listener/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/rust/listener)
- Generate bindings for custom `.msg` / `.srv` / `.action` files:
  [Message Generation](../user-guide/message-generation.md)
- Cross-compile for an RTOS: pick the right Embedded Starter from
  the next section (FreeRTOS / Zephyr / NuttX / ThreadX / ESP32 /
  Bare-metal Cortex-M3).
