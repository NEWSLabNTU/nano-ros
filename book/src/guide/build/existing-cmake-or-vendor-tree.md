# Build inside an existing CMake or vendor tree

<!-- type: how-to · level: 3 -->

**Needs first:** [Install options, pinning, working offline](install-options.md)

Use this page when nano-ros has to live inside a build you already own: a C or
C++ CMake tree with its own layout, or a company tree (a BSP, a forked RTOS, a
CI system) that drives cargo or CMake itself.

If your project is a nano-ros workspace, you do not need this page. You state
the board once in the bringup's `system.toml`, and `nros build` sets
everything below for you. Start at
[Organize a larger workspace](organize-a-larger-workspace.md) instead.

nano-ros is distributed as a release: the `nros` CLI and the library source it
builds against, the *SDK root*. Nothing is on crates.io and there is no
prebuilt library or install prefix, so "vendoring" nano-ros means pinning and
mirroring a release, not copying its repository into your tree.

## Before you start

- A release installed and pinned, as in
  [Install options, pinning, working offline](install-options.md#pinning-a-version).
- The board provisioned with `nros setup <board>`. For every board with a
  CMake build, that also installs Corrosion, which a C or C++ configure needs
  and never downloads.

## Steps

### Add nano-ros to a CMake tree

1. Ask for the SDK root. It is the installed release's `share/nano-ros`, for
   the version the project is pinned to:

   ```bash
   nros locate nano-ros
   ```

2. Pick the platform and the backend before you add nano-ros, then link it.
   The variables must be set before `add_subdirectory`, because nano-ros reads
   them when it is included:

   ```cmake
   cmake_minimum_required(VERSION 3.22)
   project(my_app C)

   set(NROS_PLATFORM posix)   # posix | freertos | nuttx | threadx
   set(NROS_RMW     zenoh)    # zenoh | xrce | cyclonedds

   add_subdirectory("${NANO_ROS_SDK}" nano_ros)

   add_executable(my_app main.c)
   target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
   if(COMMAND nros_platform_link_app)   # defined by the zephyr and threadx platforms
       nros_platform_link_app(my_app)
   endif()
   ```

   The variables and their values are listed in
   [CMake cache variables](../../reference/environment-variables.md#cmake-cache-variables).
   Zephyr and bare metal do not enter this way: Zephyr has its own module,
   and bare metal builds through cargo.

3. Generate the message bindings your sources include:

   ```cmake
   nano_ros_generate_interfaces(std_msgs LANGUAGE C
       DEPENDENCIES builtin_interfaces)
   target_link_libraries(my_app PRIVATE std_msgs__nano_ros_c)
   ```

4. Configure with the SDK root and build:

   ```bash
   cmake -B build -DNANO_ROS_SDK="$(nros locate nano-ros)"
   cmake --build build
   ```

   CMake's target propagation brings in the nano-ros libraries, the
   platform's own system libraries and the per-build configuration header.

The higher-level form is `find_package(nano_ros REQUIRED)`, located with
`-Dnano_ros_ROOT=<sdk-root>`. The CMake presets `nros setup presets` writes
pass it for you, so `cmake --preset <board>` works.

### Drive cargo or CMake yourself from CI

In a nano-ros workspace, a CI job that runs cargo or CMake itself prepares the
generated inputs first, without building, and then runs the line
`nros build` prints:

```bash
nros configure
nros build linux --dry-run
```

```text
nros build: host_node_bringup:linux -> board linux (platform posix), driver cargo
cargo update --workspace --manifest-path build/posix-zenoh/linux_entry/Cargo.toml --config build/posix-zenoh/linux_entry/nros-cargo.toml
cargo build --manifest-path build/posix-zenoh/linux_entry/Cargo.toml --config build/posix-zenoh/linux_entry/nros-cargo.toml
```

A run of `nros configure` with nothing changed does nothing, and its stamp
under `build/nros/configured/<image>/` holds a digest of every input the image
read, which makes a CI cache key.

Everything the build reads is generated under `build/`, never beside your
packages: the message crates, the target triple, the runner, the link flags,
the resolved `[env]` and the `[patch.crates-io]` rows that point registry-style
names such as `nros = { version = "*" }` at the SDK root. So a tree moves
freely. After you relocate it, or install another release, the next build
rewrites what is under `build/` and touches nothing you track.

### Keep your own boards in your tree

Point `NROS_EXTRA_BOARD_PATH` (an environment variable, or a CMake cache
variable for `nano_ros_use_board()`) at a directory of board crates in your
tree, laid out like the SDK's `packages/boards/`. Board keys stay global: one
name under two roots is an error. A new platform, an RTOS port of your own, is
the one extension that edits nano-ros itself; see
[Add a platform](../../extending/custom-platform.md).

## Example

A complete C project with no `system.toml`, `my_app/CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_app C)

set(NROS_PLATFORM posix)
set(NROS_RMW     zenoh)
add_subdirectory("${NANO_ROS_SDK}" nano_ros)

add_executable(my_app main.c)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)

nano_ros_generate_interfaces(std_msgs LANGUAGE C
    DEPENDENCIES builtin_interfaces)
target_link_libraries(my_app PRIVATE std_msgs__nano_ros_c)
```

```bash
cmake -B build -DNANO_ROS_SDK="$(nros locate nano-ros)"
cmake --build build
```

## If it doesn't work

- **The configure stops and names `nros setup --tool corrosion`.** Corrosion
  is not in the SDK store. Run that command, or the `nros setup <board>` you
  already use.
- **A setting in your CMakeLists has no effect.** It was set after
  `add_subdirectory`. Move every `NROS_*` variable above it.
- **A message crate resolves from crates.io.** A Rust package named it by
  registry name (`std_msgs = "*"`). Let `nros build` write the
  `# nros-managed` path row from `package.xml` instead.
- **You set `NROS_PLATFORM` in a nano-ros workspace.** The generated root
  already derives it from the image's `board`. Remove it: two answers to one
  question.

## See also

- [Install options, pinning, working offline](install-options.md): pin the
  release, mirror it, and build with no network.
- [Build outputs](../../reference/build-outputs.md): where each target
  family's artifact lands.
- [How a build becomes an image](../../background/how-a-build-becomes-an-image.md).
- [Add a board](../../extending/add-a-board.md) and its
  [out-of-tree example](../../extending/add-a-board.md#example): board code
  that lives in your tree.

## Design background

- [RFC-0098: generated leaf build config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md)
- [RFC-0103: one owner per fact](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0103-one-owner-per-fact.md)
