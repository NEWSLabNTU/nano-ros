# `talker` — rv-virt-threadx / cpp

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: the node package is `src/riscv64_threadx_cpp_talker` (a component,
`riscv64_threadx_cpp_talker::Talker`, with no `main()`), and `src/riscv64_threadx_cpp_talker_bringup` holds the
`system.toml` that says what to build. `nros build` generates the entry that
boots the node.

```bash
cp -r examples/rv-virt-threadx/cpp/talker ~/my-talker && cd ~/my-talker
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build rv-virt-threadx
```

The bringup also declares `rv-virt-threadx-cyclonedds`, the same node on Cyclone DDS
(`nros build rv-virt-threadx-cyclonedds`).

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [rv-virt-threadx README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/rv-virt-threadx/README.md).
`nros build` prints the image's path.

## Config

Board, RMW, domain and locator: `src/riscv64_threadx_cpp_talker_bringup/system.toml`
(`[image.rv-virt-threadx]` + `[system]`, RFC-0098 D3/D5). No build command carries any
of them, and the node package names no board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
