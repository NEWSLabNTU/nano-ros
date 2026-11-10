# `talker` — qemu-armv7a-nuttx / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: the node package is `src/nuttx_talker`, and
`src/nuttx_talker_bringup` holds the `system.toml` that says what to build.
`nros build` generates the entry that boots the node.

```bash
cp -r examples/qemu-armv7a-nuttx/rust/talker ~/my-talker && cd ~/my-talker
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build qemu-armv7a-nuttx
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [qemu-armv7a-nuttx README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-armv7a-nuttx/README.md).

`nros build` prints the image's path: the generated entry,
`qemu_armv7a_nuttx_entry`, under `build/nuttx-zenoh/qemu_armv7a_nuttx_entry/target/<triple>/`
— `<triple>` is the board's Rust target.

## Config

Board, RMW, domain and locator: `src/nuttx_talker_bringup/system.toml`
(`[image.qemu-armv7a-nuttx]` + `[system]`, RFC-0098 D3/D5). No build command
carries any of them, and `src/nuttx_talker/Cargo.toml` names no board crate:
the generated entry states the board, its RMW backend and its runtime
(RFC-0121 D6), so switching board is the `[image.*] board` line and nothing
else.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
