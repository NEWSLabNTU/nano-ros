# Phase 75 — Relocatable CMake Install Convention for C/C++ Binaries

**Goal**: Make every C/C++ platform build independent of the nano-ros source tree
location. All platforms use `CMAKE_PREFIX_PATH` pointing to a standard install
prefix — no relative paths, no symlinks at project root, no Cargo invoked by
user projects.

**Status**: In Progress (75.1, 75.2, 75.8 done; 75.3 partial)

**Priority**: Medium

**Depends on**: Phase 69 (cross-platform C/C++ examples)

## Overview

### Current State

Three of the four C/C++ platforms resolve nano-ros resources through ad-hoc
mechanisms that depend on knowing where the source tree is:

| Platform       | How resources are found                                                                         | Problem                                        |
|----------------|-------------------------------------------------------------------------------------------------|------------------------------------------------|
| Native (POSIX) | `find_package(NanoRos CONFIG)` via `CMAKE_PREFIX_PATH`                                          | Correct — no changes needed                    |
| FreeRTOS       | `get_filename_component(_NROS_ROOT ../../..)` then searches `packages/codegen/packages/target/` | Source-tree relative                           |
| NuttX          | Same as FreeRTOS; also creates symlinks at project root (`share/nano-ros/`)                     | Source-tree relative + creates stale artifacts |
| Zephyr         | `find_program(nros-codegen)` on `PATH`                                                          | Environment-dependent                          |

The FreeRTOS and NuttX platform helpers (`freertos-platform.cmake`,
`nuttx-platform.cmake`) work around the missing install by:

1. Walking `../../..` up the cmake file's own path to find the source root
2. Searching Cargo's `target/release/` directory for the `nros-codegen` binary
3. Running `cargo build` at configure time if the binary is missing
4. Creating symlinks at project root (`share/nano-ros/rust/nros-serdes`,
   `share/nano-ros/interfaces`) to fake the install layout expected by
   `NanoRosGenerateInterfaces.cmake`

This means users cloning nano-ros into a different path, vendoring it as a
submodule, or using a pre-built release cannot use these examples without also
having the full Rust toolchain and Cargo on PATH.

### Target State

All platforms follow the same pattern:

```cmake
# User passes one variable — the nano-ros install prefix.
# cmake -DCMAKE_PREFIX_PATH=/path/to/nros-install ...

find_package(NanoRos CONFIG REQUIRED)   # finds codegen tool + cmake modules
include(freertos-support.cmake)         # adds FreeRTOS kernel + lwIP sources only

nros_generate_interfaces(...)
add_executable(talker src/main.c)
target_link_libraries(talker PRIVATE NanoRos::NanoRos freertos_platform)
```

The install prefix is self-contained and location-independent:

```
<prefix>/
├── bin/
│   └── nros-codegen                    ← host codegen tool
├── include/
│   └── nros/                           ← C + C++ headers
├── lib/
│   ├── libnros_c_zenoh.a               ← POSIX x86_64
│   ├── libnros_c_zenoh_freertos_armcm3.a   ← FreeRTOS ARM Cortex-M3
│   ├── libnros_c_zenoh_nuttx_armv7a.a      ← NuttX ARMv7a
│   ├── libnros_cpp_zenoh.a             ← POSIX x86_64
│   ├── libnros_cpp_zenoh_freertos_armcm3.a
│   ├── libnros_cpp_zenoh_nuttx_armv7a.a
│   └── cmake/NanoRos/
│       ├── NanoRosConfig.cmake
│       ├── NanoRosCTargets.cmake       ← selects library by platform
│       ├── NanoRosCppTargets.cmake
│       └── NanoRosGenerateInterfaces.cmake
└── share/nano-ros/rust/nros-serdes/    ← for C++ FFI crate (already correct)
```

### Key Design Decisions

**Library naming includes platform suffix**

