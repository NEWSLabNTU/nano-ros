# Build as a CMake subdirectory

This is **the** way to integrate nano-ros into a C or C++ project. The
nano-ros repo ships a top-level `CMakeLists.txt` that exposes
everything via `add_subdirectory(...)`. No install step, no
`find_package(NanoRos)`, no install prefix removed every
last trace of that pipeline.

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
set(NANO_ROS_RMW     zenoh)    # zenoh | dds | xrce | cyclonedds

add_subdirectory(third_party/nano-ros nano_ros)

add_executable(my_app main.c)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
nros_platform_link_app(my_app)

# Optional — generate C bindings for ROS 2 .msg / .srv / .action files.
# nros_generate_interfaces() is reachable in-tree once nano-ros has been
# add_subdirectory'd; no install step required.
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)
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
| `NANO_ROS_PLATFORM` | `posix` | `posix`, `freertos` (`freertos_armcm3`), `nuttx` (`nuttx_armv7a`), `threadx` (`threadx_linux`, `threadx_riscv64`), `zephyr`, `baremetal` |
| `NANO_ROS_BOARD` | (unset) | required for `threadx` (`threadx-linux` or `riscv64-qemu`) and `baremetal` (`mps2-an385`, `stm32f4-nucleo`, …) |
| `NANO_ROS_RMW` | `zenoh` | `zenoh`, `dds`, `xrce`, `cyclonedds` |
| `NANO_ROS_ROS_EDITION` | `humble` | `humble`, `iron` |
| `NANO_ROS_BUILD_CODEGEN` | `ON` | `ON` / `OFF` |

Variables MUST be `set(...)` BEFORE `add_subdirectory(...)` — the
sub-project consumes them at include time.

## What about installing?

deleted every `install(...)` rule. nano-ros is consumed in
source form — never out of an installed prefix. If you need a
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
