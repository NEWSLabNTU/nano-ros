# First Node — Rust (Linux)

Build, run, and verify a single nano-ros publisher node on Linux in
about ten minutes. Uses the canonical Zenoh backend. The zenoh router comes from ROS 2
(`ros2 run rmw_zenoh_cpp rmw_zenohd`) — nano-ros no longer ships one.
No ROS 2 on this machine? Use `--rmw cyclonedds` instead: it needs no
router at all (see [Choosing an RMW](../user-guide/rmw-choosing.md)).

> **Stuck?** See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md) for the common first-build errors.

## Prereqs

The `nros` CLI on PATH ([Installation](./installation.md) — `install.sh`
puts it in `~/.nros/bin`), then provision the Linux host (installs the
zenoh client stack into a shared store; the router itself comes from
your ROS 2 install):

```sh
nros setup linux --rmw zenoh
```

## Scaffold the talker

```sh probe=28
nros new my_talker --platform linux --lang rust --rmw zenoh
cd my_talker
```

The talker is a **standalone Cargo package** with its own
`system.toml`. These files matter:

```text
my_talker/
├── system.toml         # WHAT this deploys to — the one file you edit
├── Cargo.toml          # registry-style deps; no board facts
├── package.xml         # ROS-style manifest (drives codegen tooling)
├── generated/          # message bindings `nros sync` writes
├── build/              # everything `nros sync` / `nros build` generate
└── src/
    ├── lib.rs          # the talker node: a 1 s timer publishing /chatter
    └── main.rs         # one line, `nros::main!(spin = "forever")`
```

`system.toml` is the whole deployment statement — the same schema a
multi-node workspace's bringup package uses (RFC-0098):

```toml
[system]
name = "my_talker"
rmw = "zenoh"
ros_edition = "humble"
domain_id = 0

[[component]]
pkg = "my_talker"
class = "my_talker::Talker"
name = "my_talker"

[image.linux]
board = "linux"
```

The `[[component]]` is the node `main.rs` boots: `nros::main!()` reads it
and runs `lib.rs`'s `Talker`. Switching RMW or ROS edition is editing
`rmw` or `ros_edition` here and re-running `nros sync`.

**You never hand-write a build setting.** No target triple, no linker
flag, no `[patch.crates-io]` table, no `.cargo/config.toml`: `nros sync`
derives every one of those from `board = "linux"` and writes them to
`build/linux/nros-cargo.toml`, which `nros build` hands cargo with
`--config`.

Native (host) images read the locator / domain from environment
variables (`NROS_LOCATOR` — the retired `ZENOH_LOCATOR` is refused at startup — and
`ROS_DOMAIN_ID`). Embedded images bake theirs from the same
`[image.<id>]` table (`locator`, and the address in `net`) — the shape
shows up under the Embedded Starters section.

The `Cargo.toml` is the contract that wires nano-ros into your
package, and it holds Rust facts only. The package is its own
standalone Cargo root (an empty `[workspace]` table stops `cargo`
walking up the filesystem), and nano-ros crates are declared
**registry-style** (`version = "*"`): nothing of nano-ros is on
crates.io, so `nros sync` generates the `[patch.crates-io]` block that
resolves them into the SDK the `nros` on PATH carries, into
`build/<image>/nros-cargo.toml` — nothing about the build is committed,
and nothing about it is hand-edited (RFC-0098 D1). The manifest names no
board crate, RMW backend or ROS edition either: it depends on
`my_talker_nros_selection = { path = "build/nros/selection" }`, a package
`nros sync` generates from `system.toml` that brings all three (RFC-0098 D6),
so the `--rmw` and `--ros-edition` you passed to `nros new` live in
`system.toml` and nowhere else.

Message packages are not listed there by hand: `package.xml`'s
`<depend>std_msgs</depend>` is the one statement, and `nros sync` (or
`nros build`) writes the matching `std_msgs = { path = "generated/std_msgs",
… }  # nros-managed` row. To use another message package, add its
`<depend>` and sync again — that is the whole change.

The directory names no path into the SDK, so you can move or copy it
anywhere and re-run `nros sync`; `--nano-ros-path <dir>` names a
different SDK root on the command line, never in a file.