Using `libnros_c_<rmw>_<platform>_<arch>.a` (e.g.
`libnros_c_zenoh_freertos_armcm3.a`) follows the same convention already used
for RMW suffixes. Users select the platform via a `NANO_ROS_PLATFORM` CMake
variable, consistent with how `NANO_ROS_RMW` selects the RMW backend.

**Single install prefix, not per-target sysroots**

Keeping all libraries under one prefix simplifies `find_package` — one
`CMAKE_PREFIX_PATH` points to everything. The platform and RMW selection is
done by the targets cmake file via variables, not by separate prefixes.

**Platform support modules provide RTOS sources only**

`freertos-support.cmake` and `nuttx-support.cmake` (renamed from
`*-platform.cmake`) provide only the RTOS kernel, networking stack, and linker
script targets. They contain no nano-ros path logic. Users who vendor nano-ros
as a submodule provide their own support modules if needed.

**Toolchain files are proper CMake toolchain files**

The ARM cross-compiler setup moves from the support module into dedicated CMake
toolchain files (`toolchain/arm-freertos-armcm3.cmake`,
`toolchain/armv7a-nuttx-eabi.cmake`) that users pass via
`-DCMAKE_TOOLCHAIN_FILE`. This is the standard CMake idiom for cross-compilation
and separates compiler selection from library discovery.

**`just install-local` builds all platform variants**

The install step extends to build FreeRTOS and NuttX cross-compiled libraries
(when the respective toolchains are available) and install them with
platform-suffixed names. Missing toolchains produce a warning, not an error —
only the native variant is required.

## Work Items

- [x] 75.1 — Add `NANO_ROS_PLATFORM` to `NanoRosCTargets.cmake`
- [x] 75.2 — Add cross-compiled library builds to `just install-local`
- [ ] 75.3 — Extract toolchain files from platform cmake helpers (FreeRTOS done; NuttX pending)
- [ ] 75.4 — Rewrite `freertos-platform.cmake` → `freertos-support.cmake`
- [ ] 75.5 — Rewrite `nuttx-platform.cmake` → `nuttx-support.cmake`
- [ ] 75.6 — Update FreeRTOS and NuttX examples to use `find_package(NanoRos)`
- [ ] 75.7 — Fix Zephyr codegen tool discovery
- [x] 75.8 — Add `just clean-install` recipe
- [ ] 75.9 — Add CPack configuration for binary distribution archives
- [ ] 75.10 — Update docs and integration tests

---

### 75.1 — Add `NANO_ROS_PLATFORM` to `NanoRosCTargets.cmake`

Extend the targets cmake file to select the library based on a `NANO_ROS_PLATFORM`
variable (default: `posix`) alongside the existing `NANO_ROS_RMW` variable.

```cmake
# NanoRosCTargets.cmake

if(NOT DEFINED NANO_ROS_PLATFORM)
  if(CMAKE_SYSTEM_NAME STREQUAL "FreeRTOS")
    set(NANO_ROS_PLATFORM "freertos_armcm3")
  elseif(CMAKE_SYSTEM_NAME MATCHES "NuttX")
    set(NANO_ROS_PLATFORM "nuttx_armv7a")
  else()
    set(NANO_ROS_PLATFORM "posix")
  endif()
endif()

if(NANO_ROS_PLATFORM STREQUAL "posix")
  set(_nros_lib "${_NANO_ROS_PREFIX}/lib/libnros_c_${NANO_ROS_RMW}.a")
else()
  set(_nros_lib "${_NANO_ROS_PREFIX}/lib/libnros_c_${NANO_ROS_RMW}_${NANO_ROS_PLATFORM}.a")
endif()
```

The fallback error message lists all detected variants (same pattern as the
existing RMW fallback in `NanoRosCppTargets.cmake`).

**Files**:
- `packages/core/nros-c/cmake/NanoRosCTargets.cmake`
- `packages/core/nros-cpp/cmake/NanoRosCppTargets.cmake`

---

