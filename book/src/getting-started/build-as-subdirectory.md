# Build as a CMake subdirectory

This is one of the two ways to integrate nano-ros into a C or C++
project, and the lower-level one: the repo's top-level `CMakeLists.txt`
exposes everything via `add_subdirectory(...)`. The higher-level,
ament-shaped alternative is `find_package(nano_ros REQUIRED)` —
`nano_rosConfig.cmake` at the checkout root, located via
`nano_ros_ROOT` (RFC-0048) — which every in-tree example now uses.
Either way there is NO install step: phase-140 removed the install
prefix and the old `find_package(NanoRos)` (capital) config pipeline.
Vendoring the repo into a company tree (pinning, offline CI, upgrade
workflow) has its own page:
[Integrating into a Vendored Tree](vendored-tree.md).

## Layout

```
my_app/
├── CMakeLists.txt
├── main.c
└── third_party/
    └── nano-ros/        # git clone / git submodule of this repo
```

## User project `CMakeLists.txt`

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_app C)

# Pick platform + RMW BEFORE add_subdirectory.
set(NANO_ROS_PLATFORM posix)   # posix | freertos | nuttx | threadx | zephyr | baremetal
set(NANO_ROS_RMW     zenoh)    # zenoh | xrce | cyclonedds

add_subdirectory(third_party/nano-ros nano_ros)

add_executable(my_app main.c)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
if(COMMAND nros_platform_link_app)   # defined by zephyr/threadx platforms only
    nros_platform_link_app(my_app)
endif()

# Optional — generate C bindings for ROS 2 .msg / .srv / .action files.
# (LANGUAGE defaults to CPP; full spelling table → user-guide
# "Message Generation".)
nano_ros_generate_interfaces(std_msgs LANGUAGE C
    DEPENDENCIES builtin_interfaces)
target_link_libraries(my_app PRIVATE std_msgs__nano_ros_c)
```

That's the whole story for the host-POSIX / zenoh case. CMake's
transitive target propagation pulls in `libnros_c.a`,
`libnros_rmw_zenoh_staticlib.a`, the POSIX platform shim, system
libraries (`pthread`, `dl`, `m`), and the per-build
`nros_config_generated.h` header automatically.

## Cache variables

| Variable | Default | Values |
|----------|---------|--------|
| `NANO_ROS_PLATFORM` | `posix` | `posix`, `freertos`, `nuttx`, `threadx`, `esp_idf` (legacy spellings: `freertos_armcm3`, `nuttx_armv7a`, `threadx_linux`, `threadx_riscv64`). Zephyr and bare-metal are not add_subdirectory platforms — they enter via the [Zephyr module](integration-zephyr.md) / cargo directly |
| `NANO_ROS_BOARD` | (unset) | required for `threadx` (`threadx-linux` or `riscv64-qemu`) |
| `NANO_ROS_SKIP_BOOTSTRAP` | `OFF` | `ON` skips the configure-time bootstrap (git submodule updates + a possible network FetchContent of Corrosion). **Required for air-gapped/mirrored CI** — pre-seed submodules and install Corrosion first (`nros setup --tool corrosion`) |
| `NANO_ROS_FEATURES` | (empty) | extra cargo features forwarded to the Rust build |
| `NANO_ROS_RMW` | `zenoh` | `zenoh`, `dds`, `xrce`, `cyclonedds` |
| `NANO_ROS_ROS_EDITION` | `humble` | `humble`, `iron`, `jazzy` |
| `NANO_ROS_BUILD_CODEGEN` | `ON` | `ON` / `OFF` |

Variables MUST be `set(...)` BEFORE `add_subdirectory(...)` — the
sub-project consumes them at include time.

## What about installing?

Phase-140 deleted every nano-ros-side `install(...)` rule. nano-ros is
consumed in source form — never out of an installed prefix. If you need a
shippable artefact, your *user project* owns the install layout; ship
your binary, not nano-ros itself.

For RTOS users who want a more idiomatic surface than raw
`add_subdirectory`, see the integration shells under
`integrations/<rtos>/` — they translate west / esp-idf / NuttX / PX4
manifests into the same root CMake. Each shell is a
~20-line wrapper around `add_subdirectory(<repo>)`.

## Worked example

The `examples/native/c/talker/CMakeLists.txt` is the canonical
copy-out template: it resolves the nano-ros checkout root once
(`-DNANO_ROS_ROOT` cache var → `NROS_REPO_DIR` env var → in-repo
walk-up), includes the workspace helpers, generates the message
bindings (`nros_find_interfaces`), and declares the app via
`nano_ros_entry(...)` — ~55 lines including codegen + per-app fixup.
All in-tree C/C++ examples follow the same shape.
