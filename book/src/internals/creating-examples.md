# Creating Examples

This guide describes the canonical example shape adopted
(layout) + (consumption). When adding a new example, copy
the nearest existing peer; the per-platform working examples are the
authoritative templates.

## Canonical layout

Every example is a **self-contained, copy-out project** under one of:

| Path | Used for |
|---|---|
| `examples/<plat>/<lang>/<example>/` | The standard cell. RMW is selected at **build time** (Cargo features / `-DNANO_ROS_RMW=` / Kconfig overlay), not encoded in the path. A single-package "app" example here is the canonical **starter** shape; the multi-package workspace shape (Node + Bringup + Entry pkgs) kicks in at ≥2 nodes — see [Multi-Node Projects](../getting-started/workspace-from-app-node.md). |
| `examples/bridges/<name>/` | Cross-RMW gateway examples (one binary, multiple backends). |
| `examples/templates/<name>/` | Multi-platform copy-out recipes (e.g. `multi-package-workspace`). |

The `<plat>` × `<lang>` coverage matrix (RMW chosen at build time) is authoritative in
[`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
Intentionally empty cells (bare-metal C/C++, PX4 Rust, …) are listed
in the same file; do not fill them without lifting the underlying
constraint.

Variant naming uses **suffix form** so peers sort together:
`talker-rtic`, `service-client-async`, `talker-rtic-mixed`. Avoid
parallel parent directories like `async-*/` or `rtic-*/`.

## Non-example binaries

Tests / benches / smokes are **not** under `examples/`. They live
under `packages/testing/`:

| Use | Location |
|---|---|
| Performance, fairness, stress, large-msg | `packages/testing/nros-bench/<name>/` |
| Driver / board bringup smoke (no nros API) | `packages/testing/nros-smoke/<name>/` |
| Fixture binaries built by integration tests | `packages/testing/nros-tests/bins/<name>/` |

## Consumption shape

### Rust (native or Cargo cross-target)

Each example is a standalone Cargo package with empty `[workspace]`
table — it does not participate in any walking-up workspace:

```toml
[package]
name = "native-rs-zenoh-talker"
edition = "2024"
publish = false

[[bin]]
name = "talker"
path = "src/main.rs"

[dependencies]
nros = { version = "*", default-features = false,
         features = ["std", "rmw-cffi", "ros-humble"] }
nros-rmw-zenoh = { version = "*",
                   features = ["std", "platform-posix", "ros-humble"] }

[workspace]
```

nano-ros crates are declared **registry-style** (phase-277 W6): they are
not on crates.io, so the example's tracked `.cargo/config.toml` carries
the `# nros-managed` `[patch.crates-io]` block resolving them into the
checkout. After adding/renaming nros deps or msg `<depend>` rows, re-run
`NROS_REPO_DIR=<repo root> nros sync` in the example dir and commit the
rewritten `.cargo/config.toml`. This is what makes the copy-out promise
real — a copied example re-runs `nros sync` at its new location.

`cargo build` / `cargo run` from inside the example directory is the
canonical invocation. There is no workspace-wide `cargo build` that
picks up examples — they are explicitly out-of-workspace.

### C / C++ (CMake)

Each example is a standalone CMake project that pulls nano-ros via
`add_subdirectory(<repo-root>)`. The canonical four-line
preamble:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_example LANGUAGES C CXX)

set(NANO_ROS_PLATFORM <plat>)
set(NANO_ROS_RMW      <rmw>)
set(NANO_ROS_BOARD    <board>)        # embedded only
# Phase-277 W6 standard root guard: cache var > NROS_REPO_DIR env > walk-up.
if(NOT DEFINED NANO_ROS_ROOT)
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        get_filename_component(NANO_ROS_ROOT
            "${CMAKE_CURRENT_SOURCE_DIR}/<rel-path-to-repo-root>" ABSOLUTE)
    endif()
endif()
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)

