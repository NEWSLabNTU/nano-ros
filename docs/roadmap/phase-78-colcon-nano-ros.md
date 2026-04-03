# Phase 78: Colcon Build Type for nano-ros

**Goal**: Enable ROS 2 users to build nano-ros packages (native, RTOS, bare-metal) with `colcon build` using a custom `nros.<lang>.<platform>` build type.

**Status**: In Progress (78.1–78.10 done)
**Priority**: Medium
**Depends on**: Phase 69 (C/C++ examples), Phase 75 (CMake install)
**Design doc**: `docs/design/colcon-nano-ros-build-type.md`
**Repo**: `packages/codegen/` submodule → `https://github.com/jerry73204/colcon-nano-ros.git`

## Overview

A colcon plugin (`colcon-nano-ros`) that registers build/test tasks for nano-ros packages. The build type name `nros.<lang>.<platform>` in `package.xml` tells colcon which language and target platform to build for. Board-specific configuration is handled by the board crate (Rust) or CMake platform module (C/C++), not by the colcon plugin.

### User Experience

```bash
# Install
pip install colcon-nano-ros

# Build a FreeRTOS Rust workspace
cd ~/nros_ws
colcon build

# Build only native packages
colcon build --packages-select-build-type ros.nros.rust.native
```

### Package Example

```xml
<!-- package.xml -->
<package format="3">
  <name>motor_controller</name>
  <version>0.1.0</version>
  <export>
    <build_type>nros.rust.freertos</build_type>
  </export>
  <depend>std_msgs</depend>
  <depend>example_interfaces</depend>
</package>
```

## Work Items

### 78.1 — Project scaffolding

Set up the `colcon-nano-ros` repo with maturin (Rust + Python) build.

- [x] `pyproject.toml` with maturin build config (Rust + Python wheel) — added `nros.*.*` entry points, removed `ament_cargo` (handled by separate `colcon-cargo-ros2` package)
- [x] `Cargo.toml` for the Rust library (PyO3) — already existed
- [x] `setup.cfg` with colcon entry points for all `lang × platform` combinations — 14 build + 14 test entries
- [x] `colcon_nano_ros/__init__.py` with version — already existed
- [x] `colcon_nano_ros/task/__init__.py` — already existed
- [x] `colcon_nano_ros/task/nros/build.py` — `NrosBuildTask` parses `pkg.type` into `(lang, platform)`
- [x] `colcon_nano_ros/task/nros/test.py` — `NrosTestTask` stub
- [x] `src/lib.rs` — PyO3 module (already existed, shared with ament_cargo functionality)
- [x] `pip install -e .` succeeds and `colcon build` discovers the plugin — no conflicts with `colcon-cargo-ros2`
- [x] `colcon build` on a package with `<build_type>nros.rust.native</build_type>` invokes `NrosBuildTask`
- **Files**: `packages/codegen/` (colcon-nano-ros repo)

### 78.2 — NrosBuildTask: Rust native

Implement the build task for `nros.rust.native`.

- [x] Parse `pkg.type = "ros.nros.rust.native"` → `lang="rust"`, `platform="native"`
- [x] Run `cargo build --release` in the package directory
- [x] Find binary targets from `cargo metadata --no-deps` (with fallback to scanning `target/release/`)
- [x] Install binaries to `install/<pkg>/lib/<pkg>/`
- [x] Install `package.xml` to `install/<pkg>/share/<pkg>/`
- [x] Create ament environment hooks (PATH, LD_LIBRARY_PATH, AMENT_PREFIX_PATH)
- [x] Single Rust native package builds and installs via `colcon build`
- [x] Installed binary is executable from `install/<pkg>/lib/<pkg>/`
- **Files**: `colcon_nano_ros/task/nros/build.py`

### 78.3 — NrosBuildTask: C/C++ native

Implement the build task for `nros.c.native` and `nros.cpp.native`.

- [x] Run `cmake -S <pkg> -B <build_base> -DCMAKE_PREFIX_PATH=<install_base>` + `cmake --build` + `cmake --install`
- [x] Pass `CMAKE_PREFIX_PATH` so `find_package(NanoRos)` works
- [x] Install binary to `install/<pkg>/lib/<pkg>/` (via `cmake --install` with `CMAKE_INSTALL_PREFIX`)
- [x] Install `package.xml` to `install/<pkg>/share/<pkg>/`
- [x] Single C native package builds via `colcon build`
- [x] Single C++ native package builds via `colcon build`
- **Files**: `colcon_nano_ros/task/nros/build.py`

