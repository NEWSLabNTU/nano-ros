# `talker` — mps2-an385-freertos / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: the node package is `src/freertos_rs_talker`, and
`src/freertos_rs_talker_bringup` holds the `system.toml` that says what to
build. `nros build` generates the entry that boots the node.

```bash
cp -r examples/mps2-an385-freertos/rust/talker ~/my-talker && cd ~/my-talker
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build mps2-an385-freertos
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [mps2-an385-freertos README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-freertos/README.md).

`nros build` prints the image's path: the generated entry,
`build/freertos-zenoh/mps2_an385_freertos_entry/target/<triple>/debug/mps2_an385_freertos_entry`
— `<triple>` is the board's Rust target.

## Config

Board, RMW, domain and locator: `src/freertos_rs_talker_bringup/system.toml`
(`[image.mps2-an385-freertos]` + `[system]`, RFC-0098 D3/D5). No build command
carries any of them, and `src/freertos_rs_talker/Cargo.toml` names no board
crate: the generated entry states the board, its RMW backend and its runtime
(RFC-0121 D6), so switching board is the `[image.*] board` line and nothing
else.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
