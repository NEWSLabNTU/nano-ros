# Phase 69: Cross-Platform C/C++ Examples & Integration Tests

**Goal**: Bring C and C++ example and integration test coverage to parity with Rust across all platforms. Currently, C/C++ examples only exist on native (POSIX), Zephyr, and NuttX (C only). Multiple platforms lack C/C++ examples entirely, and no embedded platform has C/C++ integration tests.

**Status**: Not Started
**Priority**: Medium
**Depends on**: Phase 68 (Alloc-free C/C++ bindings), Phase 54.10 (FreeRTOS C examples, deferred)

## Overview

### Current State

The project has 130 examples across 10 platforms. Rust has broad coverage (86 examples) but C (32) and C++ (12) are concentrated on just a few platforms:

| Platform              | Rust (zenoh) | C (zenoh) | C++ (zenoh) | C (xrce) | Integration Tests |
|-----------------------|:---:|:---:|:---:|:---:|:---:|
| native (POSIX)        | 17  | 8   | 6   | 6   | C + C++ + XRCE |
| qemu-arm-baremetal    | 14  | --  | --  | --  | Rust only |
| qemu-arm-freertos     | 6   | --  | --  | --  | Rust only |
| qemu-arm-nuttx        | 6   | 6   | --  | --  | Rust only |
| qemu-esp32-baremetal  | 2   | --  | --  | --  | Rust only |
| qemu-riscv64-threadx  | 6   | --  | --  | --  | Rust only |
| esp32                 | 3   | --  | --  | --  | (none) |
| stm32f4               | 9   | --  | --  | --  | (none) |
| threadx-linux         | 6   | --  | --  | --  | Rust only |
| zephyr                | 7   | 6   | 6   | 6   | C + C++ + XRCE |

### Gap Analysis

**Missing C examples** (6 platforms, 3 use-case tiers):
- FreeRTOS QEMU (Phase 54.10 deferred)
- ThreadX Linux sim
- ThreadX RISC-V QEMU
- Bare-metal QEMU (Cortex-M3) -- requires `no_std` C, complex
- ESP32 QEMU -- requires `no_std` C
- STM32F4 -- requires `no_std` C, no QEMU networking

**Missing C++ examples** (8 platforms):
- All platforms except native and Zephyr

**Missing NuttX C++ examples**:
- NuttX has C but no C++ examples

**Missing integration tests for existing C/C++ examples**:
- NuttX C examples (6 exist, untested by `nuttx_qemu.rs`)

**Missing integration tests for future C/C++ examples**:
- All new C/C++ examples need corresponding test functions

### Scope Decisions

**In scope** (RTOS platforms — C and C++ via CMake):
- FreeRTOS, NuttX, ThreadX (Linux + QEMU) -- C and C++ examples
- All C/C++ examples use CMake as the build system (see Build System below)
- C++ uses `nros-cpp` freestanding mode (C++14, no `std` required)

**Out of scope** (bare-metal `no_std` C/C++ without RTOS):
- qemu-arm-baremetal, qemu-esp32-baremetal, esp32, stm32f4 -- these require `no_std` C with custom linker scripts, startup code, and per-board BSP integration. They are better addressed per-board as extensions to Phase 23 (Arduino) or dedicated embedded C phases.

### Build System

All C and C++ examples use **CMake** as the build system, following the same conventions as the existing native C/C++ examples:

1. **`find_package(NanoRos REQUIRED CONFIG)`** — locates the installed nros-c / nros-cpp library and the `nros-codegen` tool.
2. **`nano_ros_generate_interfaces(<pkg> <files>... [LANGUAGE CPP] SKIP_INSTALL)`** — generates message/service/action bindings at build time. C mode produces `.h`+`.c`, C++ mode produces `.hpp` headers + Rust FFI `.rs` glue (compiled into a per-package static lib via Cargo).
3. **Link targets**: `NanoRos::NanoRos` (C) or `NanoRos::NanoRosCpp` (C++), plus the generated `<pkg>__nano_ros_c` / `<pkg>__nano_ros_cpp` target.