add_executable(my_example src/main.c)
target_link_libraries(my_example PRIVATE NanoRos::NanoRos)
nros_platform_link_app(my_example)
nano_ros_link_rmw(my_example RMW <rmw>)
```

`nano_ros_link_rmw` emits the strong-stub `nros_app_register_backends()`
that calls every linked RMW's `nros_rmw_<x>_register()` symbol — the
auto-registration path for targets where `linkme`'s distributed-slice
contribution isn't picked up by the linker (FreeRTOS, NuttX, Zephyr,
ESP-IDF).

There is no `find_package(NanoRos)` path deleted it along
with `just install-local`, every `install(...)` rule, and every
`Config.cmake.in` template.

## Per-example contents

```
examples/<plat>/<lang>/<example>/
├── package.xml                    # ROS-style manifest for the example
├── Cargo.toml | CMakeLists.txt    # Rust or C/C++ build entry
├── .cargo/config.toml             # Rust only — target + .cargo patches
├── src/                           # main.rs / main.c / main.cpp
├── generated/                     # codegen output for any custom msgs
└── README.md                      # usage instructions
```

Each example's `Cargo.toml` / `CMakeLists.txt` builds in isolation —
no workspace reliance, no path heuristics walking up the source tree.

## Message generation

Examples with custom `.msg`, `.srv`, or `.action` files generate
bindings in-tree under `generated/`. The `generated/` directory is
gitignored per-example (only `packages/interfaces/rcl-interfaces/`
generated bindings live in git Arduino bundle exception
aside).

```bash
source /opt/ros/humble/setup.sh        # for rosidl tooling
nros generate-rust            # or generate-c / generate-cpp / generate-all
```

For BSP / cross-target examples that maintain their own
`.cargo/config.toml`, pass `--config --nano-ros-path <relative>`:

```bash
nros generate-rust --config --nano-ros-path ../../../packages
```

The `--config` flag uses `ConfigPatcher` to idempotently add
`[patch.crates-io]` entries while preserving existing `[build]` /
`[target.*]` sections.

For CMake consumers:

```cmake
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)
nano_ros_generate_interfaces(... LANGUAGE C)
```

See [Message Generation](../user-guide/message-generation.md) for the
full reference + `package.xml` schema.

## Adding a new example — checklist

1. **Pick the canonical cell.** Confirm `<plat>/<lang>/<name>`
   isn't in the "intentionally empty" list in `examples/README.md`.
2. **Copy the nearest peer.** Identical-RMW + adjacent-platform is
   the lowest-risk template (e.g. copy `examples/qemu-arm-freertos/c/talker`
   to make a new FreeRTOS C/zenoh example).
3. **Update names + `package.xml`.** Rename `Cargo.toml`'s `name`
   and `[[bin]]` entries (Rust) or `project(...)` and `add_executable(...)`
   targets (CMake).
4. **Regenerate bindings.** Run `nros generate-rust-*` against
   the new `package.xml`. Custom messages need their own `package.xml`
   in the consuming example.
5. **Build standalone.** `cargo build` or
   `cmake -B build && cmake --build build` from the example directory.
   No walking-up workspace allowed.
6. **Wire the test fixture (optional).** If the example needs an E2E
   gate, add a builder in `packages/testing/nros-tests/src/fixtures/binaries/<plat>.rs`
   that runs `cargo build` / `cmake --build` and points the test at
   the resulting binary.
7. **Update `examples/README.md`** coverage matrix if you filled a
   previously-empty cell.

## Per-platform notes

| Platform | Source file shape | Build command | Notes |
|---|---|---|---|
| `native` | `src/main.rs`, `src/main.c`, `src/main.cpp` | `cargo run` / `cmake --build` | Full `std`. Pattern A or B. |
| `qemu-arm-baremetal` | `src/main.rs` with `#[entry]` | `cargo run` (`runner = qemu-system-arm …`) | No `std`. Pure Cortex-M3. |
| `qemu-arm-freertos` | `src/main.rs` / `src/main.cpp` / `src/main.c` | `cargo run` (Rust) or `cmake --build` (C/C++) | FreeRTOS kernel + lwIP. |
| `nuttx` | `src/main.rs` / `src/main.c` | `cmake --build` (NuttX export tarball) | NuttX kernel. |
| `threadx-linux` / `threadx-riscv64` | `src/main.rs` | `cmake --build` | ThreadX + NetX Duo. |
| `esp32` | `src/main.rs` | `cargo run` (esp-hal) | bare-metal `esp-hal`. |
| `zephyr` | `src/lib.rs` (staticlib) or `src/main.cpp` | `west build` | Kconfig + west module. |

Per-platform deep-dives — toolchain setup, Kconfig variables,
runner scripts — live in the [Platform Guides](../getting-started/).

## See Also

- [Build as a CMake subdirectory](../getting-started/build-as-subdirectory.md)
- [Message Generation](../user-guide/message-generation.md)
- [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md)
  — coverage matrix + intentionally-empty cells
- [`examples/templates/multi-package-workspace/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/multi-package-workspace)
  — Pattern A copy-out template
