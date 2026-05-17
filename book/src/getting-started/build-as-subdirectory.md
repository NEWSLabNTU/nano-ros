# Build as a CMake subdirectory

This is the recommended way to integrate nano-ros into a new C or C++
project. The nano-ros repo ships a top-level `CMakeLists.txt`
(Phase 137) that exposes everything via `add_subdirectory(...)` — no
`just install-local`, no `find_package(NanoRos CONFIG)`, no install
prefix.

The legacy `find_package(NanoRos CONFIG)` workflow is documented at
[installation.md](installation.md) and stays supported until Phase
140 retires it.

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
set(NANO_ROS_PLATFORM posix)   # posix is the only fully wired Phase 137 path
set(NANO_ROS_RMW     zenoh)    # zenoh | dds  (xrce | cyclonedds land in Phase 138)

add_subdirectory(third_party/nano-ros nano_ros)

add_executable(my_app main.c)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)

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
| `NANO_ROS_PLATFORM` | `posix` | `posix`, `freertos_armcm3`, `nuttx_armv7a`, `threadx_linux`, `threadx_riscv64` |
| `NANO_ROS_RMW` | `zenoh` | `zenoh`, `dds`, `xrce`*, `cyclonedds`* |
| `NANO_ROS_ROS_EDITION` | `humble` | `humble`, `iron` |
| `NANO_ROS_BUILD_CODEGEN` | `ON` | `ON` / `OFF` |
| `NANO_ROS_FORCE_INSTALL` | `OFF` | `ON` / `OFF` — emit `install(...)` rules even when consumed via `add_subdirectory` |

`*` Phase 137 wires `zenoh` + `dds` natively. Selecting `xrce` or
`cyclonedds` from in-tree mode surfaces a clear fatal-error pointing
back at the legacy install-local workflow until Phase 138 / 139 close
the gap.

Variables MUST be `set(...)` BEFORE `add_subdirectory(...)` — the
sub-project consumes them at include time.

## Install rules

When nano-ros is the top-level project (`cmake -S nano-ros -B build`),
the existing `install(...)` rules emit the legacy install layout under
`<prefix>/lib`, `<prefix>/include`, `<prefix>/lib/cmake/NanoRos`. When
nano-ros is consumed via `add_subdirectory`, those rules are gated on
`PROJECT_IS_TOP_LEVEL` and stay inert — the user project owns install
layout. Override with `-DNANO_ROS_FORCE_INSTALL=ON` if you need the
legacy install layout produced from inside a user project.

## What Phase 137 does not yet cover

- **Per-platform RTOS shells** (Zephyr `west` module, ESP-IDF
  component, PlatformIO `library.json`, NuttX `apps/external`) —
  Phase 139.
- **Per-platform support modules consolidation** (current
  `packages/core/nros-c/cmake/nros-{threadx,freertos,nuttx}.cmake`
  move to `cmake/platform/nano-ros-<plat>.cmake`) — Phase 138.
- **`install-local` removal** — Phase 140.

For RTOS targets today, keep using the platform-specific drivers
(`just zephyr ...`, `just freertos ...`, `just nuttx ...`,
`just threadx_linux ...`) plus the legacy
[installation.md](installation.md) flow.

## Worked example

The `examples/native/c/zenoh/talker` example was migrated to the
in-tree path in Phase 137.5; its `CMakeLists.txt` is ~16 lines and is
the canonical copy-out template. The sibling
`examples/native/c/zenoh/*` examples remain on the legacy
`find_package(NanoRos CONFIG)` path until Phase 138.
