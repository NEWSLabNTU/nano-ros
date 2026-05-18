# Phase 21: C API `no_std` Backend

**Status: REOPENED 2026-05-18 — bare-metal / ESP-IDF backend gap**

**Original closure (2024)** delivered no_std + alloc compilation via
the now-retired `shim-posix` / `shim-zephyr` feature axis. After the
Phase 137 / 138 / 144 cmake refactor and Phase 128/129 platform
consolidation, `nros-c`'s public platform features became
`platform-{posix,zephyr,freertos,nuttx,threadx}`. None of these
cover the bare-metal hosted-libc target that ESP-IDF (and therefore
arduino-esp32) needs. Per CLAUDE.md "Examples = Standalone Projects":
"no bare-metal C/C++ harness — nros-c/nros-cpp assume hosted RTOS for
startup/heap/libc". The Phase 139 `integrations/esp-idf/` shell sets
`NANO_ROS_PLATFORM=baremetal` but `packages/core/nros-c/CMakeLists.txt`
fatal-errors on that value, so the integration has never linked nros-c
end to end.

Phase 23 (Arduino precompiled lib for ESP32) blocks on this gap; the
reopened Phase 21 is now Phase 23's 23.0 prerequisite.

## Scope (reopened)

- [x] **21.6** Added `platform-esp-idf` feature to `nros-c`. Reuses
      the existing `freertos_alloc` allocator module
      (`pvPortMalloc` / `vPortFree` exported by ESP-IDF's FreeRTOS
      fork — bytes match upstream Apex); the cfg gate is now
      `any(feature = "platform-freertos", feature = "platform-esp-idf")`.
      The C platform port at
      `packages/core/nros-platform-esp-idf/` (Phase 121.3) supplies
      the canonical `nros_platform_*` ABI at link time. No new
      critical-section impl needed in nros-c — the IDF component
      handles it.
- [x] **21.7** Propagated `platform-esp-idf` through `nros`,
      `nros-node`, `nros-platform`, `nros-rmw-zenoh`, and
      `zpico-sys` (`esp-idf = ["freertos"]` — zenoh-pico's
      `system/freertos/system.c` is the right pick for ESP-IDF).
      No new `*-c-port` feature on `nros-platform-cffi`: the C
      symbols come from the IDF component, not from a Rust-built
      C port.
- [x] **21.8** `packages/core/nros-c/CMakeLists.txt` accepts
      `NANO_ROS_PLATFORM=esp-idf` (maps to
      `_platform_features = alloc panic-halt platform-esp-idf`).
      The root `CMakeLists.txt` updated its error message + adds
      `NROS_PLATFORM_ESP_IDF` compile-definition. Bare-metal
      ESP32 Rust-only path stays separate; the `esp-idf` value is
      the IDF-hosted variant only.
- [x] **21.9** `integrations/esp-idf/CMakeLists.txt` now sets
      `NANO_ROS_PLATFORM=esp-idf` and adds the IDF components
      `nros-platform-esp-idf` REQUIRES (freertos / esp_timer /
      esp_hw_support / esp_system / lwip) to the shell's
      `idf_component_register(REQUIRES …)` so the IDF dependency
      walker resolves them.
- [ ] **21.10** Smoke build through `just esp_idf build` (requires
      the extended SDK tier: `just setup tier=extended` or
      `just esp_idf setup`). Capture the first-time install
      footprint in `docs/development/sdk-tiers.md` if the tier
      classification needs to change.
- [ ] **21.10.A** ESP-IDF cross-build reaches Rust compile but the
      IDF cmake glue does not feed its component include directories
      (FreeRTOS / lwIP / esp_hw_support headers) to Corrosion-driven
      cc::Build invocations inside `zpico-sys/build.rs`. Result:
      `fatal error: FreeRTOS.h: No such file or directory` when
      zenoh-pico's `system/freertos/lwip.h` includes it. Need a
      mechanism to export the IDF `INCLUDE_DIRECTORIES` of `freertos`
      / `lwip` / `esp_hw_support` / `esp_system` components into
      `CFLAGS_<rust-target>` env so cc-rs picks them up. Likely
      lives in the Phase 23.2 `scripts/arduino/idf-builder/`
      CMakeLists.txt (use `idf_build_get_property(include_dirs
      INCLUDE_DIRECTORIES …)` + write CFLAGS env before Corrosion
      runs).

