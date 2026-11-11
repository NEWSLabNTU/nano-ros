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

A one-package workspace (RFC-0122 D3): the package is
`src/nuttx_c_typed_carrier`, and `src/nuttx_c_typed_carrier_bringup` holds the
`system.toml` that says what to build. The image names the package as its
OWNED entry (RFC-0071 D19), so its own `CMakeLists.txt` builds the image
through the typed carrier; `nros build` generates no entry.

```bash
cp -r examples/qemu-armv7a-nuttx/c/typed-carrier ~/my-typed-carrier && cd ~/my-typed-carrier
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build qemu-armv7a-nuttx
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [qemu-armv7a-nuttx README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-armv7a-nuttx/README.md).

## Config

Board, RMW, domain and locator:
`src/nuttx_c_typed_carrier_bringup/system.toml` (`[image.qemu-armv7a-nuttx]` +
`[system]`, RFC-0098 D3/D5). No build command carries any of them, and the
package names no board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
