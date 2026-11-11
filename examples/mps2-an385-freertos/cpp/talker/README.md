# `talker` — mps2-an385-freertos / cpp

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: the node package is `src/freertos_cpp_talker` (a component,
`freertos_cpp_talker::Talker`, with no `main()`), and `src/freertos_cpp_talker_bringup` holds the
`system.toml` that says what to build. `nros build` generates the entry that
boots the node.

```bash
cp -r examples/mps2-an385-freertos/cpp/talker ~/my-talker && cd ~/my-talker
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build mps2-an385-freertos
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [mps2-an385-freertos README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-freertos/README.md).
`nros build` prints the image's path.

## Config

Board, RMW, domain and locator: `src/freertos_cpp_talker_bringup/system.toml`
(`[image.mps2-an385-freertos]` + `[system]`, RFC-0098 D3/D5). No build command carries any
of them, and the node package names no board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