## Progress 2026-05-18

- Phase 21.6–21.9 landed (`platform-esp-idf` feature + cmake routing
  + `integrations/esp-idf/` repoint).
- ESP-IDF v5.3 installed locally (`just esp_idf setup`).
- `scripts/arduino/build-libnanoros.sh` reaches the Rust cross-compile
  pass for `riscv32imc-unknown-none-elf`. Resolved issues so far:
  - dedicated `[platform.esp-idf]` entry in
    `zenoh_platforms.toml` (no `required_env`, no
    `include_paths`, no arch flags).
  - `zpico-sys/build.rs` gets a `use_esp_idf` flag that bypasses
    the mps2-an385 FREERTOS_DIR / LWIP_DIR injection.
  - `.cargo/config.toml` adds
    `--cfg=portable_atomic_unsafe_assume_single_core` for the
    three ESP32-family Rust target triples.
  - `scripts/arduino/build-libnanoros.sh` scrubs
    `FREERTOS_DIR` / `LWIP_DIR` etc. from the env so direnv's
    vanilla-FreeRTOS values do not leak into the IDF build.
  - root + `nros-c` + `nros-cpp` + `nros-rmw-zenoh-staticlib`
    CMakeLists accept `NANO_ROS_PLATFORM=esp-idf`;
    `nros-cpp` Cargo gains a `platform-esp-idf` feature;
    `nros-platform/src/{lib,resolve}.rs` gain matching cfg gates.
  - `nros-rmw-dds/Cargo.toml` adds `platform-esp-idf` (nostd-runtime
    shape).
- Remaining blocker for 21.10: `scripts/arduino/idf-builder/`
  needs to export IDF component include dirs into
  `CFLAGS_<rust-target>` before Corrosion fires (see 21.10.A
  above).

## Historical record (original 2024 closure notes follow)

The C API crate (`nano-ros-c`) had ~30 functions that returned `NANO_ROS_RET_ERROR` when compiled without `std`. The `Zenoh*` type aliases (`ZenohPublisher`, `ZenohSession`, etc.) were re-exports of `Shim*` types from `nano-ros-transport`, which already worked in `no_std + alloc`. The 2024 barriers were:

1. `#[cfg(feature = "std")]` guards on all transport code (should be `alloc`)
2. `std::boxed::Box` usage (needs `alloc::boxed::Box`)
3. Hardcoded `features = ["zenoh"]` in dependencies (forces `shim-posix`, prevents `shim-zephyr`)
4. `Zenoh*` type aliases only available with `zenoh` feature, not generic `shim`

## Progress

| Task                                  | Status  | Description                                                        |
|---------------------------------------|---------|--------------------------------------------------------------------|
| 21.1 Cargo.toml feature restructuring | Done    | Decouple from hardcoded `zenoh`, add `alloc`/`shim-*` features     |
| 21.2 Compile-time guard               | Done    | `compile_error!` when `alloc` enabled without a backend            |
| 21.3 Type name migration              | Done    | `Zenoh*` → `Shim*` across all source files                         |
| 21.4 cfg guard + Box changes          | Done    | `std` → `alloc` guards, `std::boxed::Box` → `alloc::boxed::Box`    |
| 21.5 Verification                     | Done    | `just quality` ✓, `cargo check no_std` ✓, `just test-c` ✓         |

## 21.1 Cargo.toml Feature Restructuring

**Status: DONE**

**File**: `packages/core/nano-ros-c/Cargo.toml`