### 78.4 — Workspace-level message generation

Generate interface bindings once per workspace, shared by all packages.

- [x] Implement `PackageAugmentationExtensionPoint` (`NrosBindingAugmentation`) to collect all `<depend>` entries that are interface packages
- [x] Detect interface packages by checking for `.msg`/`.srv`/`.action` files in `AMENT_PREFIX_PATH`
- [x] Run `cargo nano-ros bindgen` into `build/nros_bindings/<interface_pkg>/` for Rust packages (single generation via async flag)
- [ ] Run `cargo nano-ros generate-cpp` for C/C++ packages — deferred (CMake's `nano_ros_generate_interfaces()` handles C/C++ codegen during the cmake build step)
- [ ] Set environment variables so Cargo finds the generated bindings during build — TODO (packages don't yet `use` the generated crates)
- [x] Two Rust packages depending on `std_msgs` share the same generated bindings (no duplicate codegen)
- [ ] Two C packages depending on `example_interfaces` share the same generated bindings — deferred (CMake per-package codegen)
- **Files**: `colcon_nano_ros/nros_augmentation/__init__.py`, `colcon_nano_ros/task/nros/build.py`

### 78.5 — NrosBuildTask: Rust cross-compilation (FreeRTOS, bare-metal)

Add cross-compilation support for embedded Rust targets.

- [x] Platform → target triple mapping in `PLATFORM_TARGETS` dict (freertos/baremetal/nuttx/threadx → `thumbv7m-none-eabi`)
- [x] Pass `--target <triple>` to `cargo build --release` for non-native platforms
- [x] Respect the package's `.cargo/config.toml` for linker, runner, build flags
- [x] Forward `FREERTOS_DIR`, `LWIP_DIR`, `FREERTOS_PORT`, `FREERTOS_CONFIG_DIR`, `NUTTX_DIR`, `THREADX_DIR`, etc. from environment
- [x] Install firmware ELF to `install/<pkg>/lib/<pkg>/`
- [x] FreeRTOS Rust package cross-compiles via `colcon build` (tested with board crate + `.cargo/config.toml`)
- [x] Cross-compiled binary is a valid ARM ELF (`ELF 32-bit LSB executable, ARM, EABI5`)
- **Files**: `colcon_nano_ros/task/nros/build.py`

### 78.6 — NrosBuildTask: C/C++ cross-compilation (FreeRTOS)

Add cross-compilation for C/C++ embedded targets.

- [x] Resolve `CMAKE_TOOLCHAIN_FILE` from platform name via `PLATFORM_TOOLCHAINS` dict + `NROS_TOOLCHAIN_DIR` env var
- [x] Pass `FREERTOS_DIR`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR` etc. from environment to CMake as `-D` flags
- [x] The user's `CMakeLists.txt` includes the platform support module and `find_package(NanoRos)`
- [x] FreeRTOS C package builds via `colcon build` (ARM ELF output verified)
- [ ] FreeRTOS C++ package builds via `colcon build` — same mechanism, not separately tested
- **Files**: `colcon_nano_ros/task/nros/build.py`

### 78.7 — NrosBuildTask: Zephyr

Add Zephyr support (`nros.rust.zephyr`, `nros.c.zephyr`).

- [x] Invoke `west build -b <board> -d <build_dir> -p auto <source_dir>` with `CMAKE_PREFIX_PATH`
- [x] Handle Zephyr board selection via `NROS_ZEPHYR_BOARD` env var (default: `native_sim`)
- [x] Pass `CMAKE_PREFIX_PATH` for `find_package(NanoRos)` and `nros_generate_interfaces()`
- [ ] Zephyr C talker builds via `colcon build` — implemented but not tested (requires `west init` + `west update` workspace)
- [ ] Zephyr Rust listener builds via `colcon build` — implemented but not tested
- **Files**: `colcon_nano_ros/task/nros/build.py`

### 78.8 — NrosTestTask

Implement test tasks for each platform.

- [x] `native`: run the binary, capture stdout/stderr, check exit code
- [x] `freertos` / `baremetal`: launch QEMU with `-icount shift=auto`, capture semihosting output, timeout (implemented, not yet E2E tested)
- [x] `zephyr`: run `native_sim` binary directly (hardware `west flash` not yet implemented)
- [x] JUnit XML output for `colcon test-result --all` — 3 tests, 0 failures
- [x] `colcon test` on a native package (Rust, C, C++) runs and reports pass/fail
- [ ] `colcon test` on a FreeRTOS package launches QEMU — implemented, needs E2E validation
- **Files**: `colcon_nano_ros/task/nros/test.py`

### 78.9 — `cargo nano-ros new` scaffolding

Add a `new` subcommand to `cargo nano-ros` that creates a colcon-compatible nano-ros package.

- [x] `cargo nano-ros new my_robot --lang rust --platform freertos` creates: `Cargo.toml` (board crate dep), `package.xml` (`nros.rust.freertos`), `config.toml`, `src/main.rs`
- [x] `cargo nano-ros new my_sensor --lang c --platform native` creates: `CMakeLists.txt` (`find_package(NanoRos)`), `package.xml` (`nros.c.native`), `src/main.c`
- [x] Also supports `--lang cpp` (generates `CMakeLists.txt` + `src/main.cpp`)
- [x] Generated native Rust package builds and passes `colcon build` + `colcon test`
- **Files**: `packages/codegen/packages/cargo-nano-ros/src/main.rs`

### 78.10 — Mixed-platform workspace E2E test

End-to-end test with a workspace containing packages targeting different platforms.

```
test_ws/src/
  brain/          nros.rust.native     (Linux host)
  controller/     nros.c.freertos      (FreeRTOS MCU)
```

- [x] `colcon build` builds both packages (Rust native + C native) in parallel
- [x] `colcon build --packages-select brain` builds only the native Rust package
- [ ] `--packages-select-build-type` not supported by colcon — filtering by build type would need a custom plugin (deferred)
- [x] Shared message bindings (`std_msgs`) generated once in `build/nros_bindings/`, shared by both
- [x] `colcon test` runs both and reports 2 tests, 0 failures via JUnit XML
- [ ] Test workspace checked into CI — deferred to 78.11
- **Files**: tested with temporary workspace (brain: nros.rust.native, controller: nros.c.native)

### 78.11 — Documentation and packaging

- [ ] `README.md` with installation, quick start, and configuration reference
- [ ] PyPI publishing workflow (maturin wheel via GitHub Actions)
- [ ] Book chapter: `book/src/guides/colcon.md` — getting started with colcon + nano-ros
- [ ] Document supported `lang × platform` combinations
- [ ] Document how board crate dependency selects the target board
- [ ] Document `config.toml` role (runtime config only, not parsed by colcon)
- **Files**: `README.md`, `book/src/guides/colcon.md`

## Acceptance Criteria

- [ ] `pip install colcon-nano-ros` installs the plugin (maturin wheel with bundled Rust library)
- [ ] `colcon build` builds Rust native packages (`nros.rust.native`)
- [ ] `colcon build` builds C native packages (`nros.c.native`)
- [ ] `colcon build` builds C++ native packages (`nros.cpp.native`)
- [ ] `colcon build` cross-compiles Rust FreeRTOS packages (`nros.rust.freertos`)
- [ ] `colcon build` cross-compiles C FreeRTOS packages (`nros.c.freertos`)
- [ ] `colcon build` works with Zephyr packages (`nros.rust.zephyr`, `nros.c.zephyr`)
- [ ] Workspace-level message generation — no redundant codegen for shared interfaces
- [ ] Mixed-platform workspace builds correctly (native + embedded in one workspace)
- [ ] `colcon test` runs tests on native and QEMU targets
- [ ] `colcon test-result --all` shows JUnit XML results
- [ ] `cargo nano-ros new` scaffolds a Rust package with `package.xml`
- [ ] `cargo nano-ros new` scaffolds a C package with `package.xml`
- [ ] `colcon build --packages-select-build-type ros.nros.rust.freertos` filters correctly
- [ ] Plugin does NOT parse `config.toml` — board crate / CMake module handles it
- [ ] No dependency on `colcon-cargo` — self-contained plugin

## Notes

- The colcon plugin does NOT parse `config.toml` or handle board configuration — that's the board crate's job. See design doc for rationale.
- The plugin is self-contained (no `colcon-cargo` dependency). Build logic lives in a bundled Rust library (PyO3/maturin), following the `colcon-cargo-ros2` pattern.
- `catkin_pkg` and colcon's `get_task_extension()` both accept dots in build type names. Verified experimentally.
- `colcon-ros` handles package identification from `package.xml` — no custom identification extension needed.
- The same `NrosBuildTask` class is registered under all `lang × platform` entry point names. It parses `pkg.type` at runtime to determine language and platform.
