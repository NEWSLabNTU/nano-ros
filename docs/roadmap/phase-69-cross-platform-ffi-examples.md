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

**In scope** (RTOS platforms with `std` or CMake integration):
- FreeRTOS, NuttX, ThreadX (Linux + QEMU) -- C examples
- NuttX, ThreadX Linux -- C++ examples (where CMake toolchain is straightforward)

**Out of scope** (bare-metal `no_std` C/C++ is a different challenge):
- qemu-arm-baremetal, qemu-esp32-baremetal, esp32, stm32f4 -- these require `no_std` C with custom linker scripts, startup code, and per-board BSP integration. They are better addressed per-board as extensions to Phase 23 (Arduino) or dedicated embedded C phases.

## Work Items

- [ ] 69.1 -- FreeRTOS C examples (pubsub, service, action)
- [ ] 69.2 -- ThreadX Linux C examples (pubsub, service, action)
- [ ] 69.3 -- ThreadX RISC-V QEMU C examples (pubsub, service, action)
- [ ] 69.4 -- NuttX C++ examples (pubsub, service, action)
- [ ] 69.5 -- ThreadX Linux C++ examples (pubsub, service, action)
- [ ] 69.6 -- NuttX C integration tests
- [ ] 69.7 -- FreeRTOS C integration tests
- [ ] 69.8 -- ThreadX C integration tests
- [ ] 69.9 -- NuttX C++ integration tests
- [ ] 69.10 -- ThreadX Linux C++ integration tests
- [ ] 69.11 -- Documentation

### 69.1 -- FreeRTOS C examples (pubsub, service, action)

Add 6 C examples under `examples/qemu-arm-freertos/c/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

Each example cross-compiles `nros-c` with `--features "rmw-zenoh,platform-freertos,ros-humble"` for `thumbv7m-none-eabi`. Uses the FreeRTOS board crate's C startup path. Requires Phase 68 (alloc-free executor) so the C API doesn't need heap allocation.

This was originally Phase 54.10 (deferred pending Phase 49/68).

**Files**:
- `examples/qemu-arm-freertos/c/zenoh/talker/src/main.c` (+ 5 more)
- `examples/qemu-arm-freertos/c/zenoh/*/CMakeLists.txt`
- `examples/qemu-arm-freertos/c/zenoh/*/.gitignore`

### 69.2 -- ThreadX Linux C examples (pubsub, service, action)

Add 6 C examples under `examples/threadx-linux/c/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

ThreadX Linux sim uses POSIX sockets via the ThreadX raw-socket driver. C examples link against `nros-c` with `--features "rmw-zenoh,platform-threadx,ros-humble"`.

**Files**:
- `examples/threadx-linux/c/zenoh/talker/src/main.c` (+ 5 more)
- `examples/threadx-linux/c/zenoh/*/CMakeLists.txt`

### 69.3 -- ThreadX RISC-V QEMU C examples (pubsub, service, action)

