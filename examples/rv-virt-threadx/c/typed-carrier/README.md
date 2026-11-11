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

A one-package workspace (RFC-0122 D3): the package is
`src/riscv64_threadx_c_typed_carrier`, and
`src/riscv64_threadx_c_typed_carrier_bringup` holds the `system.toml` that
says what to build. The image names the package as its OWNED entry (RFC-0071
D19), so its own `CMakeLists.txt` builds the image through the typed carrier;
`nros build` generates no entry.

```bash
cp -r examples/rv-virt-threadx/c/typed-carrier ~/my-typed-carrier && cd ~/my-typed-carrier
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build rv-virt-threadx
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [rv-virt-threadx README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/rv-virt-threadx/README.md).

## Config

Board, RMW, domain and locator:
`src/riscv64_threadx_c_typed_carrier_bringup/system.toml`
(`[image.rv-virt-threadx]` + `[system]`, RFC-0098 D3/D5). No build command
carries any of them, and the package names no board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
