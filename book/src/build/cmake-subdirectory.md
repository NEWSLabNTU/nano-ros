# Build as a CMake subdirectory

**This is the escape hatch, not the path.** If your project is a
nano-ros package or workspace, you state the board once in
the bringup's `system.toml` and let `nros build` drive CMake — you
never set a cache variable by hand, because the build root it generates
sets them from that one board name. Start at
[First Project](../get-started/first-project.md) and
[Growing your workspace](growing-your-workspace.md) instead.

This page is for the C or C++ project that is **not** a nano-ros
workspace: an existing CMake tree with its own layout and its own
conventions, that wants nano-ros as a subdirectory and nothing else.
There the cache variables below are how you say what you are building
for, because there is no `system.toml` to read it from. `nros build`
sets exactly these variables in the root it generates.

The SDK root's top-level `CMakeLists.txt` (the SDK root is the
`share/nano-ros` directory of the installed release,
`nros locate nano-ros` prints it) exposes everything via `add_subdirectory(...)`. The
higher-level, ament-shaped alternative is
`find_package(nano_ros REQUIRED)` — `nano_rosConfig.cmake` at the
SDK root, located via `-Dnano_ros_ROOT=<root>`, which
`nros build` and the CMakePresets `nros setup presets` writes pass for you (a raw
`cmake` names it; no shell exports it) — which every in-tree example
reaches through the workspace helpers. Either way there
is NO install step: there is no install prefix and no
`find_package(NanoRos)` (capital) config pipeline. Pinning a release
for a company tree (offline CI, upgrade workflow) has its own page:
[Integrating into a Vendored Tree](vendored-tree.md).

## Layout

```
my_app/
├── CMakeLists.txt
└── main.c
```

nano-ros itself is the installed SDK root; the configure names it:

```bash
cmake -B build -DNANO_ROS_SDK="$(nros locate nano-ros)"
cmake --build build
```

## User project `CMakeLists.txt`

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_app C)

# Pick platform + RMW BEFORE add_subdirectory.
set(NROS_PLATFORM posix)   # posix | freertos | nuttx | threadx | zephyr | baremetal
set(NROS_RMW     zenoh)    # zenoh | xrce | cyclonedds

add_subdirectory("${NANO_ROS_SDK}" nano_ros)

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
| `NROS_PLATFORM` | `posix` | `posix`, `freertos`, `nuttx`, `threadx` (legacy spellings: `freertos_armcm3`, `nuttx_armv7a`, `threadx_linux`, `threadx_riscv64`). `esp_idf` is no longer accepted and is REJECTED. Zephyr and bare-metal are not add_subdirectory platforms — they enter via the [Zephyr module](../targets/zephyr.md) / cargo directly |
| `NROS_BOARD` | (unset) | required for `threadx` (`threadx-linux` or `rv-virt-threadx`) |
| `NROS_SKIP_BOOTSTRAP` | `OFF` | `ON` skips the configure-time submodule bootstrap — which only a source checkout has; an installed SDK root takes its sources from `nros setup`. Corrosion is never fetched: a configure without it in the SDK store stops and names `nros setup --tool corrosion` |
| `NROS_FEATURES` | (empty) | extra cargo features forwarded to the Rust build |
| `NROS_RMW` | `zenoh` | `zenoh`, `dds`, `xrce`, `cyclonedds` |
| `NROS_ROS_EDITION` | `humble` | `humble`, `iron`, `jazzy` |
| `NROS_BUILD_CODEGEN` | `ON` | `ON` / `OFF` |

Variables MUST be `set(...)` BEFORE `add_subdirectory(...)` — the
sub-project consumes them at include time.

In a nano-ros package or workspace none of these are yours to set. The
CMake root `nros build` generates writes `NROS_PLATFORM` and
`NROS_BOARD` itself, both derived from the one `[image.<id>] board`
you named in `system.toml`, and passes the backend from `[system] rmw`
through the workspace helper. Setting them by hand there means two
answers to one question.

## What about installing?

nano-ros has no `install(...)` rules. It is
consumed in source form — never out of an installed prefix. If you need a
shippable artefact, your *user project* owns the install layout; ship
your binary, not nano-ros itself.

For RTOS users who want a more idiomatic surface than raw
`add_subdirectory`, see the integration shells under
`integrations/<rtos>/` in the SDK root — they translate NuttX / PX4
manifests into the same root CMake. Each shell is a
~20-line wrapper around `add_subdirectory(<sdk root>)`.

## What the in-tree examples do instead

Worth reading as the contrast, because it is the shape most projects
want. The C node package
`nros new c_talker_ws --board linux --template talker --lang c` scaffolds is
a dozen lines and sets no cache variable at all:

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker_node LANGUAGES C)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(talker_lib STATIC src/Talker.c)

nros_components_register_node(talker_lib
    PLUGIN c_talker_node::Talker
    EXECUTABLE c_talker_node
    SHAPE configure
    TYPED)
```

The board, the RMW, the domain and the network identity are not in
that file — they are in the bringup's `system.toml`, and the build reads
them from there. `nros build` generates the CMake root that adds the
package and the image's entry, under `build/`, and passes the board's
toolchain file; its message bindings are a CMake-time output.

All the C/C++ examples follow that shape. The `add_subdirectory`
mechanism on this page sits underneath it.
