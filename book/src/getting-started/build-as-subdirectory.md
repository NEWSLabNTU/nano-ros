# Build as a CMake subdirectory

**This is the escape hatch, not the path.** If your project is a
nano-ros package or workspace, you state the board once in
`system.toml` and let `nros sync` + `nros build` drive CMake — you
never set a cache variable by hand, because the build root it generates
sets them from that one board name (RFC-0098). Start at
[First Project](first-project.md) and
[Project layout](workspace-from-app-node.md) instead.

This page is for the C or C++ project that is **not** a nano-ros
workspace: an existing CMake tree with its own layout and its own
conventions, that wants nano-ros as a subdirectory and nothing else.
There the cache variables below are how you say what you are building
for, because there is no `system.toml` to read it from. The mechanism
is unchanged by RFC-0098 — `nros build` sets exactly these variables in
the root it generates.

The repo's top-level `CMakeLists.txt` exposes everything via
`add_subdirectory(...)`. The higher-level, ament-shaped alternative is
`find_package(nano_ros REQUIRED)` — `nano_rosConfig.cmake` at the
checkout root, located via `nano_ros_ROOT` (RFC-0048) — which every
in-tree example reaches through the workspace helpers. Either way there
is NO install step: phase-140 removed the install prefix and the old
`find_package(NanoRos)` (capital) config pipeline. Vendoring the repo
into a company tree (pinning, offline CI, upgrade workflow) has its own
page: [Integrating into a Vendored Tree](vendored-tree.md).

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
| `NANO_ROS_BOARD` | (unset) | required for `threadx` (`threadx-linux` or `rv-virt-threadx`) |
| `NANO_ROS_SKIP_BOOTSTRAP` | `OFF` | `ON` skips the configure-time bootstrap (git submodule updates + a possible network FetchContent of Corrosion). **Required for air-gapped/mirrored CI** — pre-seed submodules and install Corrosion first (`nros setup --tool corrosion`) |
| `NANO_ROS_FEATURES` | (empty) | extra cargo features forwarded to the Rust build |
| `NANO_ROS_RMW` | `zenoh` | `zenoh`, `dds`, `xrce`, `cyclonedds` |
| `NANO_ROS_ROS_EDITION` | `humble` | `humble`, `iron`, `jazzy` |
| `NANO_ROS_BUILD_CODEGEN` | `ON` | `ON` / `OFF` |

Variables MUST be `set(...)` BEFORE `add_subdirectory(...)` — the
sub-project consumes them at include time.

In a nano-ros package or workspace none of these are yours to set. The
CMake root `nros build` generates writes `NANO_ROS_PLATFORM` and
`NANO_ROS_BOARD` itself, both derived from the one `[image.<id>] board`
you named in `system.toml`, and passes the backend from `[system] rmw`
through the workspace helper. Setting them by hand there means two
answers to one question, which is the arrangement RFC-0098 exists to
end.

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

## What the in-tree examples do instead

Worth reading as the contrast, because it is the shape most projects
want. `examples/native/c/talker/CMakeLists.txt` is fourteen lines and
sets no cache variable at all:

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker LANGUAGES C CXX)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(c_talker src/main.c)
ament_target_dependencies(c_talker std_msgs)
```

The board, the RMW, the domain and the network identity are not in
that file — they are in the `system.toml` beside it, and the build
reads them from there. A single-package C/C++ leaf like this one
configures and builds with plain CMake, no sync step required, because
its message bindings are a CMake-time output:

```bash
cmake -B build
cmake --build build
```

(`nros build` does not yet drive a single-package C/C++ leaf — the
synthesised bringup is named after the directory while the driver asks
for `[system] name`. That is
[issue 1296](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1296-nros-build-c-leaf-bringup-name-mismatch.md);
use the two CMake commands above until it closes. A C/C++ *workspace*
builds with `nros sync` + `nros build` as usual.)

All in-tree C/C++ examples follow that shape. The `add_subdirectory`
mechanism on this page sits underneath it.