- [x] Add `alloc` feature: `["nano-ros/alloc", "nano-ros-transport/alloc"]`
- [x] Make `std` imply `alloc`: `["alloc", "nano-ros/std", "nano-ros-transport/std"]`
- [x] Add `shim-posix` feature: `["nano-ros/shim-posix", "nano-ros-transport/shim-posix"]`
- [x] Add `shim-zephyr` feature: `["nano-ros/shim-zephyr", "nano-ros-transport/shim-zephyr"]`
- [x] Set `default = ["std", "shim-posix"]`
- [x] Remove hardcoded `features = ["zenoh"]` from dependency declarations
- [x] Use `default-features = false` on all dependencies

Note: `alloc` and `shim-*` features already existed; the key changes were adding `shim-posix` to `default`, making `std` imply `alloc`, and removing `features = ["zenoh"]` from dependencies.

## 21.2 Compile-Time Guard

**Status: DONE**

**File**: `packages/core/nano-ros-c/src/lib.rs`

- [x] Add `compile_error!` for `alloc` without a transport backend

```rust
#[cfg(all(feature = "alloc", not(any(feature = "shim-posix", feature = "shim-zephyr"))))]
compile_error!(
    "nano-ros-c `alloc` requires a transport backend. Enable `shim-posix` or `shim-zephyr`."
);
```

## 21.3 Type Name Migration (`Zenoh*` → `Shim*`)

**Status: DONE**

`Zenoh*` names require the `zenoh` feature. `Shim*` names are available with any `shim-*` backend. All source files must migrate:

| Old name             | New name            |
|----------------------|---------------------|
| `ZenohSession`       | `ShimSession`       |
| `ZenohPublisher`     | `ShimPublisher`     |
| `ZenohSubscriber`    | `ShimSubscriber`    |
| `ZenohServiceServer` | `ShimServiceServer` |
| `ZenohServiceClient` | `ShimServiceClient` |
| `ZenohTransport`     | `ShimTransport`     |

**Files to update:**
- [x] `packages/core/nano-ros-c/src/support.rs`
- [x] `packages/core/nano-ros-c/src/publisher.rs`
- [x] `packages/core/nano-ros-c/src/subscription.rs`
- [x] `packages/core/nano-ros-c/src/service.rs`
- [x] `packages/core/nano-ros-c/src/action.rs`
- [x] `packages/core/nano-ros-c/src/executor.rs`

## 21.4 cfg Guard and Box Changes

**Status: DONE**

All transport-related code blocks need:
1. `#[cfg(feature = "std")]` → `#[cfg(feature = "alloc")]`
2. `#[cfg(not(feature = "std"))]` → `#[cfg(not(feature = "alloc"))]`
3. `std::boxed::Box` → `alloc::boxed::Box`

### support.rs
- [x] `nano_ros_support_init`: cfg `alloc`, `alloc::boxed::Box`, `ShimSession`
- [x] `nano_ros_support_fini`: cfg `alloc`, `alloc::boxed::Box::from_raw`, `ShimSession`
- [x] `get_session()`: cfg `alloc`, `ShimSession`
- [x] `get_session_mut()`: cfg `alloc`, `ShimSession`

### publisher.rs
- [x] `nano_ros_publisher_init_with_qos`: cfg `alloc`, Box, `ShimSession`
- [x] `nano_ros_publish_raw`: cfg `alloc`, `ShimPublisher`
- [x] `nano_ros_publisher_fini`: cfg `alloc`, Box, `ShimPublisher`

### subscription.rs
- [x] `nano_ros_subscription_init_with_qos`: cfg `alloc`, Box, `ShimSubscriber`
- [x] `nano_ros_subscription_fini`: cfg `alloc`, Box, `ShimSubscriber`

### service.rs
- [x] `nano_ros_service_init`: cfg `alloc`, Box, `ShimServiceServer`
- [x] `nano_ros_service_take_request`: cfg `alloc`, `ShimServiceServer`
- [x] `nano_ros_service_send_response`: cfg `alloc`, `ShimServiceServer`
- [x] `nano_ros_service_fini`: cfg `alloc`, Box, `ShimServiceServer`
- [x] `nano_ros_service_client_init`: cfg `alloc`, Box, `ShimServiceClient`
- [x] `nano_ros_service_call`: cfg `alloc`, `ShimServiceClient`
- [x] `nano_ros_service_client_fini`: cfg `alloc`, Box, `ShimServiceClient`