For cross-compiled RTOS targets (FreeRTOS ARM, ThreadX RISC-V), each platform provides a CMake toolchain file under `examples/<platform>/cmake/` and a support module that compiles the RTOS kernel + networking stack. The per-example `CMakeLists.txt` includes these shared modules, then adds only its own source file and link targets.

```cmake
# Typical cross-compiled example CMakeLists.txt
cmake_minimum_required(VERSION 3.22)
set(CMAKE_TOOLCHAIN_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../../cmake/<platform>-toolchain.cmake")
project(freertos_cpp_talker LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../cmake/<platform>-support.cmake")

nano_ros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE CPP SKIP_INSTALL)

add_executable(freertos_cpp_talker src/main.cpp)
target_link_libraries(freertos_cpp_talker PRIVATE
    std_msgs__nano_ros_cpp NanoRos::NanoRosCpp <platform>_support)
```

For host-native RTOS targets (ThreadX Linux), the standard host compiler is used — no toolchain file needed.

## Work Items

C examples:
- [ ] 69.1 -- FreeRTOS C examples + integration tests
- [ ] 69.2 -- ThreadX Linux C examples + integration tests
- [ ] 69.3 -- ThreadX RISC-V QEMU C examples + integration tests
- [ ] 69.4 -- NuttX C integration tests (examples already exist)

C++ examples:
- [ ] 69.5 -- FreeRTOS C++ examples + integration tests
- [ ] 69.6 -- NuttX C++ examples + integration tests
- [ ] 69.7 -- ThreadX Linux C++ examples + integration tests
- [ ] 69.8 -- ThreadX RISC-V QEMU C++ examples + integration tests

Documentation:
- [ ] 69.9 -- Documentation

### 69.1 -- FreeRTOS C examples + integration tests

Add 6 C examples under `examples/qemu-arm-freertos/c/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

Each example is a CMake project that cross-compiles for `thumbv7m-none-eabi` using an ARM toolchain file. Uses Corrosion to build `nros-c` with `--features "rmw-zenoh,platform-freertos,ros-humble"`. Shared CMake modules under `examples/qemu-arm-freertos/cmake/` compile FreeRTOS kernel + lwIP + LAN9118 driver + startup code. Message bindings via `nano_ros_generate_interfaces(... LANGUAGE C)`.

Integration tests in `freertos_qemu.rs`: build tests for all 6 + E2E tests (`test_freertos_c_pubsub_e2e`, `test_freertos_c_service_e2e`, `test_freertos_c_action_e2e`).

This was originally Phase 54.10 (deferred pending Phase 49/68).

**Files**:
- `examples/qemu-arm-freertos/cmake/arm-none-eabi-toolchain.cmake`
- `examples/qemu-arm-freertos/cmake/freertos-support.cmake`
- `examples/qemu-arm-freertos/c/zenoh/talker/src/main.c` (+ 5 more)
- `examples/qemu-arm-freertos/c/zenoh/*/CMakeLists.txt`
- `examples/qemu-arm-freertos/c/zenoh/*/.gitignore`
- `packages/testing/nros-tests/tests/freertos_qemu.rs`

### 69.2 -- ThreadX Linux C examples + integration tests

Add 6 C examples under `examples/threadx-linux/c/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

ThreadX Linux sim uses POSIX sockets — builds with host compiler (no cross-compilation). CMake project using `find_package(NanoRos)` + `nano_ros_generate_interfaces()`. Links against `NanoRos::NanoRos`.

Integration tests in `threadx_linux.rs`.

**Files**:
- `examples/threadx-linux/c/zenoh/talker/src/main.c` (+ 5 more)
- `examples/threadx-linux/c/zenoh/*/CMakeLists.txt`
- `packages/testing/nros-tests/tests/threadx_linux.rs`

### 69.3 -- ThreadX RISC-V QEMU C examples + integration tests

