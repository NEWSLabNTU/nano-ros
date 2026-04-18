# Phase 69: Cross-Platform C/C++ Examples & Integration Tests

**Goal**: Bring C and C++ example and integration test coverage to parity with Rust across all platforms. Currently, C/C++ examples only exist on native (POSIX), Zephyr, and NuttX (C only). Multiple platforms lack C/C++ examples entirely, and no embedded platform has C/C++ integration tests.

**Status**: In Progress (all C/C++ base examples + C++ action examples
across 10 platforms landed; 69.4 NuttX C E2E blocked on the `z_open`
→ `tcp_update_timer` hang investigation)
**Priority**: Medium
**Depends on**: Phase 68 (Alloc-free C/C++ bindings), Phase 54.10 (FreeRTOS C examples, deferred)

## Overview

### Current State

The project has 130 examples across 10 platforms. Rust has broad coverage (86 examples) but C (32) and C++ (12) are concentrated on just a few platforms:

| Platform             | Rust (zenoh) | C (zenoh) | C++ (zenoh) | C (xrce) | Integration Tests |
|----------------------|:------------:|:---------:|:-----------:|:--------:|:-----------------:|
| native (POSIX)       |      17      |     8     |      6      |    6     |  C + C++ + XRCE   |
| qemu-arm-baremetal   |      14      |    --     |     --      |    --    |     Rust only     |
| qemu-arm-freertos    |      6       |     6     |    4 (*)    |    --    |  Rust + C + C++   |
| qemu-arm-nuttx       |      6       |     6     |    4 (*)    |    --    |     Rust only     |
| qemu-esp32-baremetal |      2       |    --     |     --      |    --    |     Rust only     |
| qemu-riscv64-threadx |      6       |    --     |     --      |    --    |     Rust only     |
| esp32                |      3       |    --     |     --      |    --    |      (none)       |
| stm32f4              |      9       |    --     |     --      |    --    |      (none)       |
| threadx-linux        |      6       |    --     |     --      |    --    |     Rust only     |
| zephyr               |      7       |     6     |      6      |    6     |  C + C++ + XRCE   |

(*) C++ examples: talker, listener, service-server, service-client. Action examples
(action-server, action-client) are deferred — the C++ action API (`nros::ActionServer`,
`nros::ActionClient`) is not yet implemented in `nros-cpp`. The C action API is
alloc-free and works on FreeRTOS and NuttX (6 C examples each include actions).

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
- [x] 69.1 -- FreeRTOS C examples + integration tests
- [x] 69.2 -- ThreadX Linux C examples + integration tests
- [x] 69.3 -- ThreadX RISC-V QEMU C examples + integration tests
- [ ] 69.4 -- NuttX C integration tests (build tests pass; E2E tests timeout — z_open hang)

C++ examples (6 per platform now — 4 base + 2 action examples added as
Phase 77 / 83 unblocked them):
- [x] 69.5 -- FreeRTOS C++ examples + integration tests (6 examples incl. actions)
- [x] 69.6 -- NuttX C++ examples + integration tests (6 examples; action build tests `#[ignore]`d under the upstream libc gate)
- [x] 69.7 -- ThreadX Linux C++ examples + integration tests (6 examples; action build tests pass)
- [x] 69.8 -- ThreadX RISC-V QEMU C++ examples + integration tests (6 examples; action build tests pass)

Documentation:
- [x] 69.9 -- Documentation

### 69.1 -- FreeRTOS C examples + integration tests

Add 6 C examples under `examples/qemu-arm-freertos/c/zenoh/` using CMake cross-compilation for `thumbv7m-none-eabi`. This was originally Phase 54.10 (deferred pending Phase 49/68).

- [x] Create `examples/qemu-arm-freertos/cmake/arm-none-eabi-toolchain.cmake`
- [x] Create `examples/qemu-arm-freertos/cmake/freertos-platform.cmake` (FreeRTOS + lwIP + LAN9118 + startup via Corrosion)
- [x] Create `examples/qemu-arm-freertos/cmake/startup.c` (FreeRTOS startup + network init)
- [x] Create `examples/qemu-arm-freertos/c/zenoh/talker/` (CMakeLists.txt + src/main.c + .gitignore + config.toml)
- [x] Create `examples/qemu-arm-freertos/c/zenoh/listener/` (+ config.toml with client-role IP)
- [x] Create `examples/qemu-arm-freertos/c/zenoh/service-server/`
- [x] Create `examples/qemu-arm-freertos/c/zenoh/service-client/`
- [x] Create `examples/qemu-arm-freertos/c/zenoh/action-server/`
- [x] Create `examples/qemu-arm-freertos/c/zenoh/action-client/`
- [x] Add FreeRTOS C build tests to `freertos_qemu.rs`
- [x] Add FreeRTOS C E2E pub/sub test (`test_freertos_c_pubsub_e2e`)
- [x] Add FreeRTOS C E2E service test (`test_freertos_c_service_e2e`)
- [x] Add FreeRTOS C E2E action test (`test_freertos_c_action_e2e`)

