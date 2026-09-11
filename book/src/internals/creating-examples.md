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

nano-ros crates are declared **registry-style**: they are not on crates.io, so
something has to resolve them into the checkout. That something is no longer a
file you write. Since [RFC-0098] an example carries **no `.cargo/` directory at
all**: `nros sync` writes the `[patch.crates-io]` rows, the board's cargo
settings and the derived `[env]` into `build/<image>/nros-cargo.toml`, and cargo
reads that file through `--config`. After adding or renaming nros deps or msg
`<depend>` rows, re-run `nros sync` in the example dir — and commit nothing,
because everything it wrote is build output. This is what makes the copy-out
promise real: a copied example re-runs `nros sync` at its new location.

[RFC-0098]: https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md

The canonical invocation from inside the example directory:

```bash
nros sync     # generated/ msg crates + build/<image>/nros-cargo.toml
nros build    # every declared image; or `nros build <image-id>`
```

Driving cargo yourself works too — hand it the generated settings file. Since
phase-445 W6 deleted the leaf's `.cargo/`, the working directory no longer
matters: there is nothing beside the package for cargo to read a second time,
and a tracked one is refused (`check-example-cargo-dirs`).

```bash
cargo build --manifest-path <leaf>/Cargo.toml \
            --config <leaf>/build/<image-id>/nros-cargo.toml
```

There is still no workspace-wide `cargo build` that picks up examples — they are
explicitly out-of-workspace.

### Multi-package workspace examples

A workspace example under `examples/workspaces/` has **no root `Cargo.toml` and
no root `CMakeLists.txt`** (RFC-0098 D9) — it is a directory of packages, like a
colcon workspace, and everything generated lives under `build/`, `dist/` and
`log/`. There is no entry package to write either: the entry is generated per
`[image.*]`, at `build/<coord>/<entry>/Cargo.toml` (cargo) or
`build/<coord>/CMakeLists.txt` (cmake). The bringup package's `system.toml`
carries the images. `nros sync` then `nros build` is the flow; a workspace with
no bringup at all builds every package in dependency order, each into its own
`build/<pkg>/`.

### The board, stated once — and the one place it is still stated twice

An example names its board in `system.toml` and nowhere else in its build
configuration:

```toml
[system]
name      = "my_talker"
rmw       = "zenoh"          # zenoh | cyclonedds | xrce
domain_id = 0

[[component]]
pkg   = "my_talker"
class = "my_talker::Talker"
name  = "talker"

[image.mps2]                 # an embedded image also carries its identity
board   = "qemu-mps2-an385"
locator = "tcp/10.0.2.2:10500"
ip      = "10.0.2.10"
gateway = "10.0.2.2"
netmask = "255.255.255.0"
```

The same schema serves a workspace bringup (`src/<name>_bringup/system.toml`),
so the resolver has one input shape either way. Retired by it, and refused
wherever they still appear: `[package.metadata.nros.entry]` (including
`deploy =`), `[package.metadata.nros.deploy.<board>]`,
`[package.metadata.nros.node]`, `[package.metadata.nros.component]`, and the
`package.xml` `<nano_ros deploy= board= rmw=/>` tuple.

**The exception a contributor will hit.** A single-package leaf *is* its own
entry, so it still names its board crate by hand in `[dependencies]` —
`nros-board-mps2-an385 = { version = "*" }` and so on. RFC-0098 D6 generates
that dependency for a *generated workspace entry* only. So editing
`[image.*] board` alone makes `nros sync` report success and `nros build` fail
inside the leaf's own crate with `cannot find nros_board_<old> in the crate
root`. Change both until
[issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1305-single-package-board-crate-dep-not-generated.md)
closes.

### C / C++ (CMake)

The canonical shape is an ament package — 152 of the 162 example
`CMakeLists.txt` files in the tree, and what a new one should copy. Verbatim
from `examples/native/c/talker/`:

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(c_talker src/main.c)
ament_target_dependencies(c_talker std_msgs)

install(TARGETS c_talker DESTINATION lib/${PROJECT_NAME})
ament_package()
```