### 75.2 — Add cross-compiled library builds to `just install-local`

Extend `just install-local` to optionally build FreeRTOS and NuttX variants.
The recipe skips a platform if the required toolchain (`arm-none-eabi-gcc`,
`arm-none-eabi-g++`) is absent.

```just
install-local:
    #!/usr/bin/env bash
    set -e
    PREFIX="$(pwd)/build/install"

    # Host libraries (POSIX) — always built
    for rmw in zenoh xrce; do
        cmake -S . -B "build/cmake-$rmw" \
            -DNANO_ROS_RMW="$rmw" -DCMAKE_BUILD_TYPE=Release
        cmake --build "build/cmake-$rmw"
        cmake --install "build/cmake-$rmw" --prefix "$PREFIX"
    done

    # FreeRTOS ARM Cortex-M3 libraries — built if toolchain available
    if command -v arm-none-eabi-gcc &>/dev/null; then
        for rmw in zenoh; do
            cmake -S . -B "build/cmake-freertos-armcm3-$rmw" \
                -DCMAKE_TOOLCHAIN_FILE="cmake/toolchain/arm-freertos-armcm3.cmake" \
                -DNANO_ROS_RMW="$rmw" \
                -DNANO_ROS_PLATFORM="freertos_armcm3" \
                -DCMAKE_BUILD_TYPE=Release
            cmake --build "build/cmake-freertos-armcm3-$rmw" \
                --target nros_c-static nros_cpp-static
            cmake --install "build/cmake-freertos-armcm3-$rmw" \
                --prefix "$PREFIX" \
                --component libraries
        done
    else
        echo "arm-none-eabi-gcc not found — skipping FreeRTOS ARM libraries"
    fi

    echo "Installed to $PREFIX"
```

The `--component libraries` flag installs only the `.a` files and headers,
not the host tools (which are already installed from the POSIX build).

**Files**:
- `justfile` — extend `install-local` recipe
- `CMakeLists.txt` — add `NANO_ROS_PLATFORM` variable, use it in library rename
- `packages/core/nros-c/CMakeLists.txt` — rename output:
  `libnros_c_${NANO_ROS_RMW}_${NANO_ROS_PLATFORM}.a` when platform is not `posix`
- `packages/core/nros-cpp/CMakeLists.txt` — same

---

### 75.3 — Extract toolchain files

Create proper CMake toolchain files covering compiler selection, target triple,
and system name. Move this content out of the platform support modules.

**`cmake/toolchain/arm-freertos-armcm3.cmake`**:
```cmake
set(CMAKE_SYSTEM_NAME       FreeRTOS)
set(CMAKE_SYSTEM_PROCESSOR  armcm3)

set(CMAKE_C_COMPILER    arm-none-eabi-gcc)
set(CMAKE_CXX_COMPILER  arm-none-eabi-g++)
set(CMAKE_ASM_COMPILER  arm-none-eabi-gcc)

set(CMAKE_C_FLAGS_INIT   "-mcpu=cortex-m3 -mthumb -mfloat-abi=soft")
set(CMAKE_CXX_FLAGS_INIT "-mcpu=cortex-m3 -mthumb -mfloat-abi=soft")
set(CMAKE_EXE_LINKER_FLAGS_INIT "-specs=nosys.specs")

# Prevent CMake from probing host tools as target tools
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
```

**`cmake/toolchain/armv7a-nuttx-eabi.cmake`**:
```cmake
set(CMAKE_SYSTEM_NAME       NuttX)
set(CMAKE_SYSTEM_PROCESSOR  armv7a)

set(CMAKE_C_COMPILER    arm-none-eabi-gcc)
set(CMAKE_CXX_COMPILER  arm-none-eabi-g++)

set(CMAKE_C_FLAGS_INIT   "-mcpu=cortex-a7 -mthumb -mfloat-abi=hard -mfpu=neon-vfpv4")
set(CMAKE_CXX_FLAGS_INIT "-mcpu=cortex-a7 -mthumb -mfloat-abi=hard -mfpu=neon-vfpv4")

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
```