### 69.2 -- ThreadX Linux C examples + integration tests

Add 6 C examples under `examples/threadx-linux/c/zenoh/`. ThreadX Linux sim uses POSIX sockets — builds with host compiler (no cross-compilation). CMake project using `find_package(NanoRos)`.

- [x] Create `examples/threadx-linux/cmake/threadx-support.cmake` (ThreadX + NetX Duo support, created in 69.7)
- [x] Create `examples/threadx-linux/c/zenoh/talker/` (CMakeLists.txt + src/main.c + .gitignore + config.toml)
- [x] Create `examples/threadx-linux/c/zenoh/listener/`
- [x] Create `examples/threadx-linux/c/zenoh/service-server/`
- [x] Create `examples/threadx-linux/c/zenoh/service-client/`
- [x] Create `examples/threadx-linux/c/zenoh/action-server/`
- [x] Create `examples/threadx-linux/c/zenoh/action-client/`
- [x] Add ThreadX Linux C build tests to `threadx_linux.rs`
- [x] Add ThreadX Linux C E2E pub/sub test
- [x] Add ThreadX Linux C E2E service test
- [x] Add ThreadX Linux C E2E action test

### 69.3 -- ThreadX RISC-V QEMU C examples + integration tests

Add 6 C examples under `examples/qemu-riscv64-threadx/c/zenoh/`. Cross-compiles for `riscv64gc-unknown-none-elf` with ThreadX + NetX Duo.

- [x] Create `cmake/toolchain/riscv64-threadx.cmake` (RISC-V cross-toolchain with rust-lld)
- [x] Create `examples/qemu-riscv64-threadx/cmake/threadx-riscv64-support.cmake` (ThreadX + NetX Duo + virtio-net + picolibc)
- [x] Create `examples/qemu-riscv64-threadx/cmake/startup.c` (bare-metal entry → tx_kernel_enter)
- [x] Add `threadx_riscv64` platform to nros-c/nros-cpp CMakeLists.txt
- [x] Add ThreadX RISC-V build to `install-local` justfile recipe
- [x] Create `examples/qemu-riscv64-threadx/c/zenoh/talker/` (+ 5 more C examples)
- [x] Fix compiler_builtins float ABI: strip soft-float objects at install time, use rust-lld wrapper
- [x] Add UART output for C examples (picolibc stdout + uart_putc)
- [x] Add C++ compat headers for picolibc (cstdio, cstdint, etc.)
- [x] Add ThreadX RISC-V C build tests to `threadx_riscv64_qemu.rs` (6 pass)
- [x] Add ThreadX RISC-V C++ build tests (talker, listener pass)
- [x] Fix ThreadX app thread startup (app_main via function pointer, TLS init, memset -fno-builtin)
- [x] Add ThreadX RISC-V C E2E tests (pubsub, service, action — all pass)
- [x] Add ThreadX RISC-V C++ E2E tests (pubsub — pass)

### 69.4 -- NuttX C integration tests (examples already exist)

The 6 NuttX C examples already exist under `examples/qemu-arm-nuttx/c/zenoh/`. Add integration tests.

- [x] Add NuttX C build tests to `nuttx_qemu.rs` (all 6 build: talker, listener, service-server/client, action-server/client)
- [x] Fix `nuttx_build_example()` to pass generated .c sources via `APP_EXTRA_SOURCES` env var to build.rs
- [ ] Add NuttX C E2E pub/sub test (`test_nuttx_c_pubsub_e2e`) — currently times out (z_open hang on ARM QEMU)
- [ ] Add NuttX C E2E service test (`test_nuttx_c_service_e2e`) — same z_open hang
- [ ] Add NuttX C E2E action test (`test_nuttx_c_action_e2e`) — same z_open hang

### 69.5 -- FreeRTOS C++ examples + integration tests

