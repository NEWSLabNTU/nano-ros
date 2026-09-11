# `service-client` — mps2-an385-freertos / cpp

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/mps2-an385-freertos/cpp/service-client ~/my-service-client && cd ~/my-service-client
cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros   # or: export NROS_REPO_DIR=…
cmake --build build
```

No `nros sync` here: a C/C++ leaf's message bindings are a CMake-time
output. `-DNANO_ROS_ROOT` only says where the checkout is — the board, the
RMW and the domain come from `system.toml` (below), never from a `-D` flag.

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [mps2-an385-freertos README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-freertos/README.md).

## Config

Board, RMW, domain and locator: `system.toml` beside `CMakeLists.txt`
(`[image.mps2-an385-freertos]` + `[system]`, RFC-0098 D3/D5). `find_package(nano_ros)`
reads it while CMake configures, so switching board is editing that one
line and re-configuring (`cmake -B build`); no build command and no
manifest names a board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