**Files**:
- `cmake/toolchain/arm-freertos-armcm3.cmake` ✓ done
- `cmake/toolchain/armv7a-nuttx-eabi.cmake` (pending)

---

### 75.4 — Rewrite `freertos-platform.cmake` → `freertos-support.cmake`

Strip all nano-ros path logic from the FreeRTOS platform module. It provides
only RTOS sources. The nano-ros library and codegen are discovered via
`find_package`.

Before (current approach — source-relative, builds Cargo if needed):
```cmake
get_filename_component(_NROS_ROOT "${CMAKE_CURRENT_LIST_FILE}/../../../.." ABSOLUTE)
set(_CODEGEN_TARGET_DIR "${_NROS_ROOT}/packages/codegen/packages/target")
find_program(_NANO_ROS_CODEGEN_TOOL nros-codegen
    PATHS "${_CODEGEN_TARGET_DIR}/release" ...)
if(NOT _NANO_ROS_CODEGEN_TOOL)
    execute_process(COMMAND cargo build ...)  # builds Cargo at configure time
endif()
set(_NANO_ROS_PREFIX "${_NROS_ROOT}")
set(_NANO_ROS_CMAKE_DIR "${_NROS_ROOT}/packages/codegen/.../cmake")
file(CREATE_LINK ...)  # creates share/ symlinks at project root
```

After (install-prefix approach):
```cmake
# freertos-support.cmake
# Provides: freertos_platform target (FreeRTOS kernel + lwIP + LAN9118)
# Requires: FREERTOS_DIR, LWIP_DIR (env vars or cmake -D)
# Does NOT touch nano-ros paths — caller does find_package(NanoRos) separately.

# ... FreeRTOS kernel sources, lwIP sources, LAN9118 driver, linker script ...
# Unchanged from current implementation except removing all _NROS_ROOT logic.
```

Example `CMakeLists.txt` after:
```cmake
cmake_minimum_required(VERSION 3.22)
set(CMAKE_TOOLCHAIN_FILE
    "${CMAKE_CURRENT_SOURCE_DIR}/../../cmake/toolchain/arm-freertos-armcm3.cmake")
project(freertos_cpp_talker LANGUAGES C CXX ASM)

# nano-ros: codegen tool + cmake module + target library
# User passes: cmake -DCMAKE_PREFIX_PATH=/path/to/nros-install ...
find_package(NanoRos CONFIG REQUIRED)

# RTOS platform: kernel + networking sources
include("${CMAKE_CURRENT_SOURCE_DIR}/../../cmake/freertos-support.cmake")

nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE CPP SKIP_INSTALL)

add_executable(freertos_cpp_talker src/main.cpp)
target_link_libraries(freertos_cpp_talker PRIVATE
    NanoRos::NanoRosCpp std_msgs freertos_platform)
```

**Files**:
- `examples/qemu-arm-freertos/cmake/freertos-platform.cmake` →
  `examples/qemu-arm-freertos/cmake/freertos-support.cmake`
- All FreeRTOS example `CMakeLists.txt` files updated

---

### 75.5 — Rewrite `nuttx-platform.cmake` → `nuttx-support.cmake`

Same transformation as 75.4 for NuttX. Additionally, remove the symlink
creation lines that produce `share/nano-ros/` at the project root:

```cmake
# REMOVE these lines:
if(NOT EXISTS "${_NROS_ROOT}/share/nano-ros/rust/nros-serdes/src")
    file(CREATE_LINK "${_serdes_src}" "${_NROS_ROOT}/share/nano-ros/rust/nros-serdes" SYMBOLIC)
endif()
if(NOT EXISTS "${_NROS_ROOT}/share/nano-ros/interfaces")
    file(CREATE_LINK "${_NROS_ROOT}/packages/codegen/interfaces" ...)
endif()
```