No platform, no board, no RMW on the command line or in the file:
`find_package(nano_ros)` reads the example's `system.toml` through `nros ws
leaf-system` and derives the platform from the board it names.
`-DNANO_ROS_PLATFORM`, `-DNANO_ROS_BOARD` and `-DNROS_RMW` are retired as the
way a user chooses. A cross build still passes `-DCMAKE_TOOLCHAIN_FILE` by hand
(or uses `nros init` + `cmake --preset <board>`), because `nros build` — the
verb that maps a board to its toolchain file — does not yet work in a
single-package C/C++ leaf
([issue 1296](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1296-nros-build-c-leaf-bringup-name-mismatch.md)).

A single-package C/C++ leaf needs **no `nros sync`**: its message bindings are a
CMake-time output, so a clean copy outside the checkout configures and builds
with no sync at all.

The other shape — a standalone CMake project pulling nano-ros in by path with
`add_subdirectory(<repo-root>)`, setting `NANO_ROS_PLATFORM` / `NANO_ROS_RMW` /
`NANO_ROS_BOARD` itself, linking `NanoRos::NanoRos` and calling
`nros_platform_link_app()` + `nano_ros_link_rmw()` — is what the ThreadX Rust
leaves and the `examples/templates/` recipes use, and is the right shape for an
out-of-tree consumer that owns its whole `CMakeLists.txt`. Its cache-variable
contract is in
[`docs/reference/c-api-cmake.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/reference/c-api-cmake.md).

`nano_ros_link_rmw` emits the strong-stub `nros_app_register_backends()`
that calls every linked RMW's `nros_rmw_<x>_register()` symbol — the
auto-registration path for targets where `linkme`'s distributed-slice
contribution isn't picked up by the linker (FreeRTOS, NuttX, Zephyr,
ESP-IDF).

Note `find_package(nano_ros)` above is the **ament package**. There is still no
`find_package(NanoRos)` CMake-config export — that was deleted along with
`just install-local`, every `install(...)` rule, and every `Config.cmake.in`
template.

## Per-example contents

```
examples/<plat>/<lang>/<example>/
├── system.toml                    # board, RMW, domain, components, net identity
├── package.xml                    # ROS-style manifest for the example
├── Cargo.toml | CMakeLists.txt    # language-toolchain facts only
├── src/                           # main.rs / main.c / main.cpp
├── generated/                     # codegen output — `nros sync`, gitignored
├── build/                         # `nros sync` / `nros build` output, gitignored
└── README.md                      # usage instructions
```

There is **no `.cargo/`**. `Cargo.toml` and `CMakeLists.txt` carry
language-toolchain facts; every build setting the board implies is generated
under `build/`. Each example's `Cargo.toml` / `CMakeLists.txt` builds in
isolation — no workspace reliance, no path heuristics walking up the source
tree.

## Message generation

Examples with custom `.msg`, `.srv`, or `.action` files generate
bindings in-tree under `generated/`. The `generated/` directory is
gitignored per-example (only `packages/interfaces/rcl-interfaces/`
generated bindings live in git Arduino bundle exception
aside).

```bash
source /opt/ros/humble/setup.sh        # for rosidl tooling
nros sync                              # what you run in an example
```

`nros generate-rust` (and `generate-c` / `generate-cpp` / `generate-all`) is the
codegen-only **primitive** underneath it: it emits `generated/` and nothing
else. `nros sync` is the verb an example needs, because it also writes the
image's `build/<image>/nros-cargo.toml`.

Its `--generate-config` / `--config` / `--nano-ros-path` / `--nano-ros-git`
flags still parse, and are retired as a way to configure an example: they wrote
a leaf `.cargo/config.toml`, which no longer exists. (`--nano-ros-path` is a
documented no-op on `nros sync` itself, kept for back-compat.)

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
   the lowest-risk template (e.g. copy `examples/mps2-an385-freertos/c/talker`
   to make a new FreeRTOS C/zenoh example).
