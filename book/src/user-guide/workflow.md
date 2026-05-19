# Application Workflow

nano-ros users usually want one path: prepare package, write node,
build it, then deploy to target. Use Concepts only when workflow raises
a technical question.

## 1. Prepare Workspace and Package

nano-ros is shipped source-only. Clone it next to (or inside) your
workspace and run `just setup`:

```bash
git clone --branch=v<X.Y.Z> https://github.com/NEWSLabNTU/nano-ros.git
cd nano-ros
just setup tier=default      # Phase 142 SDK tiers: minimal | default | extended
```

For multi-package workspaces (Pattern A — recommended for POSIX +
mixed C / C++ / Rust deployments), put nano-ros and your packages
side-by-side under a shared `src/`:

```text
~/ros2_ws/
├── src/
│   ├── nano-ros/                  # this repo
│   └── my_robot_node/
│       ├── package.xml
│       ├── Cargo.toml             # Rust, if using Rust API
│       ├── CMakeLists.txt         # C/C++, if using CMake
│       ├── config.toml            # embedded targets
│       └── src/
└── build/ install/ log/           # if you use colcon
```

For third-party C/C++ projects without colcon (Pattern B), pull
nano-ros in as a `third_party/nano-ros/` git submodule and consume
it via `add_subdirectory(third_party/nano-ros nano_ros)`.

See [Installation](../getting-started/installation.md) for both
patterns + [Package Preparation](package-preparation.md) for the
per-package details.

## 2. Write Node Code

Choose API language first:

- **Rust** — use `nros`, generated message crates, and `Executor`.
- **C** — include `nros/nros.h` and generate interfaces with
  `nros_find_interfaces(LANGUAGE C)` in CMake.
- **C++** — include `nros/nros.hpp` and use typed wrappers.

Start with [First Native Rust Node](../getting-started/native.md),
then adapt to C or C++ via the API references.

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

Runtime configuration (`ROS_DOMAIN_ID`, `ZENOH_LOCATOR`, …) works on
POSIX. Embedded targets resolve config via CMake cache vars, Kconfig
(Zephyr), Cargo features, or `config.toml`. See
[Configuration](configuration.md).

## 5. Build, Test, Deploy

For each canonical entry point:

```bash
# Single example (POSIX, Pattern A or B):
cd examples/native/rust/zenoh/talker
cargo run

# Single C/C++ example (CMake + add_subdirectory):
cd examples/qemu-arm-freertos/cpp/zenoh/talker
cmake -B build -DCMAKE_TOOLCHAIN_FILE=$PWD/../../../../../cmake/toolchain/arm-freertos-armcm3.cmake
cmake --build build

# Per-platform multi-example build:
just freertos build-fixtures
just zephyr  build-fixtures
just nuttx   build-fixtures

# Multi-component system (Phase 126 orchestration):
nros metadata my_system
nros plan my_system launch/my_system.launch.py
nros check
nros build

# POSIX-only colcon consumer-workspace build:
colcon build && source install/setup.bash
```

For target-specific deployment, go to the matching platform guide.
Each guide covers toolchain setup, package layout, code example, build
command, run/flash command, and deployment notes.

See [Deployment Workflow](deployment.md).