Add 4 C++ examples under `examples/qemu-arm-freertos/cpp/zenoh/` using `nros-cpp` freestanding mode (C++14). Action examples deferred until `nros-cpp` gains `ActionServer`/`ActionClient`.

- [x] Add `NanoRos::NanoRosCpp` target to `freertos-platform.cmake` (Corrosion + nros-cpp-ffi)
- [x] Create `examples/qemu-arm-freertos/cpp/zenoh/talker/` (CMakeLists.txt + src/main.cpp + .gitignore + config.toml)
- [x] Create `examples/qemu-arm-freertos/cpp/zenoh/listener/`
- [x] Create `examples/qemu-arm-freertos/cpp/zenoh/service-server/`
- [x] Create `examples/qemu-arm-freertos/cpp/zenoh/service-client/`
- [x] Add FreeRTOS C++ build tests to `freertos_qemu.rs`
- [x] Add FreeRTOS C++ E2E pub/sub test (`test_freertos_cpp_pubsub_e2e`)
- [x] Add FreeRTOS C++ E2E service test (`test_freertos_cpp_service_e2e`)
- [ ] Add action-server + action-client once `nros-cpp` has action support

### 69.6 -- NuttX C++ examples + integration tests

Add 4 C++ examples under `examples/qemu-arm-nuttx/cpp/zenoh/` using `nros-cpp` freestanding mode. Cross-compiles for `armv7a-nuttx-eabihf` via `nros-nuttx-ffi` cargo crate.

- [x] Create `examples/qemu-arm-nuttx/cmake/nuttx-platform.cmake` (NuttX codegen + `nuttx_build_example()`)
- [x] Create `examples/qemu-arm-nuttx/cmake/armv7a-nuttx-toolchain.cmake`
- [x] Create `examples/qemu-arm-nuttx/cmake/nros-nuttx-ffi/` (FFI crate: compiles C++ via cc-rs, links NuttX kernel)
- [x] Create `examples/qemu-arm-nuttx/cpp/zenoh/talker/` (CMakeLists.txt + src/main.cpp + .gitignore + config.toml)
- [x] Create `examples/qemu-arm-nuttx/cpp/zenoh/listener/`
- [x] Create `examples/qemu-arm-nuttx/cpp/zenoh/service-server/`
- [x] Create `examples/qemu-arm-nuttx/cpp/zenoh/service-client/`
- [x] Add NuttX C++ build tests to `nuttx_qemu.rs` (`test_nuttx_cpp_{talker,listener,service_server,service_client}_builds`)
- [x] Add NuttX C++ E2E pub/sub test (`test_nuttx_cpp_pubsub_e2e`)
- [x] Add NuttX C++ E2E service test (`test_nuttx_cpp_service_e2e`)
- [x] Create `examples/qemu-arm-nuttx/cpp/zenoh/action-server/` (commit `9bcb86f6`)
- [x] Create `examples/qemu-arm-nuttx/cpp/zenoh/action-client/` (commit `9bcb86f6`)
- [x] Add NuttX C++ action build tests (`test_nuttx_cpp_action_{server,client}_builds`, `#[ignore]`d under the upstream libc `_SC_HOST_NAME_MAX` gate shared with the existing NuttX C++ tests)

### 69.7 -- ThreadX Linux C++ examples + integration tests

Add 4 C++ examples under `examples/threadx-linux/cpp/zenoh/`. ThreadX Linux sim — builds with host compiler. Action examples deferred until `nros-cpp` gains `ActionServer`/`ActionClient`.

- [x] Create `examples/threadx-linux/cmake/threadx-support.cmake` (ThreadX + NetX Duo + driver + glue)
- [x] Add `threadx_linux` platform to `nros-c/CMakeLists.txt` and `nros-cpp/CMakeLists.txt`
- [x] Add ThreadX Linux build to `install-local` justfile recipe
- [x] Create `examples/threadx-linux/cpp/zenoh/talker/` (CMakeLists.txt + src/main.cpp + .gitignore + config.toml + package.xml)
- [x] Create `examples/threadx-linux/cpp/zenoh/listener/`
- [x] Create `examples/threadx-linux/cpp/zenoh/service-server/`
- [x] Create `examples/threadx-linux/cpp/zenoh/service-client/`
- [x] Add ThreadX Linux C++ build tests to `threadx_linux.rs`
- [x] Add ThreadX Linux C++ E2E pub/sub test
- [x] Add ThreadX Linux C++ E2E service test
- [x] Create `examples/threadx-linux/cpp/zenoh/action-server/` (commit `9bcb86f6`)
- [x] Create `examples/threadx-linux/cpp/zenoh/action-client/` (commit `9bcb86f6`)
- [x] Add ThreadX Linux C++ action build tests (`test_threadx_cpp_action_{server,client}_builds`)
- [ ] Add ThreadX Linux C++ action E2E test (follow-up)

