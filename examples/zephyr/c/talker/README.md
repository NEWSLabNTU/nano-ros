# `talker` — zephyr / c

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/zephyr/c/talker ~/my-talker && cd ~/my-talker
```

Zephyr is the carve-out: the build verb is `west`, and the board plus the
`CONF_FILE` RMW overlay are west arguments — see the [zephyr README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/zephyr/README.md).
No `nros sync` here: a C/C++ leaf's message bindings are a build-system
output, generated while the project configures.

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [zephyr README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/zephyr/README.md).

## Config

Board, RMW, domain and locator: `system.toml` beside `CMakeLists.txt`
(`[image.zephyr]` + `[system]`, RFC-0098 D3/D5). `find_package(nano_ros)`
reads it while CMake configures, so switching board is editing that one
line and re-configuring (`cmake -B build`); no build command and no
manifest names a board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
