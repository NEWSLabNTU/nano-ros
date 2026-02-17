# Phase 44 — CMake Install Package

## Status: Not Started

## Background

The C API's CMake integration currently uses `NANO_ROS_ROOT` to locate 7
artifacts via hardcoded repo-relative paths spread across 4 CMake modules:

| Module                               | Location                      |
|--------------------------------------|-------------------------------|
| `FindNanoRos.cmake`                  | `cmake/`                      |
| `FindNanoRosCodegen.cmake`           | `cmake/`                      |
| `FindNanoRosC.cmake`                 | `packages/core/nros-c/cmake/` |
| `nano_ros_generate_interfaces.cmake` | `packages/core/nros-c/cmake/` |

These modules infer `NANO_ROS_ROOT` from their own location, then resolve
artifacts via paths like `${NANO_ROS_ROOT}/target/release/libnros_c.a` and
`${NANO_ROS_ROOT}/packages/codegen/packages/target/release/libnano_ros_codegen_c.a`.

**Problems:**

1. **Only works in the dev repo** — end-users can't install nros-c to a system
   prefix and use `find_package(NanoRos)` from arbitrary projects
2. **Fragile** — hardcoded paths break if the repo structure changes
3. **Stale codegen library** — `just build` doesn't rebuild the C codegen
   library when templates change, causing silent test failures
4. **Inconsistent naming** — the codegen crate is `nano-ros-codegen-c` while
   all other crates use the `nros-` prefix

### Goals

1. **Config-mode CMake package** — `find_package(NanoRos CONFIG)` works from
   both a pseudo-install dir (dev) and a real system prefix (installed)
2. **Pseudo-install directory** — `build/install/` populated by `just build`,
   mirroring a standard CMake install layout
3. **Fresh build chain** — `just build` ensures all artifacts are fresh
   end-to-end: Rust source → codegen lib → bindings → C examples
4. **Rename codegen crate** — `nano-ros-codegen-c` → `nros-codegen-c` for
   consistency with the rest of the project
5. **Deprecate Find modules** — old `Find*.cmake` modules get deprecation
   notices pointing to the new config-mode package

### Non-Goals

- Making the Zephyr C examples use the CMake package (they use west/CMake with
  direct source inclusion — a different build paradigm)
- Publishing the CMake package to any package registry
- Cross-compiling nros-c via CMake (Cargo handles this)

## Sub-phases

### 44.1 — Rename `nano-ros-codegen-c` to `nros-codegen-c`

Rename the crate, library, header, and all references for naming consistency.

- [ ] Rename directory `packages/codegen/packages/nano-ros-codegen-c/` →
  `packages/codegen/packages/nros-codegen-c/`
- [ ] Update `Cargo.toml` package name: `nano-ros-codegen-c` → `nros-codegen-c`
- [ ] Update workspace `Cargo.toml` member list
- [ ] Rename header: `nano_ros_codegen.h` → `nros_codegen.h`
- [ ] Rename C function: `nano_ros_codegen_generate_c()` → `nros_codegen_generate_c()`
- [ ] Update `lib.rs` `#[unsafe(no_mangle)]` function name
- [ ] Update `codegen_main.c` includes and function calls
- [ ] Update `justfile` (`build-codegen-lib` recipe)
- [ ] Update `tests/c-msg-gen-tests.sh`
- [ ] Update `cmake/FindNanoRosCodegen.cmake` (library name, paths)
- [ ] Update docs: `CLAUDE.md`, `docs/guides/message-generation.md`
- [ ] Verify: `just build-codegen-lib && just test-c`

### 44.2 — Create pseudo-install layout and `install-local` recipe

Populate `build/install/` with all C API artifacts in a standard layout.

- [ ] Define install directory structure:
  ```
  build/install/
  ├── lib/
  │   ├── libnros_c.a
  │   ├── libnros_codegen_c.a
  │   └── cmake/NanoRos/
  │       ├── NanoRosConfig.cmake
  │       ├── NanoRosCTargets.cmake
  │       └── NanoRosGenerateInterfaces.cmake
  ├── include/nros/
  │   ├── nros.h, types.h, cdr.h, ...
  ├── libexec/nano-ros/
  │   ├── nros_codegen.h
  │   └── codegen_main.c
  └── share/nano-ros/interfaces/
      ├── std_msgs/, builtin_interfaces/, ...
  ```
- [ ] Add `install-local` recipe to justfile (depends on `build-codegen-lib`)
- [ ] Add `build/install/` to `.gitignore`
- [ ] Update `just build` chain: `install-local` runs before `build-examples`
- [ ] Update `just clean` to remove `build/install/`

### 44.3 — Write config-mode CMake package

Create the 3 new CMake files that form the `NanoRos` config-mode package.