Add 6 C examples under `examples/qemu-riscv64-threadx/c/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

Cross-compiles for `riscv64gc-unknown-none-elf` with ThreadX + NetX Duo. CMake project with RISC-V toolchain file. Shared CMake modules compile ThreadX + NetX Duo.

Integration tests in `threadx_riscv64_qemu.rs`.

**Files**:
- `examples/qemu-riscv64-threadx/cmake/riscv64-toolchain.cmake`
- `examples/qemu-riscv64-threadx/cmake/threadx-support.cmake`
- `examples/qemu-riscv64-threadx/c/zenoh/talker/src/main.c` (+ 5 more)
- `examples/qemu-riscv64-threadx/c/zenoh/*/CMakeLists.txt`
- `packages/testing/nros-tests/tests/threadx_riscv64_qemu.rs`

### 69.4 -- NuttX C integration tests (examples already exist)

The 6 NuttX C examples already exist under `examples/qemu-arm-nuttx/c/zenoh/` but have no integration tests.

Add C example tests to `nuttx_qemu.rs`: build tests for all 6 + E2E tests (`test_nuttx_c_pubsub_e2e`, `test_nuttx_c_service_e2e`, `test_nuttx_c_action_e2e`).

**Files**:
- `packages/testing/nros-tests/tests/nuttx_qemu.rs`

### 69.5 -- FreeRTOS C++ examples + integration tests

Add 6 C++ examples under `examples/qemu-arm-freertos/cpp/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

Each example is a CMake project that cross-compiles for `thumbv7m-none-eabi`. Uses `nros-cpp` freestanding mode (C++14, no `std`). Reuses the shared ARM toolchain and FreeRTOS support modules from `examples/qemu-arm-freertos/cmake/` (created in 69.1). Uses Corrosion to build `nros-cpp-ffi` with `--features "rmw-zenoh,platform-freertos,ros-humble"`. Message bindings via `nano_ros_generate_interfaces(... LANGUAGE CPP)`.

Integration tests in `freertos_qemu.rs`: build tests for all 6 + E2E tests (`test_freertos_cpp_pubsub_e2e`, `test_freertos_cpp_service_e2e`, `test_freertos_cpp_action_e2e`).

**Files**:
- `examples/qemu-arm-freertos/cpp/zenoh/talker/src/main.cpp` (+ 5 more)
- `examples/qemu-arm-freertos/cpp/zenoh/*/CMakeLists.txt`
- `examples/qemu-arm-freertos/cpp/zenoh/*/.gitignore`
- `packages/testing/nros-tests/tests/freertos_qemu.rs`

### 69.6 -- NuttX C++ examples + integration tests

Add 6 C++ examples under `examples/qemu-arm-nuttx/cpp/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

NuttX supports `std` so C++ examples can optionally use `NROS_CPP_STD` mode. Uses `nros-cpp` header-only library with `nros-cpp-ffi`. Cross-compiles for `armv7a-nuttx-eabi`. CMake project with NuttX toolchain. Message bindings via `nano_ros_generate_interfaces(... LANGUAGE CPP)`.

Integration tests in `nuttx_qemu.rs`.

**Files**:
- `examples/qemu-arm-nuttx/cpp/zenoh/talker/src/main.cpp` (+ 5 more)
- `examples/qemu-arm-nuttx/cpp/zenoh/*/CMakeLists.txt`
- `packages/testing/nros-tests/tests/nuttx_qemu.rs`

### 69.7 -- ThreadX Linux C++ examples + integration tests

Add 6 C++ examples under `examples/threadx-linux/cpp/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

ThreadX Linux sim — builds with host compiler. CMake project using `find_package(NanoRos)` + `nano_ros_generate_interfaces(... LANGUAGE CPP)`. Links against `NanoRos::NanoRosCpp`.

Integration tests in `threadx_linux.rs`.

**Files**:
- `examples/threadx-linux/cpp/zenoh/talker/src/main.cpp` (+ 5 more)
- `examples/threadx-linux/cpp/zenoh/*/CMakeLists.txt`
- `packages/testing/nros-tests/tests/threadx_linux.rs`

### 69.8 -- ThreadX RISC-V QEMU C++ examples + integration tests

