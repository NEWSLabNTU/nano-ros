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

Set up the `colcon-nano-ros` repo with maturin (Rust + Python) build.

- [ ] `pyproject.toml` with maturin build config (Rust + Python wheel)
- [ ] `Cargo.toml` for the Rust library (PyO3)
- [ ] `setup.cfg` with colcon entry points for all `lang × platform` combinations
- [ ] `colcon_nano_ros/__init__.py` with version
- [ ] `colcon_nano_ros/task/__init__.py`
- [ ] `colcon_nano_ros/task/build.py` — stub `NrosBuildTask` that parses `pkg.type`
- [ ] `colcon_nano_ros/task/test.py` — stub `NrosTestTask`
- [ ] `src/lib.rs` — stub PyO3 module
- [ ] `pip install -e .` succeeds and `colcon build` discovers the plugin
- [ ] `colcon build` on a package with `<build_type>nros.rust.native</build_type>` invokes `NrosBuildTask` (even if it does nothing yet)
- **Files**: `packages/codegen/` (colcon-nano-ros repo)

### 78.2 — NrosBuildTask: Rust native

Implement the build task for `nros.rust.native`.

- [ ] Parse `pkg.type = "ros.nros.rust.native"` → `lang="rust"`, `platform="native"`
- [ ] Run `cargo build --release` in the package directory
- [ ] Find binary targets from `cargo metadata --no-deps`
- [ ] Install binaries to `install/<pkg>/lib/<pkg>/`
- [ ] Install `package.xml` to `install/<pkg>/share/<pkg>/`
- [ ] Create ament environment hooks (PATH, LD_LIBRARY_PATH)
- [ ] Single Rust native package builds and installs via `colcon build`
- [ ] Installed binary is executable from `install/<pkg>/lib/<pkg>/`
- **Files**: `colcon_nano_ros/task/build.py`

### 78.3 — NrosBuildTask: C/C++ native

Implement the build task for `nros.c.native` and `nros.cpp.native`.

- [ ] Run `cmake -S <pkg> -B <build_base> -DCMAKE_PREFIX_PATH=<install_base>` + `cmake --build`
- [ ] Pass `CMAKE_PREFIX_PATH` so `find_package(NanoRos)` works
- [ ] Install binary to `install/<pkg>/lib/<pkg>/`
- [ ] Install `package.xml` to `install/<pkg>/share/<pkg>/`
- [ ] Single C native package builds via `colcon build`
- [ ] Single C++ native package builds via `colcon build`
- **Files**: `colcon_nano_ros/task/build.py`

### 78.4 — Workspace-level message generation

Generate interface bindings once per workspace, shared by all packages.

- [ ] Implement `PackageAugmentationExtensionPoint` to collect all `<depend>` entries that are interface packages
- [ ] Detect interface packages by checking for `.msg`/`.srv`/`.action` files in `AMENT_PREFIX_PATH` or workspace
- [ ] Run `cargo nano-ros generate-rust` into `build/nros_bindings/<interface_pkg>/` for Rust packages
- [ ] Run `cargo nano-ros generate-cpp` into `build/nros_bindings/<interface_pkg>/` for C/C++ packages
- [ ] Set environment variables / CMake variables so Cargo and CMake find the generated bindings
- [ ] Two Rust packages depending on `std_msgs` share the same generated bindings (no duplicate codegen)
- [ ] Two C packages depending on `example_interfaces` share the same generated bindings
- **Files**: `colcon_nano_ros/package_augmentation/__init__.py`, `src/lib.rs`

### 78.5 — NrosBuildTask: Rust cross-compilation (FreeRTOS, bare-metal)

Add cross-compilation support for embedded Rust targets.

- [ ] Platform → target triple mapping table in the Rust library:
  - `freertos` → `thumbv7m-none-eabi`
  - `baremetal` → `thumbv7m-none-eabi`
  - `nuttx` → `thumbv7m-none-eabi`
  - `threadx` → `thumbv7m-none-eabi` or `riscv64gc-unknown-none-elf`