3. **Update names + `package.xml`.** Rename `Cargo.toml`'s `name`
   and `[[bin]]` entries (Rust) or `project(...)` and
   `nano_ros_add_executable(...)` targets (CMake).
4. **Point `system.toml` at your board.** `[system] name`, the `[[component]]`
   rows (`pkg`, `class`, `name`), and `[image.<id>] board` plus any network
   identity the board needs (`locator`, `ip`, `gateway`, `netmask`). A Rust
   leaf also needs its `[dependencies]` board crate changed to match — see the
   exception above. If the board cannot be host-probed (a foreign `[build]
   target`, `build-std`, or a board crate with no host build), declare the
   component's `entities` here too.
5. **Sync.** `nros sync` in the example directory — generated message crates
   plus `build/<image>/nros-cargo.toml`. Custom messages need their own
   `package.xml` in the consuming example. A single-package C/C++ leaf can skip
   this; everything else cannot, and `nros build` refuses with one line naming
   it.
6. **Build standalone.** `nros build` from the example directory (C/C++
   single-package leaves: `cmake -B build && cmake --build build`). No
   walking-up workspace allowed.
7. **Wire the test fixture (optional).** If the example needs an E2E
   gate, add a row to `examples/fixtures.toml` and the matching cell in
   `matrix::CELLS`; the builder lives in
   `packages/testing/nros-tests/src/fixtures/binaries/<plat>.rs`.
8. **Update `examples/README.md`** coverage matrix if you filled a
   previously-empty cell.

## Per-platform notes

The build command is `nros build` (Rust and workspaces) or `cmake --build`
(single-package C/C++) almost everywhere; the table records what differs.

| Platform | Source file shape | Build | Notes |
|---|---|---|---|
| `native` | `src/main.rs`, `src/main.c`, `src/main.cpp` | `nros build` / `cmake --build` | Full `std`. |
| `mps2-an385-baremetal` | `src/main.rs` with `#[entry]` | `nros build` | No `std`. Pure Cortex-M3. The board's QEMU `runner` is in the generated settings, so `cargo run --config …` boots it. |
| `mps2-an385-freertos` | `src/main.rs` / `src/main.cpp` / `src/main.c` | `nros build` (Rust) or `cmake --build` (C/C++) | FreeRTOS kernel + lwIP. |
| `nuttx` | `src/main.rs` / `src/main.c` | `cmake --build` (NuttX export tarball) | NuttX kernel. |
| `threadx-linux` / `threadx-riscv64` | `src/main.rs` | `cmake --build` | ThreadX + NetX Duo. |
| `esp32` | `src/main.rs` | `nros build` + `espflash save-image` | bare-metal `esp-hal`; no cargo `runner`. |
| `zephyr` | `src/lib.rs` (staticlib) or `src/main.cpp` | `west build` (after `nros sync`) | Kconfig + west module — the one carve-out where `nros build` is not the verb. |

Artifacts land under `<leaf>/build/<image-id>/target/[<triple>/]<profile>/<bin>`:
`examples/native/rust/talker` gives `build/native/target/debug/talker`,
`examples/mps2-an385-baremetal/rust/talker` gives
`build/mps2-an385-baremetal/target/thumbv7m-none-eabi/debug/qemu-bsp-talker`.

Per-platform deep-dives — toolchain setup, Kconfig variables,
runner scripts — live in the [Platform Guides](../getting-started/).

## See Also

- [Build as a CMake subdirectory](../getting-started/build-as-subdirectory.md)
- [Message Generation](../user-guide/message-generation.md)
- [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md)
  — coverage matrix + intentionally-empty cells
- [`examples/templates/multi-package-workspace/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/multi-package-workspace)
  — Pattern A copy-out template