### 69.8 -- ThreadX RISC-V QEMU C++ examples + integration tests

Add 4 C++ examples under `examples/qemu-riscv64-threadx/cpp/zenoh/`. Cross-compiles for `riscv64gc-unknown-none-elf` using `nros-cpp` freestanding mode. Action examples deferred until `nros-cpp` gains `ActionServer`/`ActionClient`.

- [x] Create `examples/qemu-riscv64-threadx/cpp/zenoh/talker/` (+ listener, service-server, service-client)
- [x] Add ThreadX RISC-V C++ build tests (talker, listener — pass)
- [x] Add ThreadX RISC-V C++ E2E pubsub test (pass)
- [ ] Add ThreadX RISC-V C++ service build + E2E tests
- [x] Create `examples/qemu-riscv64-threadx/cpp/zenoh/action-server/` (commit `9bcb86f6`)
- [x] Create `examples/qemu-riscv64-threadx/cpp/zenoh/action-client/` (commit `9bcb86f6`)
- [x] Add ThreadX RISC-V C++ action build tests (`test_rv64_cpp_action_{server,client}_builds`)
- [ ] Add ThreadX RISC-V C++ action E2E test (follow-up)

### 69.9 -- Documentation

- [x] Update `CLAUDE.md` phase table with Phase 69 status
- [x] Update `book/src/platforms/nuttx.md` to mention C/C++ examples
- [x] Update `book/src/platforms/freertos.md` to mention C/C++ examples
- [ ] Update `book/src/platforms/threadx.md` to mention C/C++ examples (after 69.2/69.7)
- [ ] Update `book/src/getting-started/first-app-c.md` with cross-platform notes
- Update coverage matrix in this document

**Files**:
- `CLAUDE.md`
- `book/src/platforms/freertos.md`
- `book/src/platforms/nuttx.md`
- `book/src/platforms/threadx.md`

## Example Count After Completion

With `nros-cpp` now providing `ActionServer` (Phase 83) and
`ActionClient` (Phase 77), the C++ count is 6 per RTOS platform
(talker, listener, service-server, service-client, action-server,
action-client) — matching the C count and the Zephyr / native totals.

| Platform              | Rust | C  | C++ | Total |
|-----------------------|:----:|:--:|:---:|:-----:|
| native (POSIX)        | 28   | 14 | 6   | 48    |
| qemu-arm-baremetal    | 14   | -- | --  | 14    |
| qemu-arm-freertos     | 6    |  6 |  6  | 18    |
| qemu-arm-nuttx        | 6    |  6 |  6  | 18    |
| qemu-esp32-baremetal  | 2    | -- | --  | 2     |
| qemu-riscv64-threadx  | 6    | +6 | +6  | 18    |
| esp32                 | 3    | -- | --  | 3     |
| stm32f4               | 9    | -- | --  | 9     |
| threadx-linux         | 6    | +6 | +6  | 18    |
| zephyr                | 7    | 12 | 6   | 25    |
| **Total**             | 87   | 50 | 28  | 171   |

Live count (as of this phase doc update): C++ is at 4 on qemu-arm-nuttx,
qemu-riscv64-threadx, and threadx-linux — the 6 missing examples
(action-client + action-server per platform × 3 platforms) are the
remaining 69.6 / 69.7 / 69.8 deliverables.

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

- [x] All new C examples build and run on their target platform
- [x] All new C++ examples build and run on their target platform (freestanding mode, no `std`)
- [x] All C/C++ examples use CMake as the build system with `nano_ros_generate_interfaces()`
- [ ] Each work item includes integration tests (build + E2E) alongside its examples
- [ ] Existing NuttX C examples have integration tests (69.4)
- [ ] `just test-nuttx`, `just test-freertos`, `just test-threadx` include C/C++ tests
- [ ] `just ci` passes
- [x] No heap allocation required in C examples (Phase 68 alloc-free executor)
- [ ] C++ action examples added once `nros-cpp` gains `ActionServer`/`ActionClient`