- [ ] Pass `--target <triple>` to `cargo build --release`
- [ ] Respect the package's `.cargo/config.toml` for linker, runner, build flags
- [ ] Pass `FREERTOS_DIR`, `LWIP_DIR` etc. from environment to the build
- [ ] Install firmware ELF to `install/<pkg>/lib/<pkg>/`
- [ ] FreeRTOS Rust talker cross-compiles via `colcon build`
- [ ] Cross-compiled binary is a valid ARM ELF (`file` reports ARM)
- **Files**: `colcon_nano_ros/task/build.py`, `src/lib.rs`

### 78.6 — NrosBuildTask: C/C++ cross-compilation (FreeRTOS)

Add cross-compilation for C/C++ embedded targets.

- [ ] Resolve `CMAKE_TOOLCHAIN_FILE` from platform name (e.g., `freertos` → `arm-freertos-armcm3.cmake`)
- [ ] Pass `FREERTOS_DIR`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR` from environment to CMake
- [ ] The user's `CMakeLists.txt` includes the platform support module and `find_package(NanoRos)`
- [ ] FreeRTOS C action server builds via `colcon build`
- [ ] FreeRTOS C++ action client builds via `colcon build`
- **Files**: `colcon_nano_ros/task/build.py`

### 78.7 — NrosBuildTask: Zephyr

Add Zephyr support (`nros.rust.zephyr`, `nros.c.zephyr`).

- [ ] Invoke `west build` with appropriate board and config
- [ ] Handle Zephyr module registration (extra module path for nano-ros)
- [ ] Pass `CONFIG_NROS_*` Kconfig options from `config.toml` or environment
- [ ] Zephyr C talker builds via `colcon build`
- [ ] Zephyr Rust listener builds via `colcon build`
- **Files**: `colcon_nano_ros/task/build.py`

### 78.8 — NrosTestTask

Implement test tasks for each platform.

- [ ] `native`: run the binary, capture stdout/stderr, check exit code
- [ ] `freertos` / `baremetal`: launch QEMU with `-icount shift=auto`, capture semihosting output, timeout
- [ ] `zephyr`: `west flash` + serial capture, or `native_sim` build + run
- [ ] JUnit XML output for `colcon test-result --all`
- [ ] `colcon test` on a native package runs and reports pass/fail
- [ ] `colcon test` on a FreeRTOS package launches QEMU and reports pass/fail
- **Files**: `colcon_nano_ros/task/test.py`

### 78.9 — `cargo nano-ros new` scaffolding

Add a `new` subcommand to `cargo nano-ros` that creates a colcon-compatible nano-ros package.

- [ ] `cargo nano-ros new my_robot --lang rust --platform freertos` creates a directory with:
  - `Cargo.toml` (with board crate dependency based on platform + default board)
  - `package.xml` (with `<build_type>nros.rust.freertos</build_type>` and `<depend>` for common interfaces)
  - `config.toml` (with platform-appropriate defaults)
  - `src/main.rs` (minimal hello-world with board crate `run()`)
- [ ] `cargo nano-ros new my_sensor --lang c --platform native` creates:
  - `CMakeLists.txt` (with `find_package(NanoRos)`)
  - `package.xml` (with `<build_type>nros.c.native</build_type>`)
  - `src/main.c`
- [ ] Generated package builds successfully with `colcon build`
- **Files**: `packages/codegen/cargo-nano-ros/src/`

### 78.10 — Mixed-platform workspace E2E test

End-to-end test with a workspace containing packages targeting different platforms.

```
test_ws/src/
  brain/          nros.rust.native     (Linux host)
  controller/     nros.c.freertos      (FreeRTOS MCU)
```

- [ ] `colcon build` builds both packages in correct order
- [ ] `colcon build --packages-select brain` builds only the native package
- [ ] `colcon build --packages-select-build-type ros.nros.c.freertos` builds only the FreeRTOS package
- [ ] Shared message bindings (e.g., `std_msgs`) are generated once and used by both
- [ ] `colcon test` runs native test + QEMU test
- [ ] Test workspace checked into CI
- **Files**: test workspace directory, CI config

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
