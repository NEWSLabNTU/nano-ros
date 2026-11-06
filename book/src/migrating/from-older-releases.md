# Migrating from older releases

<!-- type: how-to · level: 3 -->

**Needs first:** [Build inside an existing CMake or vendor tree](../guide/build/existing-cmake-or-vendor-tree.md)

You have a project written against an older nano-ros release, and something
it relies on is gone: the installed CMake prefix, a configuration file, or a
board. Each section below is one such change: what was removed, what replaces
it, and the edit that moves your project over. For a single renamed variable,
board or command, look it up in [Retired names](retired-names.md).

## Before you start

- Install the current release ([Install](../start/install.md)), and keep the
  old project's build directory out of the way: a cache an older release wrote
  can hold names that are now refused.
- Know how your project consumes nano-ros: a CMake tree, a `system.toml`
  workspace, or a hand-written `no_std` application.

## Move off the `install-local` prefix

nano-ros no longer has a CMake install step. The `install-local` recipe, the
`build/install/` prefix, its `install(...)` rules and `Config.cmake.in`
templates, and the `find_package(NanoRos)` path that consumed them are gone. A
CMake project adds the installed SDK root with `add_subdirectory`.

1. Replace the `find_package` call. Before:

   ```cmake
   cmake_minimum_required(VERSION 3.16)
   project(my_app C)

   find_package(NanoRos REQUIRED CONFIG)
   nano_ros_generate_interfaces(std_msgs SKIP_INSTALL)
   add_executable(my_app src/main.c)
   target_link_libraries(my_app PRIVATE std_msgs__nano_ros_c NanoRos::NanoRos)
   ```

   After:

   ```cmake
   cmake_minimum_required(VERSION 3.22)
   project(my_app C)

   set(NROS_PLATFORM posix)
   set(NROS_RMW     zenoh)
   add_subdirectory("${NANO_ROS_SDK}" nano_ros)

   add_executable(my_app src/main.c)
   target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
   nano_ros_generate_interfaces(std_msgs LANGUAGE C
       DEPENDENCIES builtin_interfaces)
   target_link_libraries(my_app PRIVATE std_msgs__nano_ros_c)
   ```

2. Configure with the SDK root instead of an install prefix:

   ```sh
   cmake -B build -DNANO_ROS_SDK="$(nros locate nano-ros)"
   cmake --build build
   ```

The other pieces of the old shape map as follows:

| Before | Now |
| --- | --- |
| `cmake -DCMAKE_PREFIX_PATH=<nano-ros>/build/install` | `-DNANO_ROS_SDK="$(nros locate nano-ros)"` |
| `nano_ros_link_platform(target)`, `nano_ros_link_rmw(target)` | nothing: `NanoRos::NanoRos` links both |
| `nros_freertos_compose_platform(target)`, `nros_threadx_compose_platform(target)` | `nros_platform_link_app(target)`, the one per-image link fixup |
| `find_package(NrosRmwCyclonedds CONFIG REQUIRED)` | nothing: `NROS_RMW=cyclonedds` wires it |
| `cmake_minimum_required(VERSION 3.16)` | `3.22`, as nano-ros's own CMake |
| a prebuilt `libnros_c.a` under the prefix | none: each project builds the library in its own build tree |

A Zephyr application takes nano-ros as a Zephyr module instead
([A Zephyr app you build with west](../guide/platforms/zephyr-west-app.md)).
NuttX and PX4 have their own integration paths
([NuttX](../guide/platforms/nuttx.md), [PX4](../guide/platforms/px4.md)).

## Replace a retired configuration file

These files used to carry configuration and no longer do. If a guide, an old
project or a blog post tells you to write one, its content belongs elsewhere:

| File | Now |
| --- | --- |
| a workspace-root `nros.toml` | `system.toml`, an image's settings under `[image.<id>]`. The CLI refuses a workspace-root `nros.toml`. Migrate it with the `nros-v0.5.0` release's `nros migrate workspace`; newer releases do not carry that command. |
| a per-package `nros.toml` overlay | `system.toml`. The overlay is still read, as a deprecated fallback, and `nros check` reports it. |
| `config.toml` with `[network]`, `[zenoh]`, `[scheduling]` or `[[transport]]` | `system.toml`, or for a hand-written `no_std` application with no code generation, the standalone `config.toml` of [Configure an image](../guide/build/configure-an-image.md). |

1. Move each setting into the `[image.<id>]` table it configures, keeping its
   key where `system.toml` has the same key. The
   [`system.toml` keys](../reference/system-toml.md) reference lists them.
2. Delete the old file, and rebuild with `nros build`.

## ESP32 boards

nano-ros no longer supports the ESP32. Both ways it once ran there are gone:

- **The ESP32-C3 bare-metal board, over `esp-hal`**, retired 2026-11-02. It
  ran under Espressif's QEMU fork. nano-ros supports a board only where an
  executor can park until an arrival or its next deadline. On bare metal that
  needs a network receive interrupt and a one-shot timer behind the platform's
  wake object, and this board had neither, so it could only poll. It was
  retired rather than served by polling. Every name it answered to is refused
  wherever a board is read, with that reason; [Retired names](retired-names.md#retired-board-names)
  lists them.
- **The ESP-IDF component and the `idf.py` build driver**, retired 2026-09.
  Nothing built the component, and it was never published to the Espressif
  Component Registry. With it gone, `nros build` refuses a C or C++ project
  for an ESP32 instead of running `idf.py` with no project to build.

The platform ABI and the C API are unchanged, so an out-of-tree port is
possible. It has to bring what every supported board has: on bare metal, the
receive interrupt and one-shot timer that signal the wake object; on ESP-IDF,
a component, a way to carry nano-ros's resolved settings into the ESP-IDF
build, and a build that something runs. [Add a board](../extending/add-a-board.md)
and [Add a platform](../extending/add-a-platform.md) describe what a port
provides.

## If it doesn't work

- **The configure stops on a `NANO_ROS_*` variable.** The variable is retired,
  or an older release wrote it into the build directory's cache. The error
  prints the `cmake -U …` line that drops it; [Retired names](retired-names.md)
  lists every rename.
- **`find_package(NanoRos)` fails.** That package no longer exists. Follow
  [Move off the install-local prefix](#move-off-the-install-local-prefix).

## See also

- [Retired names](retired-names.md): every retired variable, board, knob and
  command spelling, and what to write instead.
- [Install options, pinning, working offline](../guide/build/install-options.md):
  pinning a project to a release.
