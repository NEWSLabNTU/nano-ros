# Switching RMW Backends

<!-- type: how-to · level: 2 -->

nano-ros selects its RMW backend at **build time** — there is no
`RMW_IMPLEMENTATION` environment variable to flip at runtime the way
desktop ROS 2 does. Which edit you make depends on how your project is
built. This page lists the exact change per builder; for help picking a
backend in the first place, see [Choosing an RMW](choosing-an-rmw.md).

Backend names are the same everywhere: `zenoh`, `xrce`, `cyclonedds`.
When nothing selects a backend, the default is `zenoh`.

## Workspace (C, C++, mixed or Rust)

One edit, in the bringup's `system.toml`:

```toml
[system]
rmw = "cyclonedds"          # zenoh | xrce | cyclonedds
```

or, for one image only, `rmw = "…"` inside its `[image.<id>]` block. Then

```bash
nros build
```

A workspace has no root build file to edit: `nros build`
writes the CMake root under `build/` with `BACKEND` taken from the
image's `rmw`, and for a Rust image it generates the entry that gives the
board crate the backend feature. Each image builds
into its own directory, so no object files from another backend linger.

## A CMake tree that is not a workspace

A CMake project with no `system.toml` that consumes nano-ros in source
form — either `find_package(nano_ros REQUIRED)` (the ament-shape entry
point, `nano_rosConfig.cmake` at the SDK root, located via
`-Dnano_ros_ROOT`) or a raw `add_subdirectory`
([Build as a CMake subdirectory](../build/cmake-subdirectory.md)) — picks
its backend on the configure line. `nros setup presets` writes the
package a `CMakePresets.json` that already carries the SDK root and the
board's toolchain (from the presets `nros setup <board>` wrote), so the
configure line names only the backend:

```bash
nros setup presets                       # once per package
cmake --preset linux -DNROS_RMW=cyclonedds   # zenoh | xrce | cyclonedds
cmake --build build/linux
```

Use one build directory per backend (`-B build-zenoh`,
`-B build-cyclonedds`, …) if you switch back and forth.

## A Rust node package

A Rust node package needs no edit at all. Its `Cargo.toml` names no
`rmw-<x>` or `ros-<edition>` feature and no board crate, and no code calls a
backend's `register()`: the generated entry boots through the board crate,
which links the backend the image selects and registers it. The workspace
section above is the whole change.

## Several backends from one bringup

A bringup whose nodes run on more than one backend declares one image per RMW
in its `system.toml` and builds the image it names:

```toml
# src/talker_node_bringup/system.toml
[system]
name = "talker_node"
rmw = "zenoh"
default_images = ["linux"]

[image_defaults]
board = "linux"

[image.linux]

[image.linux_cyclonedds]
rmw = "cyclonedds"
```

```bash
nros build linux_cyclonedds
```

The full-system declaration model — how the one value in `system.toml`
is lowered to cargo features and CMake cache variables — is specified in
the RMW selection-and-lowering design (see Design background below).

## Embedded targets

On embedded platforms the backend is fixed at **compile time** like
everything else — there is nothing to switch on the device. Each
platform surfaces the same choice through its own configuration
mechanism; on Zephyr, for instance, the image's `rmw` in `system.toml` is
rendered into a Kconfig choice (`CONFIG_NROS_RMW_ZENOH` /
`CONFIG_NROS_RMW_XRCE` / `CONFIG_NROS_RMW_CYCLONEDDS`, exactly one), never
written in a conf file. One image per backend, each with the Zephyr
settings that backend needs:

```toml
[image.zephyr_zenoh]
board = "zephyr"
rmw = "zenoh"
os_config = ["prj-zenoh.conf"]

[image.zephyr_cyclonedds]       # Cyclone: Cortex-A/R and native_sim targets
board = "zephyr"
rmw = "cyclonedds"
os_config = ["prj-cyclonedds.conf"]
```

A Cyclone DDS image also needs multicast-capable networking on the
target; the per-platform settings are in
[Choosing an RMW](choosing-an-rmw.md#cyclone-dds-network-configuration-per-platform).

See the platform guides for the concrete per-platform workflow:

- [Zephyr](../targets/zephyr.md)
- [FreeRTOS](../targets/freertos.md)
- [NuttX](../targets/nuttx.md)
- [ThreadX](../targets/threadx.md)
- [ESP32](../migrating/esp32.md)

and [When `nros build` drives the build](../targets/how-integration-works.md#when-nros-build-drives-the-build) for
which builder applies where.

## Design background

- [RFC-0031: RMW selection and lowering](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0031-rmw-selection-and-lowering.md)
- [RFC-0048: CMake / ament consumption](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0048-cmake-ament-consumption.md)
- [RFC-0098: generated leaf build config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md)
