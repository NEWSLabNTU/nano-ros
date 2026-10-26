# `typed-carrier` — rv-virt-threadx / c

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## What this shows

A C **typed component** (`RCLC_C_COMPONENT`, RFC-0043) deployed with
`nros_components_register_node(... TYPED DEPLOY threadx)`. The image comes
from the ThreadX **typed-entry carrier**, which generates a C++ entry that
holds the component's node as a static object and boots it on the board's
executor. Every other example here links through `nano_ros_add_executable()`.

It is also the fixture for that build road (issue 1795). The road failed to
link twice, in issues 1742 and 1753, while nothing built it.

## Build

```bash
cp -r examples/rv-virt-threadx/c/typed-carrier ~/my-typed-carrier && cd ~/my-typed-carrier
cmake -S . -B build -Dnano_ros_ROOT=/path/to/nano-ros   # or: nros build
cmake --build build
```

No `nros sync` here: a C/C++ leaf's message bindings are a CMake-time
output. `-Dnano_ros_ROOT` is `find_package(nano_ros)`'s hint: it says where
the checkout is, and nothing else. The board, the RMW and the domain come
from `system.toml` (below), never from a `-D` flag.

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [rv-virt-threadx README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/rv-virt-threadx/README.md).

## Config

Board, RMW, domain and locator: `system.toml` beside `CMakeLists.txt`
(`[image.rv-virt-threadx]` + `[system]`, RFC-0098 D3/D5). `find_package(nano_ros)`
reads it while CMake configures, so switching board is editing that one
line and re-configuring (`cmake -B build`); no build command and no
manifest names a board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