### action.rs
- [x] `nano_ros_action_server_init`: cfg `alloc`, Box
- [x] `nano_ros_action_server_send_goal_response`: cfg `alloc`
- [x] `nano_ros_action_server_publish_feedback`: cfg `alloc`
- [x] `nano_ros_action_server_publish_result`: cfg `alloc`
- [x] `nano_ros_action_server_fini`: cfg `alloc`, Box
- [x] `nano_ros_action_client_init`: cfg `alloc`, Box
- [x] `nano_ros_action_client_send_goal_request`: cfg `alloc`
- [x] `nano_ros_action_client_send_cancel_request`: cfg `alloc`
- [x] `nano_ros_action_client_get_result_request`: cfg `alloc`
- [x] `nano_ros_action_client_process_feedback`: cfg `alloc`
- [x] `nano_ros_action_client_fini`: cfg `alloc`, Box

### executor.rs
- [x] `process_subscription`: cfg `alloc`, `ShimSubscriber`
- [x] `process_service_request`: cfg `alloc`, `ShimServiceServer`
- [x] `sample_subscription_for_let`: cfg `alloc`, `ShimSubscriber`
- [x] `process_subscription_from_let`: cfg `alloc`
- [x] `sample_all_handles_for_let`: cfg `alloc`
- [x] `spin_some` LET blocks: cfg `alloc`
- [x] `spin_some` subscription dispatch: cfg `alloc`
- [x] `spin_some` service dispatch: cfg `alloc`

### Note on action.rs UUID generation
`nano_ros_goal_uuid_generate()` retains `#[cfg(feature = "std")]` / `#[cfg(not(feature = "std"))]` guards because it genuinely requires `std::time::SystemTime` and `std::sync::atomic::AtomicU64`. In `no_std` mode it returns zeroed UUIDs (a valid fallback).

### Note on trait imports
The trait imports (`Publisher`, `Subscriber`, `Session`, `Transport`, `ServiceServerTrait`, `ServiceClientTrait`, `QosSettings`, `TopicInfo`, `ServiceInfo`, `ActionInfo`) from `nano_ros_transport::traits` are always available — no feature gating needed.

## 21.5 Verification

**Status: DONE**

- [x] `just quality` — default (`std + shim-posix`) still works, all tests pass
- [x] `cargo check -p nano-ros-c --no-default-features --features alloc,shim-posix` — `no_std + alloc` type-checks (only expected `no_std` boilerplate errors: `global_allocator`, `panic_handler`)
- [x] `just test-c` — C integration tests pass (c-integration, c-codegen, c-msg-gen)

## Files Modified

| File                                    | Change                                                                   |
|-----------------------------------------|--------------------------------------------------------------------------|
| `packages/core/nano-ros-c/Cargo.toml`          | Remove hardcoded `zenoh`, `std` implies `alloc`, `shim-posix` in default |
| `packages/core/nano-ros-c/src/lib.rs`          | Add `compile_error!` guard                                               |
| `packages/core/nano-ros-c/src/support.rs`      | cfg `alloc`, Box, `ShimSession`                                          |
| `packages/core/nano-ros-c/src/publisher.rs`    | cfg `alloc`, Box, `ShimPublisher`                                        |
| `packages/core/nano-ros-c/src/subscription.rs` | cfg `alloc`, Box, `ShimSubscriber`                                       |
| `packages/core/nano-ros-c/src/service.rs`      | cfg `alloc`, Box, `ShimServiceServer/Client`                             |
| `packages/core/nano-ros-c/src/action.rs`       | cfg `alloc`, Box, all action types                                       |
| `packages/core/nano-ros-c/src/executor.rs`     | cfg `alloc`, `ShimSubscriber/ServiceServer`                              |