These symlinks faked the install layout inside the source tree. With the proper
install prefix approach they are unnecessary — `build/install/share/nano-ros/`
already contains the real files.

**Files**:
- `examples/qemu-arm-nuttx/cmake/nuttx-platform.cmake` →
  `examples/qemu-arm-nuttx/cmake/nuttx-support.cmake`
- All NuttX example `CMakeLists.txt` files updated
- `examples/qemu-arm-nuttx/cmake/NanoRosConfig.cmake` — remove (no longer needed;
  replaced by `find_package(NanoRos)` against the install prefix)

---

### 75.6 — Update FreeRTOS and NuttX examples

Update every `CMakeLists.txt` under `examples/qemu-arm-freertos/` and
`examples/qemu-arm-nuttx/` to:

1. Set `CMAKE_TOOLCHAIN_FILE` at the top
2. Call `find_package(NanoRos CONFIG REQUIRED)` instead of including the old
   platform cmake directly
3. Include the new `*-support.cmake` for RTOS sources
4. Remove any `NanoRos_DIR` / `_NANO_ROS_PREFIX` overrides

**Files**:
- All `examples/qemu-arm-freertos/{c,cpp}/zenoh/*/CMakeLists.txt`
- All `examples/qemu-arm-nuttx/{c,cpp}/zenoh/*/CMakeLists.txt`

---

### 75.7 — Fix Zephyr codegen tool discovery

Replace the `find_program(nros-codegen)` PATH search in the Zephyr cmake
module with a prefix-relative lookup.

```cmake
# zephyr/cmake/nros_generate_interfaces.cmake — current
find_program(_NROS_ZEPHYR_CODEGEN_TOOL nros-codegen)

# After: prefer prefix-relative location; fall back to PATH
if(DEFINED NanoRos_DIR)
    get_filename_component(_nros_prefix "${NanoRos_DIR}/../../.." ABSOLUTE)
    find_program(_NROS_ZEPHYR_CODEGEN_TOOL nros-codegen
        PATHS "${_nros_prefix}/bin"
        NO_DEFAULT_PATH)
endif()
if(NOT _NROS_ZEPHYR_CODEGEN_TOOL)
    find_program(_NROS_ZEPHYR_CODEGEN_TOOL nros-codegen)
endif()
```

Users set `NanoRos_DIR` in the west workspace's `CMakeLists.txt` or via the
Kconfig `CONFIG_NROS_INSTALL_PREFIX`.

**Files**:
- `zephyr/cmake/nros_generate_interfaces.cmake`
- `zephyr/Kconfig` — add `NROS_INSTALL_PREFIX` string config

---

### 75.8 — Add `just clean-install` recipe

`cmake --install` is additive and never removes stale files (see known issue 10).
Add a recipe that removes the install prefix before reinstalling.

```just
# Remove the install prefix and rebuild from scratch.
# Use this after library renames or CMake structural changes.
clean-install:
    rm -rf build/install/
    just install-local
```

**Files**:
- `justfile`

---

### 75.9 — Add CPack configuration for binary distribution

Add CPack support so nano-ros can be distributed as a pre-built archive that
users extract and point `CMAKE_PREFIX_PATH` at — no Rust toolchain required.

```cmake
# CMakeLists.txt (root)
include(CPack)

set(CPACK_PACKAGE_NAME      "nros")
set(CPACK_PACKAGE_VERSION   "${PROJECT_VERSION}")
set(CPACK_GENERATOR         "TGZ;ZIP")
set(CPACK_INSTALL_CMAKE_PROJECTS
    "${CMAKE_BINARY_DIR};NanoRos;ALL;/")
```

Running `cpack --config build/cmake-zenoh/CPackConfig.cmake` produces
`nros-<version>-linux-x86_64.tar.gz`. The archive unpacks to a standard prefix
tree, ready for `cmake -DCMAKE_PREFIX_PATH=...`.

