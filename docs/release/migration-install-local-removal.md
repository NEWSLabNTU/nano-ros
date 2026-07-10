# Migrating off `just install-local` (Phase 140)

**TL;DR.** Phase 140 deleted `just install-local`, the `build/install/`
prefix, every `install(...)` rule, every `Config.cmake.in` template,
and the `find_package(NanoRos)` consumption path. nano-ros is now
consumed exclusively via `add_subdirectory(<repo-root>)` from the
user's `CMakeLists.txt`.

If your project was on a pre-140 checkout, this page is the one-page
rewrite that gets you onto the supported shape.

## Before (pre-140)

```cmake
cmake_minimum_required(VERSION 3.16)
project(my_app C)

find_package(NanoRos REQUIRED CONFIG)
nano_ros_generate_interfaces(std_msgs SKIP_INSTALL)
add_executable(my_app src/main.c)
target_link_libraries(my_app PRIVATE
    std_msgs__nano_ros_c
    NanoRos::NanoRos)
```

Build:

```bash
# In the nano-ros checkout:
just install-local                                # populates build/install/

# In the user project:
cmake -S . -B build -DCMAKE_PREFIX_PATH=<nano-ros>/build/install
cmake --build build
```

## After (post-140)

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_app C)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory(<path-to-nano-ros> nano_ros)

nano_ros_generate_interfaces(std_msgs SKIP_INSTALL)
add_executable(my_app src/main.c)
target_link_libraries(my_app PRIVATE
    std_msgs__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(my_app)
```

Build:

```bash
# No nano-ros-side install step. Just configure your project.
cmake -S . -B build
cmake --build build
```

## What changed

| Pre-140 | Post-140 |
|---------|----------|
| `find_package(NanoRos REQUIRED CONFIG)` | `add_subdirectory(<path-to-nano-ros>)` after `set(NANO_ROS_PLATFORM ...)` + `set(NANO_ROS_RMW ...)` |
| `just install-local` | (deleted) — nano-ros builds in-tree per-example via Corrosion |
| `cmake -DCMAKE_PREFIX_PATH=<nano-ros>/build/install` | (not needed) |
| `nano_ros_link_platform(target)` / `nano_ros_link_rmw(target)` | (folded into `NanoRos::NanoRos`'s INTERFACE link libraries) |
| `nros_freertos_compose_platform(target)` / `nros_threadx_compose_platform(target)` | `nros_platform_link_app(target)` — single per-app fixup hook |
| `find_package(NrosRmwCyclonedds CONFIG REQUIRED)` | (auto-wired when `NANO_ROS_RMW=cyclonedds`) |
| `cmake_minimum_required(VERSION 3.16)` | `cmake_minimum_required(VERSION 3.22)` (matches root nano-ros) |

## RTOS-specific notes

- **Zephyr.** Consume nano-ros via the `zephyr/`
  module — drop `nano-ros` into your `west.yml`, set
  `CONFIG_NROS=y` + `CONFIG_NROS_RMW="zenoh"` in `prj.conf`.
- **ESP-IDF.** `integrations/nano-ros/` is a component manifest —
  add it to your `idf_component.yml`.
- **PlatformIO.** The manifest is `library.json` at the **repo root**
  (`integrations/platformio/` holds only the `nros_codegen.py` extra-script).
  The library is **not published** to the PlatformIO registry, so
  `lib_deps = nano-ros@*` will not resolve — use a path or git pointer in
  `platformio.ini`.
- **NuttX.** `integrations/nuttx/` is a `apps/external/` shim —
  symlink (or copy) and enable via `make menuconfig`.
- **PX4.** `integrations/px4/module-template/` is a
  `EXTERNAL_MODULES_LOCATION` template — copy out, edit the user
  glue, point PX4 at it.

Each shell internally does
`add_subdirectory(<nano-ros>)` with the right
`NANO_ROS_PLATFORM` cache var set; user-side code is the same as
the C/C++ snippet above.

## What you lose

- A pre-built `<prefix>/lib/libnros_c.a` you could link from arbitrary
  external projects. Post-140, each consumer builds the staticlib in
  its own build tree (Corrosion handles caching via the Cargo
  target dir).
- The `build/install/share/nano-ros/interfaces/` bundled-interface
  drop. Codegen now resolves interface files via the ament index
  (when colcon-discovered) or directly from `packages/codegen/interfaces/`
  inside the nano-ros checkout.
- The `cmake --install build` step. There is nothing nano-ros-side
  to install. Your own project ships its binary; nano-ros is a
  static dependency.

## Why this change

`find_package(NanoRos)` was a Debian-style "library installed
once, consumed by many projects" model. The RTOS workflows that
nano-ros actually targets (Zephyr `west`, ESP-IDF, PlatformIO,
NuttX `apps/external`, PX4 `EXTERNAL_MODULES_LOCATION`) all
consume dependencies as source trees inside the user's workspace —
not from an installed prefix. The install path drifted out of sync
with the cargo path more than once (Phase 134 UDP multicast
linker error was the proof) and added ~30 s warm / ~10 min cold to
every test-all run. Removing it collapses two surfaces into one.

See `docs/roadmap/phase-140-install-local-rip-off.md` for the full
design notes and the audit table that drove the deletion.