- [ ] `cmake/NanoRosConfig.cmake` — entry point for `find_package(NanoRos CONFIG)`
  - Computes `_NANO_ROS_PREFIX` from own location (`../../..`)
  - Includes `NanoRosCTargets.cmake` and `NanoRosGenerateInterfaces.cmake`
- [ ] `cmake/NanoRosCTargets.cmake` — defines `NanoRos::NanoRos` imported target
  - Finds `libnros_c.a` and headers relative to prefix
  - Sets platform link libraries (pthread, dl, m)
  - Creates `nros_c::nros_c` alias for backward compatibility
- [ ] `cmake/NanoRosGenerateInterfaces.cmake` — merged codegen + generate function
  - Builds codegen tool via `try_compile` from prefix-relative paths
  - Resolves interfaces: local → ament → `${prefix}/share/nano-ros/interfaces/`
  - Provides `nano_ros_generate_interfaces()` function (same API as current)
- [ ] Verify: config files are copied to `build/install/lib/cmake/NanoRos/` by
  `install-local`

### 44.4 — Migrate C examples to config-mode

Update all native C example CMakeLists.txt to use `find_package(NanoRos CONFIG)`.

- [ ] `examples/native/c/zenoh/talker/CMakeLists.txt`
- [ ] `examples/native/c/zenoh/listener/CMakeLists.txt`
- [ ] `examples/native/c/zenoh/custom-msg/CMakeLists.txt`
- [ ] `examples/native/c/zenoh/baremetal-demo/CMakeLists.txt`
- [ ] `examples/native/c/xrce/talker/CMakeLists.txt`
- [ ] `examples/native/c/xrce/listener/CMakeLists.txt`
- [ ] Remove `-DNANO_ROS_ROOT=...` from `build-examples-c` and
  `build-examples-c-xrce` justfile recipes
- [ ] Change `build-examples-c` dependency from `build-codegen-lib` to
  `install-local`
- [ ] Verify: `just build-examples-c && just test-c`

### 44.5 — Delete old Find modules + clean up

After all examples are migrated, delete the superseded modules.

- [ ] Delete `cmake/FindNanoRos.cmake`
- [ ] Delete `cmake/FindNanoRosCodegen.cmake`
- [ ] Delete `packages/core/nros-c/cmake/FindNanoRosC.cmake`
- [ ] Delete `packages/core/nros-c/cmake/nano_ros_generate_interfaces.cmake`
- [ ] Delete `packages/core/nros-c/cmake/nano_ros_cConfig.cmake.in`
- [ ] Grep verify: no remaining references to `FindNanoRos`, `FindNanoRosC`,
  `FindNanoRosCodegen`, `NANO_ROS_ROOT` in any example `CMakeLists.txt`
- [ ] Update CLAUDE.md C API / CMake documentation
- [ ] Update `docs/guides/message-generation.md`

## Design Decisions

### CMake source files stay in `cmake/` at repo root

The config-mode package spans multiple crates (nros-c lib, codegen lib,
bundled interfaces), so it doesn't belong inside any single package directory.
The root `cmake/` directory is already the public-facing location for CMake
modules. Source templates live in `cmake/` and get copied to
`build/install/lib/cmake/NanoRos/` during `install-local`.

### Example auto-detection

C examples inside the repo auto-detect the pseudo-install dir by walking up
from their location:

```cmake
if(NOT DEFINED NanoRos_DIR)
    get_filename_component(_repo_root "${CMAKE_CURRENT_LIST_DIR}/../../../../.." ABSOLUTE)
    set(_local "${_repo_root}/build/install/lib/cmake/NanoRos")
    if(EXISTS "${_local}/NanoRosConfig.cmake")
        set(NanoRos_DIR "${_local}")
    endif()
endif()
find_package(NanoRos REQUIRED CONFIG)
```

End-users installing to a system prefix just use:
```cmake
find_package(NanoRos REQUIRED CONFIG)
```

### Freshness chain

| Source change       | Rebuilds                 | Mechanism                        |
|---------------------|--------------------------|----------------------------------|
| Rust nros-c source  | `libnros_c.a`            | Cargo auto-recompiles            |
| Rust codegen source | `libnros_codegen_c.a`    | Cargo auto-recompiles            |
| Jinja templates     | `libnros_codegen_c.a`    | Cargo detects via `include_str!` |
| nros-c headers      | `build/install/include/` | `cp` in `install-local`          |
| Bundled .msg files  | `build/install/share/`   | `cp` in `install-local`          |

The `rm -rf build` in each C example's build recipe ensures CMake cache is
clean, so `try_compile` always uses fresh codegen tool.

## Verification

1. `just build` succeeds end-to-end (includes `install-local`)
2. `just test-c` and `just test-c-xrce` pass
3. Touch a Jinja template → `just build` → C example gets fresh codegen output
4. System install test: copy `build/install` to `/tmp/` and build a C example
   against it with `cmake -DNanoRos_DIR=...`
5. `just quality` passes
