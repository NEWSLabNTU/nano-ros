# Switching RMW Backends

nano-ros selects its RMW backend at **build time** — there is no
`RMW_IMPLEMENTATION` environment variable to flip at runtime the way
desktop ROS 2 does. Which edit you make depends on how your project is
built. This page lists the exact change per builder; for help picking a
backend in the first place, see [Choosing an RMW](./rmw-choosing.md).

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
nros sync
nros build
```

A workspace has no root build file to edit (RFC-0098 D9): `nros build`
writes the CMake root under `build/` with `BACKEND` taken from the
image's `rmw`, and for a Rust image `nros sync` writes the selection
facade that gives the board crate the backend feature. Each image builds
into its own directory, so no object files from another backend linger.

> **Known issue ([1295](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1295-rust-cyclonedds-workspace-entry-no-publisher.md)).**
> A *Rust* image on `cyclonedds` also needs the `nros` crate's
> `rmw-cyclonedds` feature — the type-descriptor gate: Cyclone DDS
> resolves topic types through a runtime descriptor registry, and this
> feature registers a message's descriptor before its topic is created.
> The generated facade does not name it yet, so such an entry builds and
> then fails `PublisherCreationFailed` at startup. Zenoh and XRCE need no
> such feature, and C/C++ images are not affected.

## Single C/C++ package (no workspace function)

A standalone package that consumes nano-ros in source form — either
`find_package(nano_ros REQUIRED)` (the ament-shape entry point,
`nano_rosConfig.cmake` at the checkout root, located via
`nano_ros_ROOT`; RFC-0048) or a raw `add_subdirectory` — for
example a copied-out leaf like `examples/native/c/talker/` — picks its
backend on the configure line:

```bash
cmake -S . -B build -DNROS_RMW=cyclonedds   # zenoh | xrce | cyclonedds
cmake --build build
```

Use one build directory per backend (`build-zenoh/`, `build-cyclonedds/`,
…) if you switch back and forth.

## Single Rust package

A single Rust package with its own `system.toml` (no bringup) is the same
one edit — `rmw` in that `system.toml` — followed by `nros sync` and a
rebuild. The Rust workspace case is covered by the section above: a
workspace entry is generated, so there is no entry `Cargo.toml` to edit.

The full-system declaration model — how the one value in `system.toml`
is lowered to cargo features and CMake cache variables — is specified in
[RFC-0031](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0031-rmw-selection-and-lowering.md).

## Embedded targets

On embedded platforms the backend is fixed at **compile time** like
everything else — there is nothing to switch on the device. Each
platform surfaces the same choice through its own configuration
mechanism; on Zephyr, for instance, it is a Kconfig choice
(`CONFIG_NROS_RMW_ZENOH` / `CONFIG_NROS_RMW_XRCE` /
`CONFIG_NROS_RMW_CYCLONEDDS`, exactly one selected).

See the platform guides for the concrete per-platform workflow:

- [Zephyr](../getting-started/zephyr.md)
- [FreeRTOS](../getting-started/freertos.md)
- [NuttX](../getting-started/nuttx.md)
- [ThreadX](../getting-started/threadx.md)
- [ESP32](../getting-started/esp32.md)

and [Workflow by Platform and Language](./workflow-by-platform.md) for
the overview of which builder applies where.
