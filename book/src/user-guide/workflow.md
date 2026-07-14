# Application Workflow

nano-ros users usually want one path: prepare package, write node,
build it, then deploy to target. Use Concepts only when workflow raises
a technical question.

## 1. Prepare Workspace and Package

nano-ros is shipped source-only — vendor it next to (or inside) your
workspace, then provision the board's toolchain with `nros setup`:

```bash
# Build the in-tree nros CLI (Phase 218), then provision your board (+ RMW):
./scripts/bootstrap.sh base
source ./activate.sh        # OR: direnv allow / source ./activate.fish
nros setup native --rmw zenoh        # or qemu-arm-freertos, zephyr, …
```

`nros setup` ships prebuilt toolchains per platform per RMW — see
[Installation](../getting-started/installation.md).

For multi-package workspaces (Pattern A — recommended for POSIX +
mixed C / C++ / Rust deployments), put nano-ros and your packages
side-by-side under a shared `src/`:

```text
~/ros2_ws/
├── src/
│   ├── nano-ros/                  # this repo
│   └── my_robot_node/
│       ├── package.xml
│       ├── Cargo.toml             # Rust — deps + [package.metadata.nros.*]
│       ├── CMakeLists.txt         # C/C++ — targets (deploy tuple in package.xml)
│       └── src/
└── build/ install/ log/           # if you use colcon
```

For third-party C/C++ projects without colcon (Pattern B), pull
nano-ros in as a `third_party/nano-ros/` git submodule and consume
it via `add_subdirectory(third_party/nano-ros nano_ros)`.

See [Installation](../getting-started/installation.md) for both
patterns. The per-language starter pages document the canonical
package shape in their "Project layout" sections:
[Rust](../getting-started/first-node-rust.md),
[C](../getting-started/first-node-c.md),
[C++](../getting-started/first-node-cpp.md).
For two or more nodes, use the
[Multi-Node Projects](../getting-started/workspace-from-app-node.md)
group: start from the full project layout, then drill into Node,
Bringup, and Entry packages.

## 2. Write Node Code

Choose API language first:

- **Rust** — use `nros`, generated message crates, and `Executor`.
- **C** — include `nros/nros.h` and generate interfaces with
  `nros_find_interfaces(LANGUAGE C)` in CMake.
- **C++** — include `nros/nros.hpp` and use typed wrappers.

Start with one of the Linux starters above, then adapt to your
target via the [Embedded Starters](../getting-started/freertos.md)
section.

## 3. Generate Messages

If you use custom `.msg`, `.srv`, or `.action` files, generate bindings
inside the workspace/build tree. See
[Message Binding Generation](message-generation.md).

## 4. Configure Target

Pick a platform and an RMW backend at build time (compile-time
choice — there is no `RMW_IMPLEMENTATION` runtime switch on embedded
targets):

| CMake side | Cargo side |
|---|---|
| `set(NANO_ROS_PLATFORM <plat>)` | feature `platform-<plat>` |
| `set(NANO_ROS_RMW <rmw>)` | feature `rmw-<rmw>-cffi` (transports auto-pull) |
| `set(NANO_ROS_BOARD <board>)` (optional, embedded only) | `nros-board-<board>` dep |

Supported pairs: `posix / freertos / nuttx / threadx / zephyr / esp32 /
baremetal` × `zenoh / xrce / dds / cyclonedds`. Not every cell is
implemented — see the [Coverage Matrix](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md#coverage-matrix).

Runtime configuration (`ROS_DOMAIN_ID`, `NROS_LOCATOR`, …) works on
POSIX. Embedded targets bake config from
`[package.metadata.nros.deploy.<t>]` (Rust) / the package.xml
`<nano_ros deploy=…/>` tuple (C/C++), plus
Kconfig on Zephyr. See [Configuration](configuration.md).

## 5. Build, Test, Deploy

For each canonical entry point:

```bash
# Single example (POSIX, Pattern A or B):
cd examples/native/rust/talker
cargo run

# Single C/C++ example (CMake + add_subdirectory):
cd examples/qemu-arm-freertos/cpp/talker
cmake -B build -DCMAKE_TOOLCHAIN_FILE=$PWD/../../../../../cmake/toolchain/arm-freertos-armcm3.cmake
cmake --build build

# Per-platform multi-example build:
just freertos build-fixtures
just zephyr  build-fixtures
just nuttx   build-fixtures

# Discover full-matrix commands for a platform:
just --group full-matrix --list zephyr

# Multi-component system (orchestration):
nros metadata my_system
nros plan my_system launch/my_system.launch.py
nros check
cargo build                 # or: cmake --build / west build / idf.py build

# POSIX-only colcon consumer-workspace build:
colcon build && source install/setup.bash
```

For target-specific deployment, go to the matching platform guide.
Each guide covers toolchain setup, package layout, code example, build
command, run/flash command, and deployment notes.

See [Deployment Workflow](deployment.md).