For a fuller talker to read — the same shape, with one image per RMW
and ROS edition — see
[`examples/linux/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/rust/talker)
on GitHub.

## Configure

Three runtime knobs, each an env override on a built-in default:

| Knob | Default | Env override |
|---|---|---|
| Zenoh locator | `tcp/127.0.0.1:7447` | `NROS_LOCATOR` |
| ROS domain ID | `0` | `ROS_DOMAIN_ID` |
| Zenoh mode | client | `NROS_SESSION_MODE` |

On native these stay environment variables — `[image.linux]` declares
no `locator`. An embedded image bakes them from its own `[image.<id>]`
table instead; see the
[Configuration Guide](../user-guide/configuration.md).

## Build

```bash probe=30
nros sync             # writes generated/ message bindings and
                      # build/linux/nros-cargo.toml from system.toml
nros build            # builds the image `[image.linux]` declares
```

First build pulls dependencies (~3 minutes). Re-builds finish in
seconds. The binary lands at `build/linux/target/debug/my_talker` —
under `build/`, never beside your sources.

**Driving cargo yourself** (an IDE, a CI step, `--release`) is
supported: run `nros sync` first, then point cargo at the generated
settings file, from the directory above the package:

```bash
cd ..
cargo build --manifest-path my_talker/Cargo.toml \
            --config my_talker/build/linux/nros-cargo.toml
```

That file is regenerated on every `nros sync`; never edit it, and never
add a `.cargo/config.toml` of your own to carry board facts. Your own
Rust-toolchain preferences go where cargo already looks for them — a
parent directory or `$CARGO_HOME`.

## Run

Three terminals (each command below blocks; keep them open):

```bash
# Terminal 1 — zenoh router (ROS 2's own). Blocks the shell until Ctrl-C.
source /opt/ros/humble/setup.bash
ros2 run rmw_zenoh_cpp rmw_zenohd

# Terminal 2 — the talker. The talker logs via `log::info!`, so set
# RUST_LOG=info — without it `env_logger` only shows errors and the
# `Publishing:` lines stay hidden.
cd my_talker
RUST_LOG=info ./build/linux/target/debug/my_talker
# Expected output (on stderr):
#   [INFO  my_talker] Publishing: 'Hello World: 1'
#   [INFO  my_talker] Publishing: 'Hello World: 2'
#   …
```

That's the nano-ros side fully working. **Optional step:** verify
interop with stock ROS 2.

```bash
# Terminal 3 — stock ROS 2 with rmw_zenoh_cpp. It talks to the SAME
# router already running in terminal 1 (default tcp/127.0.0.1:7447).
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

If `ros2 topic echo` shows no output despite the talker printing
`Publishing:`, the routers aren't peering — confirm both processes
point at the same port (default `tcp/127.0.0.1:7447`).

**Readiness signal.** Within ~6 seconds of starting the binary
(session open + the first 1 s timer tick), the talker should print
`Publishing: 'Hello World: 1'` — the count starts at 1, matching the
official ROS 2 `demo_nodes_cpp` talker. If no `Publishing:` line in 30
seconds:

1. Confirm `RUST_LOG` is set. Without `RUST_LOG=info` (or `debug`),
   `env_logger` filters out the `Publishing:` lines and the run looks
   silent even when it's working.
2. Confirm the router is running (terminal 1). Without it, the talker
   blocks on `Executor::open` indefinitely.
3. Re-run with `RUST_LOG=debug` and look for "RMW session open
   failed" — usually a wrong locator or wrong port.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## Retargeting

To aim the package at a board, change `[image.linux] board` in
`system.toml` to that board's name ([Supported Boards](../reference/supported-boards.md)),
run `nros setup <board>`, and re-run `nros sync`. The scaffold's
`Cargo.toml` names the Linux board crate (`nros-board-linux`), so a
cross build swaps that dependency too (issue 1305); the platform
chapters scaffold with `--platform <freertos|nuttx|threadx|baremetal>`,
which writes the right one.

## Next

- Add a subscription:
  [`examples/linux/rust/listener/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/rust/listener)
- Generate bindings for custom `.msg` / `.srv` / `.action` files:
  [Message Generation](../user-guide/message-generation.md)
- Cross-compile for an RTOS: pick the right Embedded Starter from
  the next section (FreeRTOS / Zephyr / NuttX / ThreadX / ESP32 /
  Bare-metal Cortex-M3).
