# `listener-host-loop` — linux / rust

The `listener` example with a `main` that owns its loop: `nros::setup!()`
emits the assembly `system.toml` describes and leaves the loop to the program
([RFC-0112](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0112-framework-support-tiers.md)
Tier 0, [phase-520](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/phase-520-setup-macro-host-loop.md)).
Start here when something else — an async runtime, a GUI main loop — already
owns `main`.

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/linux/rust/listener-host-loop ~/my-host-loop && cd ~/my-host-loop
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros sync     # generated/ message crates + build/<image>/nros-cargo.toml
nros build    # or: nros build linux
```

## Run

Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`) and a talker
(`examples/linux/rust/talker`, or `ros2 run demo_nodes_cpp talker`).
`nros build` leaves the binary at `build/linux/target/debug/listener-host-loop`.
SIGTERM (or Ctrl-C) ends the loop and the board closes the RMW session.

## What `nros::setup!()` gives `main`

| Function | What it does |
| --- | --- |
| `nros_boot(host)` | the board's start-up (RMW registration, log sinks, the executor sized from the model), then `host(rt)`; never spins |
| `nros_setup(rt)` | installs every `[[component]]` the model declares |
| `nros_stop_requested()` | the board's termination guard |
| `nros_deploy()` | the image's locator / domain / transport / RMW |

## Config

Board, RMW, domain and locator: `system.toml` beside `Cargo.toml`, exactly as
for `listener`. `system.contract.yaml` is that example's too: one
`KEEP_LAST(1)` subscription on `/chatter`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
