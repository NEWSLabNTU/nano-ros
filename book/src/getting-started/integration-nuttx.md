# NuttX (integration shell)

Phase 139 ships a NuttX app shell under `integrations/nuttx/`.
NuttX users symlink (or submodule) the shell into
`apps/external/nano-ros/`; Kconfig surfaces it under
`Application Configuration → External Modules → nano-ros`.

## Prereqs

- NuttX ≥ nuttx-12 checkout (with `apps/` sibling)
- A NuttX cross-toolchain (e.g. `gcc-arm-none-eabi` for ARM
  configurations)

## One-liner integrate

```bash
ln -s /path/to/nano-ros/integrations/nuttx \
      $NUTTX_DIR/../apps/external/nano-ros
```

Or as a git submodule under `apps/external/`. NuttX's app
discovery picks up the `Make.defs` automatically.

## Enable via menuconfig

```bash
cd $NUTTX_DIR
make menuconfig
# Navigate to: Application Configuration → External Modules → nano-ros
# Enable: nano-ros ROS 2 client
# Set: RMW backend (zenoh), ROS 2 edition (humble)
make
```

## Build flavours

- **Kconfig + Make** (default for ARM configs): uses `Make.defs` +
  `Makefile`. The Cargo-built `libnros_c.a` is linked at the final
  app stage (`apps/external/nano-ros/Makefile` declares the
  consumer-app shape).
- **CMake** (cmake-driven NuttX builds): uses `CMakeLists.txt`,
  which dispatches into the Phase 137 root CMake with
  `NANO_ROS_PLATFORM=nuttx`.

Both surfaces coexist — pick the one matching your NuttX config.

## Minimal user code

User application code consumes nano-ros via the standard headers
exactly as on any other RTOS:

```c
#include <nros/init.h>
```

See `examples/qemu-arm-nuttx/` for a working publisher loop.
