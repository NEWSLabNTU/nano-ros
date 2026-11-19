# `errno-isolation` — rv-virt-threadx / c

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([example directory layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A one-package workspace: the package is
`src/riscv64_threadx_c_errno_isolation`, and
`src/riscv64_threadx_c_errno_isolation_bringup` holds the `system.toml` that
says what to build. Its `main` is the test, and the image names the package as
its OWNED entry, so `nros build` generates no entry.

```bash
cp -r examples/rv-virt-threadx/c/errno-isolation ~/my-errno-isolation && cd ~/my-errno-isolation
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build rv-virt-threadx
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [rv-virt-threadx README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/rv-virt-threadx/README.md).

## Config

Board, RMW, domain and locator:
`src/riscv64_threadx_c_errno_isolation_bringup/system.toml`
(`[image.rv-virt-threadx]` + `[system]`). No build command
carries any of them, and the package names no board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
