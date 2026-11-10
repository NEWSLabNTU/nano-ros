# `talker` — threadx-linux / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: the node package is `src/threadx_linux_rs_talker`,
and `src/threadx_linux_rs_talker_bringup` holds the `system.toml` that says
what to build. `nros build` generates the entry that boots the node.

```bash
cp -r examples/threadx-linux/rust/talker ~/my-talker && cd ~/my-talker
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build threadx-linux
```

## Run

Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`).
`nros build` prints the binary's path: the generated entry,
`build/threadx-linux-zenoh/threadx_linux_entry/target/debug/threadx_linux_entry`.

## Config

Board, RMW, domain and locator: `src/threadx_linux_rs_talker_bringup/system.toml`
(`[image.threadx-linux]` + `[system]`, RFC-0098 D3/D5). No build command
carries any of them, and `src/threadx_linux_rs_talker/Cargo.toml` names no
board crate: the generated entry states the board, its RMW backend and its
runtime (RFC-0121 D6), so switching board is the `[image.*] board` line and
nothing else.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
