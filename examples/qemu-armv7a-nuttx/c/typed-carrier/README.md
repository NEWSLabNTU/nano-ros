# `typed-carrier` — qemu-armv7a-nuttx / c

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## What this shows

A C **typed component** (`RCLC_C_COMPONENT`, RFC-0043) deployed with
`nros_components_register_node(... TYPED DEPLOY nuttx)`. The image comes
from the NuttX **typed-entry carrier**, which generates a C++ entry that
holds the component's node as a static object and boots it on the board's
executor. Every other example here links through `nano_ros_add_executable()`.

It is also the fixture for that build road (issue 1795). Until then the NuttX
carrier never fired on a lane build: the recipes pass
`NROS_PLATFORM=nuttx_armv7a`, and the arm compared the raw value with
`nuttx`.

## Build

```bash
cp -r examples/qemu-armv7a-nuttx/c/typed-carrier ~/my-typed-carrier && cd ~/my-typed-carrier
cmake -S . -B build -DNROS_REPO_DIR=/path/to/nano-ros   # or: export NROS_REPO_DIR=…
cmake --build build
```

No `nros sync` here: a C/C++ leaf's message bindings are a CMake-time
output. `-DNROS_REPO_DIR` only says where the checkout is — the board, the
RMW and the domain come from `system.toml` (below), never from a `-D` flag.

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [qemu-armv7a-nuttx README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-armv7a-nuttx/README.md).

## Config

Board, RMW, domain and locator: `system.toml` beside `CMakeLists.txt`
(`[image.qemu-armv7a-nuttx]` + `[system]`, RFC-0098 D3/D5). `find_package(nano_ros)`
reads it while CMake configures, so switching board is editing that one
line and re-configuring (`cmake -B build`); no build command and no
manifest names a board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