This is the distribution mechanism for:
- GitHub Releases (CI builds the archives for each supported host)
- Users who do not have Rust/Cargo installed
- Reproducible builds in CI pipelines

**Files**:
- `CMakeLists.txt` — add `include(CPack)` + package metadata
- `.github/workflows/release.yml` (new or updated) — build + upload archives on tag

---

### 75.10 — Update docs and integration tests

**Docs**:
- `docs/reference/c-api-cmake.md` — replace "set `_NROS_ROOT`" instructions with
  `CMAKE_PREFIX_PATH` instructions for all platforms
- `docs/guides/cpp-api.md` — same
- Add a platform-specific section: "FreeRTOS / NuttX cross-compilation setup"
  showing the toolchain file + `CMAKE_PREFIX_PATH` pattern
- `CLAUDE.md` table — update phase 69 note referencing the support module rename

**Integration tests**:
- `packages/testing/nros-tests/tests/freertos_qemu.rs` — pass
  `-DCMAKE_PREFIX_PATH=build/install` instead of the current `NanoRos_DIR`
  workaround, if any
- `packages/testing/nros-tests/tests/nuttx_qemu.rs` — same

**Files**:
- `docs/reference/c-api-cmake.md`
- `docs/guides/cpp-api.md`
- `packages/testing/nros-tests/tests/freertos_qemu.rs`
- `packages/testing/nros-tests/tests/nuttx_qemu.rs`

---

## Acceptance Criteria

- [ ] `cmake -DCMAKE_PREFIX_PATH=<prefix>` is the only nano-ros location variable
      needed for native, FreeRTOS, and NuttX examples
- [ ] No platform cmake helper contains `get_filename_component(... ../../..)` to
      find the nano-ros source root
- [ ] No platform cmake helper invokes `cargo build` or searches Cargo `target/`
      directories
- [ ] No symlinks are created at the project root during CMake configure
- [x] `just install-local` installs FreeRTOS library variants when the
      ARM cross-compiler is available (NuttX pending 75.5)
- [x] `just clean-install` exists and produces a clean install prefix
- [ ] Zephyr examples work with `NanoRos_DIR` pointing to the install prefix
      (no PATH dependency on `nros-codegen`)
- [ ] `cpack` produces a redistributable `.tar.gz` containing the complete prefix
- [ ] All existing C/C++ integration tests pass after the refactor
- [ ] A user can use nano-ros as a CMake subproject or installed package
      interchangeably by setting `CMAKE_PREFIX_PATH`

## Notes

- **Backwards compatibility for FreeRTOS/NuttX examples**: The old
  `*-platform.cmake` filenames should be kept as deprecated thin wrappers that
  emit a `cmake_deprecation_warning` and include the new `*-support.cmake` +
  call `find_package(NanoRos)`. This avoids breaking any downstream forks.

- **Integration test `CMAKE_PREFIX_PATH`**: The test fixtures in `binaries.rs`
  already pass `-DNanoRos_DIR=build/install/lib/cmake/NanoRos`. Setting
  `CMAKE_PREFIX_PATH=build/install` is equivalent and more idiomatic; either
  works. Prefer `CMAKE_PREFIX_PATH` for the new examples to stay consistent.

- **Corrosion dependency**: FreeRTOS and NuttX cmake builds use Corrosion to
  compile the Rust FFI crates. Corrosion finds `cargo` on `PATH`. This is a
  build-time (developer machine) dependency — users who only consume pre-built
  `.a` files from CPack archives do not need Rust. The C++ codegen (which builds
  a per-package FFI crate at user-project configure time) still requires Cargo.

- **NuttX `cmake/NanoRosConfig.cmake`**: The file at
  `examples/qemu-arm-nuttx/cmake/NanoRosConfig.cmake` is a local shim that
  existed to make `find_package` work without a real install. It should be
  removed entirely once the install contains the NuttX libraries.
