# `talker` — rv-virt-nuttx / c

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: the node package is `src/nuttx_riscv_c_talker` (a component,
`nuttx_riscv_c_talker::Talker`, with no `main()`), and `src/nuttx_riscv_c_talker_bringup` holds the
`system.toml` that says what to build. `nros build` generates the entry that
boots the node.

```bash
cp -r examples/rv-virt-nuttx/c/talker ~/my-talker && cd ~/my-talker
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build rv-virt-nuttx
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [rv-virt-nuttx README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/rv-virt-nuttx/README.md).
`nros build` prints the image's path.

## Config

Board, RMW, domain and locator: `src/nuttx_riscv_c_talker_bringup/system.toml`
(`[image.rv-virt-nuttx]` + `[system]`, RFC-0098 D3/D5). No build command carries any
of them, and the node package names no board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
