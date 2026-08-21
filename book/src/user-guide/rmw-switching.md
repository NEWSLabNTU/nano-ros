# Switching RMW Backends

nano-ros selects its RMW backend at **build time** — there is no
`RMW_IMPLEMENTATION` environment variable to flip at runtime the way
desktop ROS 2 does. Which edit you make depends on how your project is
built. This page lists the exact change per builder; for help picking a
backend in the first place, see [Choosing an RMW](./rmw-choosing.md).

Backend names are the same everywhere: `zenoh`, `xrce`, `cyclonedds`.
When nothing selects a backend, the default is `zenoh`.

## C/C++ workspace (CMake)

One edit. The root `CMakeLists.txt` of a nano-ros workspace calls
`nano_ros_workspace(...)`; change its `BACKEND` argument:

```cmake
nano_ros_workspace(
    ORDER_FROM_DEPENDS
    BACKEND  cyclonedds          # zenoh | xrce | cyclonedds
    PLATFORM "${NANO_ROS_PLATFORM}"
    SUBDIRS  ${_ws_subdirs})
```

Then reconfigure and rebuild — preferably into a fresh build directory,
so no object files from the previous backend linger.

Note: when the root hard-codes `BACKEND`, passing `-DNROS_RMW=...` on
the configure line is **silently overridden** — `nano_ros_workspace()`
stamps the workspace-wide RMW from its `BACKEND` argument, and the
cache variable loses without a warning. Edit the `BACKEND` argument;
do not fight it from the command line.

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

## Rust workspace

Today this is **three edits** that must agree. (A scaffold flag that
bakes all three from one value is planned; until then, treat this as a
checklist.) The example below switches to `cyclonedds`:

| # | File | Edit |
|---|------|------|
| 1 | `src/<bringup_pkg>/system.toml` | `[system]` table: `rmw = "cyclonedds"` |
| 2 | Entry package `Cargo.toml` — the **board** dependency | `nros-board-linux = { path = "...", default-features = false, features = ["rmw-cyclonedds"] }` |
| 3 | Entry package `Cargo.toml` — the **`nros`** dependency | add `"rmw-cyclonedds"` to its `features` list |

Why three places:

1. **`system.toml`** is the declared, language-agnostic selection — it
   is what the toolchain and deploy tooling read. See
   [Configuration](./configuration.md).
2. **The board crate** is what actually links the backend. On the host
   board, `nros-board-linux`'s features are `rmw-zenoh` (the default),
   `rmw-xrce`, and `rmw-cyclonedds`; each pulls in the corresponding
   backend crate and registers it during boot. Switching away from
   zenoh therefore needs `default-features = false` plus the feature
   you want.
3. **The `nros` facade feature `rmw-cyclonedds`** is the
   type-descriptor gate: Cyclone DDS resolves topic types through a
   runtime descriptor registry, and this feature turns on the hook that
   registers a message's descriptor before the topic is created.
   Without it, `create_publisher` fails at runtime because the
   descriptor was never registered. (Zenoh and XRCE do not need a
   facade feature — steps 1 and 2 are enough for them.)

After editing, re-run `nros sync` in the workspace and rebuild.

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