Add 6 C examples under `examples/qemu-riscv64-threadx/c/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

Cross-compiles for `riscv64gc-unknown-none-elf` with ThreadX + NetX Duo.

**Files**:
- `examples/qemu-riscv64-threadx/c/zenoh/talker/src/main.c` (+ 5 more)
- `examples/qemu-riscv64-threadx/c/zenoh/*/CMakeLists.txt`

### 69.4 -- NuttX C++ examples (pubsub, service, action)

Add 6 C++ examples under `examples/qemu-arm-nuttx/cpp/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

NuttX supports `std` so C++ examples can use `nros-cpp` header-only library with `nros-cpp-ffi`. Cross-compiles for `armv7a-nuttx-eabi`.

**Files**:
- `examples/qemu-arm-nuttx/cpp/zenoh/talker/src/main.cpp` (+ 5 more)
- `examples/qemu-arm-nuttx/cpp/zenoh/*/CMakeLists.txt`

### 69.5 -- ThreadX Linux C++ examples (pubsub, service, action)

Add 6 C++ examples under `examples/threadx-linux/cpp/zenoh/`:
- `talker/`, `listener/`, `service-server/`, `service-client/`, `action-server/`, `action-client/`

ThreadX Linux sim can host C++ examples via CMake + `nros-cpp-ffi`.

**Files**:
- `examples/threadx-linux/cpp/zenoh/talker/src/main.cpp` (+ 5 more)
- `examples/threadx-linux/cpp/zenoh/*/CMakeLists.txt`

### 69.6 -- NuttX C integration tests

Add C example tests to `nuttx_qemu.rs`:
- `test_nuttx_c_talker_builds`, `test_nuttx_c_listener_builds`
- `test_nuttx_c_service_server_builds`, `test_nuttx_c_service_client_builds`
- `test_nuttx_c_action_server_builds`, `test_nuttx_c_action_client_builds`
- `test_nuttx_c_pubsub_e2e`
- `test_nuttx_c_service_e2e`
- `test_nuttx_c_action_e2e`

The 6 NuttX C examples already exist but have no integration tests.

**Files**:
- `packages/testing/nros-tests/tests/nuttx_qemu.rs`

### 69.7 -- FreeRTOS C integration tests

Add C example tests to `freertos_qemu.rs`:
- Build tests for all 6 C examples
- E2E tests: `test_freertos_c_pubsub_e2e`, `test_freertos_c_service_e2e`, `test_freertos_c_action_e2e`

**Files**:
- `packages/testing/nros-tests/tests/freertos_qemu.rs`

### 69.8 -- ThreadX C integration tests

Add C example tests to `threadx_linux.rs` and `threadx_riscv64_qemu.rs`:
- Build tests for all 6 C examples (both Linux sim and QEMU)
- E2E tests for pubsub, service, action

**Files**:
- `packages/testing/nros-tests/tests/threadx_linux.rs`
- `packages/testing/nros-tests/tests/threadx_riscv64_qemu.rs`

### 69.9 -- NuttX C++ integration tests

Add C++ example tests to `nuttx_qemu.rs` (after 69.4 creates the examples):
- Build tests for all 6 C++ examples
- E2E tests: `test_nuttx_cpp_pubsub_e2e`, `test_nuttx_cpp_service_e2e`, `test_nuttx_cpp_action_e2e`

**Files**:
- `packages/testing/nros-tests/tests/nuttx_qemu.rs`

### 69.10 -- ThreadX Linux C++ integration tests

Add C++ example tests to `threadx_linux.rs` (after 69.5 creates the examples):
- Build tests for all 6 C++ examples
- E2E tests for pubsub, service, action

**Files**:
- `packages/testing/nros-tests/tests/threadx_linux.rs`

### 69.11 -- Documentation

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
| qemu-arm-freertos     | 6    | +6 | --  | 12    |
| qemu-arm-nuttx        | 6    | 6  | +6  | 18    |
| qemu-esp32-baremetal  | 2    | -- | --  | 2     |
| qemu-riscv64-threadx  | 6    | +6 | --  | 12    |
| esp32                 | 3    | -- | --  | 3     |
| stm32f4               | 9    | -- | --  | 9     |
| threadx-linux         | 6    | +6 | +6  | 18    |
| zephyr                | 7    | 12 | 6   | 25    |
| **Total**             | 87   | 50 | 18  | 161   |

## Integration Test Count After Completion

| Platform              | Rust E2E | C E2E | C++ E2E |
|-----------------------|:--------:|:-----:|:-------:|
| native (POSIX)        | Yes      | Yes   | Yes     |
| qemu-arm-baremetal    | Yes      | --    | --      |
| qemu-arm-freertos     | Yes      | +Yes  | --      |
| qemu-arm-nuttx        | Yes      | +Yes  | +Yes    |
| qemu-esp32-baremetal  | Yes      | --    | --      |
| qemu-riscv64-threadx  | Yes      | +Yes  | --      |
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
- [ ] All new C++ examples build and run on their target platform
- [ ] Existing NuttX C examples have integration tests
- [ ] All new examples have corresponding integration tests
- [ ] `just test-nuttx`, `just test-freertos`, `just test-threadx` include C/C++ tests
- [ ] `just quality` passes
- [ ] No heap allocation required in C examples (Phase 68 alloc-free executor)
