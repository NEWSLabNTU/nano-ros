# Phase 78: Colcon Build Type for nano-ros

**Goal**: Enable ROS 2 users to build nano-ros packages (native, RTOS, bare-metal) with `colcon build` using a custom `nros.<lang>.<platform>` build type.

**Status**: Not Started
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

Set up the `colcon-nano-ros` repo with maturin (Rust + Python) build:

- `pyproject.toml` — maturin build config
- `setup.cfg` — colcon entry points (all `lang × platform` combinations)
- `Cargo.toml` — Rust library (PyO3)
- `colcon_nano_ros/__init__.py`
- `colcon_nano_ros/task/__init__.py`
- `colcon_nano_ros/task/build.py` — stub `NrosBuildTask`
- `colcon_nano_ros/task/test.py` — stub `NrosTestTask`
- Verify `pip install -e .` and colcon discovers the plugin
- **Files**: `packages/codegen/` (colcon-nano-ros repo)

### 78.2 — NrosBuildTask: Rust native

Implement the build task for `nros.rust.native`:

- Parse `pkg.type = "ros.nros.rust.native"` → `lang="rust"`, `platform="native"`
- Run `cargo build --release` in the package directory
- Install binary to `install/<pkg>/lib/<pkg>/`
- Install `package.xml` to `install/<pkg>/share/<pkg>/`
- Create ament environment hooks (PATH, LD_LIBRARY_PATH)
- **Test**: single Rust native package builds and installs via `colcon build`
- **Files**: `colcon_nano_ros/task/build.py`

### 78.3 — NrosBuildTask: C/C++ native

Implement the build task for `nros.c.native` and `nros.cpp.native`:

- Run `cmake -S <pkg> -B <build_base> -DCMAKE_PREFIX_PATH=<install_base>` + `cmake --build`
- Install binary to `install/<pkg>/lib/<pkg>/`
- Pass `CMAKE_PREFIX_PATH` so `find_package(NanoRos)` works
- **Test**: single C native package builds via `colcon build`
- **Files**: `colcon_nano_ros/task/build.py`

### 78.4 — Workspace-level message generation

Generate interface bindings once per workspace, shared by all packages:

- Implement `PackageAugmentationExtensionPoint` to collect all interface dependencies across workspace packages
- Find `.msg`/`.srv`/`.action` files from `AMENT_PREFIX_PATH` or workspace
- Run `cargo nano-ros generate-rust` and/or `cargo nano-ros generate-cpp` into `build/nros_bindings/<interface_pkg>/`
- Set environment variables so Cargo/CMake find the generated bindings
- **Test**: two packages depending on `std_msgs` share the same generated bindings
- **Files**: `colcon_nano_ros/package_augmentation/__init__.py`, `src/lib.rs`

### 78.5 — NrosBuildTask: Rust cross-compilation (FreeRTOS, bare-metal)

Add cross-compilation support for embedded Rust targets:

- Platform → target triple mapping:
  - `freertos` → `thumbv7m-none-eabi`
  - `baremetal` → `thumbv7m-none-eabi`
  - `nuttx` → `thumbv7m-none-eabi`
  - `threadx` → `thumbv7m-none-eabi` or `riscv64gc-unknown-none-elf`
- Pass `--target <triple>` to `cargo build`
- The board crate's `.cargo/config.toml` handles linker, runner, build flags
- Install firmware to `install/<pkg>/lib/<pkg>/`
- **Test**: FreeRTOS Rust package cross-compiles via `colcon build`
- **Files**: `colcon_nano_ros/task/build.py`

### 78.6 — NrosBuildTask: C/C++ cross-compilation (FreeRTOS)

Add cross-compilation for C/C++ embedded targets:

- Pass `CMAKE_TOOLCHAIN_FILE` based on platform
- Pass `FREERTOS_DIR`, `LWIP_DIR`, etc. from environment
- The user's `CMakeLists.txt` includes the platform support module
- **Test**: FreeRTOS C action server builds via `colcon build`
- **Files**: `colcon_nano_ros/task/build.py`

### 78.7 — NrosBuildTask: Zephyr

Add Zephyr support (`nros.rust.zephyr`, `nros.c.zephyr`):

- Invoke `west build` with `--board <board>` from config
- Handle Zephyr module registration (Kconfig, CMakeLists.txt)
- **Test**: Zephyr talker/listener build via `colcon build`
- **Files**: `colcon_nano_ros/task/build.py`

### 78.8 — NrosTestTask

Implement test tasks for each platform:

- `native`: run the binary directly, capture output
- `freertos`/`baremetal`: launch QEMU, capture semihosting output
- `zephyr`: `west flash` + serial capture, or `native_sim`
- JUnit XML output for `colcon test-result`
- **Files**: `colcon_nano_ros/task/test.py`

### 78.9 — `cargo nano-ros new` scaffolding

Add a `new` subcommand to `cargo nano-ros` that creates a nano-ros package:

- `cargo nano-ros new my_robot --lang rust --platform freertos`
- Generates: `Cargo.toml`, `package.xml` (with correct `build_type`), `config.toml`, `src/main.rs`
- Board crate dependency in `Cargo.toml` based on platform + default board
- **Files**: `packages/codegen/cargo-nano-ros/src/`

### 78.10 — Mixed-platform workspace E2E test

End-to-end test with a workspace containing multiple platforms:

```
test_ws/src/
  brain/          nros.rust.native     (Linux host)
  controller/     nros.c.freertos      (FreeRTOS MCU)
```

- `colcon build` builds both
- `colcon test` runs native test + QEMU test
- Verify shared message bindings work across languages
- **Files**: test workspace, CI

### 78.11 — Documentation and packaging

- User guide: getting started with colcon + nano-ros
- PyPI publishing (maturin wheel)
- Integration with nano-ros book (`book/src/guides/colcon.md`)
- **Files**: `README.md`, book chapter

## Acceptance Criteria

- [ ] `pip install colcon-nano-ros` installs the plugin
- [ ] `colcon build` builds Rust native packages
- [ ] `colcon build` builds C/C++ native packages
- [ ] `colcon build` cross-compiles Rust FreeRTOS packages
- [ ] `colcon build` cross-compiles C/C++ FreeRTOS packages
- [ ] `colcon build` works with Zephyr packages
- [ ] Workspace-level message generation (no redundant codegen)
- [ ] Mixed-platform workspace builds correctly
- [ ] `colcon test` runs tests (native + QEMU)
- [ ] `cargo nano-ros new` scaffolds a package with `package.xml`
- [ ] `colcon build --packages-select-build-type ros.nros.rust.freertos` filters correctly

## Notes

- The colcon plugin does NOT parse `config.toml` or handle board configuration — that's the board crate's job. See design doc for rationale.
- The plugin is self-contained (no `colcon-cargo` dependency). Build logic lives in a bundled Rust library (PyO3/maturin), following the `colcon-cargo-ros2` pattern.
- `catkin_pkg` and colcon's `get_task_extension()` both accept dots in build type names. Verified experimentally.
- `colcon-ros` handles package identification from `package.xml` — no custom identification extension needed.
- The same `NrosBuildTask` class is registered under all `lang × platform` entry point names. It parses `pkg.type` at runtime to determine language and platform.