Add 6 C++ examples under `examples/qemu-riscv64-threadx/cpp/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

Cross-compiles for `riscv64gc-unknown-none-elf`. Uses `nros-cpp` freestanding mode. Reuses RISC-V toolchain and ThreadX support modules from `examples/qemu-riscv64-threadx/cmake/` (created in 69.3). Message bindings via `nano_ros_generate_interfaces(... LANGUAGE CPP)`.

Integration tests in `threadx_riscv64_qemu.rs`.

**Files**:
- `examples/qemu-riscv64-threadx/cpp/zenoh/talker/src/main.cpp` (+ 5 more)
- `examples/qemu-riscv64-threadx/cpp/zenoh/*/CMakeLists.txt`
- `packages/testing/nros-tests/tests/threadx_riscv64_qemu.rs`

### 69.9 -- Documentation

- Update `CLAUDE.md` examples list with new C/C++ platform directories
- Update `book/src/platforms/*.md` pages to mention C/C++ example availability
- Update `book/src/getting-started/first-app-c.md` with cross-platform notes
- Update coverage matrix in this document

**Files**:
- `CLAUDE.md`
- `book/src/platforms/freertos.md`
- `book/src/platforms/nuttx.md`
- `book/src/platforms/threadx.md`

## Example Count After Completion

| Platform              | Rust | C  | C++ | Total |
|-----------------------|:----:|:--:|:---:|:-----:|
| native (POSIX)        | 28   | 14 | 6   | 48    |
| qemu-arm-baremetal    | 14   | -- | --  | 14    |
| qemu-arm-freertos     | 6    | +6 | +6  | 18    |
| qemu-arm-nuttx        | 6    | 6  | +6  | 18    |
| qemu-esp32-baremetal  | 2    | -- | --  | 2     |
| qemu-riscv64-threadx  | 6    | +6 | +6  | 18    |
| esp32                 | 3    | -- | --  | 3     |
| stm32f4               | 9    | -- | --  | 9     |
| threadx-linux         | 6    | +6 | +6  | 18    |
| zephyr                | 7    | 12 | 6   | 25    |
| **Total**             | 87   | 50 | 30  | 173   |

## Integration Test Count After Completion

| Platform              | Rust E2E | C E2E | C++ E2E |
|-----------------------|:--------:|:-----:|:-------:|
| native (POSIX)        | Yes      | Yes   | Yes     |
| qemu-arm-baremetal    | Yes      | --    | --      |
| qemu-arm-freertos     | Yes      | +Yes  | +Yes    |
| qemu-arm-nuttx        | Yes      | +Yes  | +Yes    |
| qemu-esp32-baremetal  | Yes      | --    | --      |
| qemu-riscv64-threadx  | Yes      | +Yes  | +Yes    |
| threadx-linux         | Yes      | +Yes  | +Yes    |
| zephyr                | Yes      | Yes   | Yes     |

## Out-of-Scope Platforms

These platforms are excluded from this phase because they require `no_std` C/C++ with per-board startup code, custom linker scripts, and no CMake toolchain:

| Platform | Reason |
|----------|--------|
| qemu-arm-baremetal | Cortex-M3 bare-metal: needs custom startup.s, linker script, semihosting |
| qemu-esp32-baremetal | RISC-V bare-metal: needs ESP-IDF or custom toolchain |
| esp32 | Hardware WiFi: needs ESP-IDF C integration |
| stm32f4 | Hardware SPI/ETH: needs per-board BSP, no QEMU networking |

These are better addressed by Phase 23 (Arduino precompiled library) or dedicated per-board phases.

## Acceptance Criteria

- [ ] All new C examples build and run on their target platform
- [ ] All new C++ examples build and run on their target platform (freestanding mode, no `std`)
- [ ] All C/C++ examples use CMake as the build system with `nano_ros_generate_interfaces()`
- [ ] Each work item includes integration tests (build + E2E) alongside its examples
- [ ] Existing NuttX C examples have integration tests (69.4)
- [ ] `just test-nuttx`, `just test-freertos`, `just test-threadx` include C/C++ tests
- [ ] `just quality` passes
- [ ] No heap allocation required in C examples (Phase 68 alloc-free executor)
