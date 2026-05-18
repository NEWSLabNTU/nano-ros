# NuttX (integration shell)

> **Contributor docs?** Building nano-ros's own NuttX examples on
> QEMU from this repository is covered at [NuttX (contributor)](./nuttx.md).

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

## Per-example external apps (Phase 157)

The 12 C/C++ examples under
`examples/qemu-arm-nuttx/{c,cpp}/zenoh/` ship as canonical
NuttX external apps — each carries a sibling `Kconfig`,
`Make.defs`, and `Makefile` next to its `CMakeLists.txt`. They
register as built-in `nshlib` commands so you can run them
directly from the NSH prompt:

```
nsh> nuttx_c_talker
nsh> nuttx_cpp_listener
```

### Staging script (one-liner)

`scripts/nuttx/stage-external-apps.sh` symlinks the integration
shell + every example into `apps/external/`, runs per-example
codegen (`nros_generate_interfaces` / `nros_find_interfaces` /
`nano_ros_generate_config_header` equivalents in Python), and
builds per-package C++ FFI staticlibs:

```bash
scripts/nuttx/stage-external-apps.sh /path/to/nuttx-apps
```

After staging, `apps/external/` looks like:

```
apps/external/
├── Kconfig              # generated — sources every nano-ros sub-Kconfig
├── Make.defs            # generated — wildcard includes
├── nano-ros            -> .../integrations/nuttx
├── nano-ros-talker-c   -> .../examples/qemu-arm-nuttx/c/zenoh/talker
├── nano-ros-listener-c -> .../examples/qemu-arm-nuttx/c/zenoh/listener
├── nano-ros-talker-cpp -> .../examples/qemu-arm-nuttx/cpp/zenoh/talker
└── …
```

### Justfile wrapper (recommended)

`just nuttx build-fixtures-make` handles staging, defconfig
setup, Kconfig propagation, and the kernel build in one shot:

```bash
just nuttx build-fixtures-make
# → $NUTTX_DIR/nuttx with all 12 examples linked as built-ins
```

Auto-configures NuttX with the nano-ros board defconfig (full
networking + virtio-net + TLS support) if `.config` is missing
or lacks `CONFIG_NET=y`. Respects a pre-existing user defconfig
as long as `CONFIG_NET` is on.

### Two parallel paths

  * **CMake bring-up** (`just nuttx build-fixtures`) — the
    Phase 144.6 standalone path. Fast smoke per example;
    Corrosion + in-tree Cargo workspace. Good for regression
    coverage.
  * **Make-based external apps** (`just nuttx
    build-fixtures-make`) — canonical NuttX user flow.
    Exercises `apps/external/*/Make.defs` discovery,
    `apps/Application.mk` integration, `EXTRA_LIBS`/`CFLAGS`
    propagation, `<PROGNAME>_main` rename. Slower (full
    kernel link) but matches what users follow from NuttX
    docs.

`just nuttx build-all` runs both.

### Per-example file layout

```
examples/qemu-arm-nuttx/c/zenoh/talker/
├── CMakeLists.txt   # canonical build entry (cmake path)
├── Kconfig          # NROS_EXAMPLE_TALKER_C declaration
├── Make.defs        # ifneq → CONFIGURED_APPS += apps/external/nano-ros-talker-c
├── Makefile         # delegates to apps/Application.mk
├── config.toml      # network + zenoh config (auto-generates app_config.h)
├── package.xml      # msg-pkg deps (auto-resolves via AMENT_PREFIX_PATH)
└── src/main.c
```

Same shape per CPP example with `src/main.cpp` + `LANGUAGE CPP`
codegen.

### Verification

The parity test
`packages/testing/nros-tests/tests/nuttx_make_e2e.rs` asserts
every example's `<PROGNAME>_main` symbol is present in the
built `nuttx` ELF:

```bash
cargo nextest run -p nros-tests --test nuttx_make_e2e
```

Run after `just nuttx build-fixtures-make`. Skips cleanly when
the staged tree is missing.
