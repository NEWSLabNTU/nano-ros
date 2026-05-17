# Phase 124 Test Triage - 2026-05-14

## Update - 2026-05-15 Phase 127.G refresh attempt

Full-matrix refresh was started after reinstalling `just`.

Verified gates and blockers:

- `just format`: passed.
- `just ci`: failed after static checks/examples passed and `test-all`
  produced nextest run id `b0ca0525-85ae-4931-ae76-529b41214b2c`.
- `just build-all`: advanced past the earlier codegen blocker after
  `packages/codegen` gained its own `play_launch_parser` dependency subtree.
- Build-only fixes landed during the attempt:
  - `justfile` and `just/threadx-riscv64.just` now honor
    `CARGO_TARGET_DIR` when copying/installing RMW static libraries.
  - `zpico_fill_session_zid` no longer links FreeRTOS/ThreadX builds against
    POSIX `clock_gettime`; it mixes `z_clock_now()` bytes instead. Zephyr now
    uses the upstream `sys_rand_get` ZID path.
- `CARGO_TARGET_DIR=/tmp/nano-ros-build-all-target just build-all` completed
  the workspace and example matrix, including FreeRTOS QEMU and ThreadX QEMU
  examples. It reached `build-test-fixtures`.
- Fixture build initially stopped on host disk pressure while linking
  `threadx_cpp_action_client`: `/usr/bin/ld: final link failed: No space left on device`.
  After disk was freed, `build-test-fixtures` was rerun and completed native,
  QEMU bare-metal, FreeRTOS, NuttX Rust, ThreadX Linux, and ThreadX RISC-V
  fixtures. The rerun was intentionally stopped during the Zephyr fixture tail
  so the repository could pull/rebase onto the latest upstream commits.
- `just test-all`: not rerun standalone because the refreshed
  `build-test-fixtures` run was interrupted before Zephyr completed;
  the `test-all` run inside `just ci` is fixture-prereq heavy and not an
  authoritative runtime failure inventory.

Fresh `just ci` evidence:

- JUnit: `target/nextest/default/junit.xml`
- Logs: `test-logs/latest/`
- C codegen log: `test-logs/latest/c-codegen.log`
- Nextest: 824 tests run, 519 passed, 305 failed, 11 skipped.
- Harness environment skips counted inside failures: 27.
- Real failures after subtracting harness env skips: 278, but the majority are
  blocked fixture/configure prerequisites, not runtime behavior.

Failure separation from the partial run:

| Class | Count | Signal |
|---|---:|---|
| nextest skipped | 11 | Explicit skipped tests. |
| Harness env skip | 27 | 10 zenoh-pico ARM build unavailable, 9 XRCE agent unavailable, 4 ROS 2 unavailable, 3 DDS talker binary missing, 1 ThreadX-Linux DDS prerequisite missing. |
| Fixture not prebuilt | 231 | Requires completed `just build-all` before retesting. |
| Native C/C++ configure missing `NanoRosConfig.cmake` | 44 | Caused by incomplete `install-local-posix`. |
| Other QEMU/runtime/build failures | 3 | Residual partial-run signal. |

Build-only blocker:

- Earlier missing `~/repos/play_launch` path dependency is resolved by the
  codegen submodule's own `play_launch_parser` dependency subtree.
- Earlier FreeRTOS QEMU `clock_gettime` link failure is resolved; talker,
  listener, service, and action examples compile.
- No current source/link blocker is known before the Zephyr fixture tail. The
  latest fixture run was stopped for repository synchronization, not because of
  a new build failure.

## Update - 2026-05-15 after parent/submodule pull

Repository sync:

- Parent branch `main` was fetched from `origin/main` and merged locally.
- Merge commit: `255046a2 Merge remote-tracking branch 'origin/main'`.
- `packages/codegen` submodule was updated to the merged parent pointer:
  `3069524eb1e4b8d33da0de77a9e83df7681aac36`.
- New upstream material from the pull is Phase 126 orchestration planning and
  codegen schema work; re-run the full gate before treating the older Phase 124
  full-matrix counts as current.

Focused ESP32 allocation fix:

- Commit: `8094047c fix(esp32): avoid subscriber heap allocation`.
- Verification:
  - `just format`
  - `cargo check -p nros-rmw-cffi`
  - `cargo build --release` in
    `examples/qemu-esp32-baremetal/rust/zenoh/listener`
  - `cargo build --release` in
    `examples/qemu-esp32-baremetal/rust/zenoh/talker`
  - `just esp32 test --no-capture`
- Result: ESP32 listener no longer panics on subscriber creation. It reaches
  `Subscriber declared` / `Waiting for messages...` with the original 64 KiB
  ESP32 runtime heap.
- Remaining focused ESP32 status: 9 tests run, 6 passed, 3 failed, 0 skipped.
  The 3 failures are now message-delivery failures, not allocation failures.

Remaining parallel work groups:

| Group | Scope | Current signal | Suggested owner output |
|---|---|---|---|
| ESP32 Zenoh delivery | `esp32_emulator::{test_esp32_talker_listener_e2e,test_esp32_to_native,test_native_to_esp32}` | QEMU boots, listener subscribes, but ESP32/native paths deliver 0 messages. | Identify whether the break is router discovery, TCP connect/session open, publish path, or smoltcp polling. Include QEMU/router logs and one minimal focused fix. |
| RTOS/QEMU platform E2E | FreeRTOS, NuttX, ThreadX Linux/RISC-V, baremetal DDS, platform DDS runtime tests | Largest bucket in the last full `just ci`: 39 failures. | Split by platform first; report whether each is build fixture, boot, network, or protocol handshake. |
| Zephyr runtime/E2E | Zephyr native/host, DDS, XRCE runtime cases | Last full `just ci`: 29 failures; build/smoke was mostly passing. | Separate host/board boot failures from DDS/XRCE message-flow failures; preserve exact west/QEMU logs. |
| Bare-metal Zenoh QEMU | RTIC action, RTIC service, serial pub/sub | Last full `just ci`: 3 failures. | Check whether failures share router/session readiness or serial transport framing. |
| DDS native action | Native DDS action server/client E2E | Last full `just ci`: 1 failure. | Produce focused action logs and compare with passing Zenoh/XRCE action paths. |
| ROS 2 lifecycle interop | Lifecycle full-cycle interop | Last full `just ci`: 1 failure. | Confirm whether this is graph discovery, transition service, or state-observation timing. |
| Full-matrix refresh | `just ci`, then `just build-all`, then `just test-all` as needed | Older counts are pre-Phase-126 pull and pre-ESP32 allocation fix. | Produce a fresh categorized JUnit/log summary before broad platform work branches too far. |

## Update - 2026-05-15

Focused verification after CFFI message-info propagation fix:

- `cargo test -p nros-rmw-cffi --test rust_adapter --features alloc`
- `cargo nextest run -p nros-tests --test nano2nano test_sequence_number_increment --no-capture`
- `cargo nextest run -p nros-tests --test nano2nano test_gid_consistency --no-capture`
- `cargo nextest run -p nros-tests --test zero_copy test_zero_copy_message_info --no-capture`

Newly resolved in focused runs:

- `nano2nano::test_sequence_number_increment`
- `nano2nano::test_gid_consistency`
- `zero_copy::test_zero_copy_message_info`

Focused verification after CFFI safety-status propagation fix:

- `cargo check -p nros-rmw-cffi --no-default-features`
- `cargo test -p nros-rmw-cffi --test rust_adapter --features alloc,safety-e2e`
- `cargo nextest run -p nros-tests --test native_api test_native_talker_listener_communication --no-capture`
- `cargo nextest run -p nros-tests --test native_api test_native_service_communication --no-capture`
- `cargo nextest run -p nros-tests --test safety_e2e --no-capture`

Newly resolved in focused runs:

- `native_api::test_native_talker_listener_communication::lang_1_Language__C`
- `native_api::test_native_talker_listener_communication::lang_2_Language__Cpp`
- `native_api::test_native_service_communication::lang_1_Language__C`
- `native_api::test_native_service_communication::lang_2_Language__Cpp`
- `safety_e2e::test_safety_e2e_talker_listener`
- `safety_e2e::test_safety_talker_standard_listener`

Focused verification after rebuilding Zenoh stress-test fixtures:

- `cargo nextest run -p nros-tests --test large_msg --no-capture`

Newly resolved in focused runs:

- `large_msg::test_zenoh_e2e_integrity`
- `large_msg::test_zenoh_e2e_large_receive`
- `large_msg::test_zenoh_throughput_100hz`
- `large_msg::test_zenoh_throughput_burst`

Focused verification after rebuilding native RTIC fixtures and fixing TLS
feature/config propagation:

- `cargo nextest run -p nros-tests --test nano2nano test_rtic_pattern_action test_rtic_pattern_service test_rtic_pattern_communication test_tls_talker_listener_communication --no-capture`

Newly resolved in focused runs:

- `nano2nano::test_rtic_pattern_action`
- `nano2nano::test_rtic_pattern_communication`
- `nano2nano::test_rtic_pattern_service`
- `nano2nano::test_tls_talker_listener_communication`

Notes:

- RTIC failures were stale prebuilt native RTIC fixtures; rebuilding the
  release fixtures resolved pub/sub, service, and action.
- TLS required forwarding `link-tls` to `nros-rmw-zenoh`, enabling `std` on
  the native listener backend so env config is available, and mirroring TLS
  env properties into the TLS locator for zenoh-pico open.

Refreshed full-gate result after the TLS config guard fix:

- Command: `just ci`
- Nextest run id: `e14540fe-d8cc-47be-9c28-a5f4540d95b9`
- JUnit: `target/nextest/default/junit.xml`
- Logs: `test-logs/latest/`

Quality/build gates verified in this run:

- `just format`
- `just ci` static portions: formatting, clippy, example check matrix,
  C/C++/Python checks, `zenohd`, doctests, Miri, C codegen, and C message
  generation.

Runtime summary:

- Nextest: 816 tests run, 732 passed, 84 failed, 11 skipped.
- Harness-reported environment skip: `ThreadX-Linux DDS prerequisites not
  available`.
- Real failures: 83 of 84 total failures, because the ThreadX-Linux DDS
  prerequisite case is counted as an environment skip by the test harness.
- Doctests: 1 passed, 4 ignored.
- Miri: all selected tests passed; one clock test ignored under Miri.
- C codegen and C message generation passed.

Current failure buckets:

| Category | Failures | Notes |
|---|---:|---|
| RTOS/QEMU platform E2E | 39 | FreeRTOS, NuttX, ThreadX Linux/RISC-V, baremetal DDS, and platform DDS runtime cases. |
| Zephyr runtime/E2E | 29 | Zephyr native/host E2E and DDS/XRCE runtime cases. Build/smoke cases pass outside failing runtime handshakes. |
| ESP32 emulator E2E | 6 | QEMU listener/talker build/boot plus ESP32/native bridge communication; the immediate build error is unresolved ESP32 links to `clock_gettime` and `__atomic_fetch_add_4` from `zpico_init_with_config`. |
| XRCE runtime E2E | 5 | Native C XRCE starts/talker-listener plus Rust XRCE action/large-message cases. |
| Bare-metal Zenoh QEMU E2E | 3 | RTIC action, RTIC service, and serial pub/sub. |
| DDS native runtime E2E | 1 | Native DDS action server/client E2E. |
| ROS 2 lifecycle interop | 1 | Lifecycle full-cycle interop. |

Native behavior resolved in the full run:

- Native API C/C++ pub/sub and service cases now pass.
- Safety, zero-copy, large-message Zenoh, QoS, params, executor, multi-node,
  services, RTIC pattern, sequence/GID metadata, and TLS talker/listener cases
  now pass.

Focused verification after the ESP32 session-ID portability and CMake XRCE
linkage fixes:

- `cargo build --release` in
  `examples/qemu-esp32-baremetal/rust/zenoh/listener`
- `cargo build --release` in
  `examples/qemu-esp32-baremetal/rust/zenoh/talker`
- `cargo nextest run -p nros-tests --test esp32_emulator test_esp32_qemu_listener_builds test_esp32_qemu_talker_builds test_esp32_qemu_talker_boots test_esp32_talker_listener_e2e test_esp32_to_native test_native_to_esp32 --no-capture`
- `just install-local`
- `just xrce test-c --no-capture`
- `just xrce test --no-capture`

Newly resolved in focused runs:

- `esp32_emulator::test_esp32_qemu_listener_builds`
- `esp32_emulator::test_esp32_qemu_talker_boots`
- `esp32_emulator::test_esp32_qemu_talker_builds`
- `c_xrce_api::test_c_xrce_listener_starts`
- `c_xrce_api::test_c_xrce_talker_listener_communication`
- `c_xrce_api::test_c_xrce_talker_starts`
- `xrce::test_xrce_action_fibonacci`
- `xrce::test_xrce_large_message_publish`

Still failing in focused ESP32 E2E:

- `esp32_emulator::test_esp32_talker_listener_e2e`
- `esp32_emulator::test_esp32_to_native`
- `esp32_emulator::test_native_to_esp32`

Notes:

- ESP32 build/boot failures were caused by the C shim's session-zid helper
  using C11 atomics and `clock_gettime` in RV32 bare-metal builds. The shim now
  uses a plain counter for single-threaded smoltcp/serial builds and the
  smoltcp clock for entropy.
- C XRCE startup failures were caused by the installed `NanoRos::NanoRos`
  target linking `NrosRmwXrce::NrosRmwXrce` without the common whole-archive
  wrapper, so the backend registration ctor was dead-stripped. The C XRCE
  binaries now contain `nros_rmw_xrce_register`.
- The remaining focused ESP32 E2E failures happen before ESP32 traffic starts:
  `ZenohRouter::start` times out while starting `zenohd`.

Source run:

- Command: `just ci`
- Nextest run id: `bf522883-71fa-4168-8e97-09da545d1447`
- JUnit: `target/nextest/default/junit.xml`
- Logs: `test-logs/latest/`

Quality/build gates verified in this run:

- `just format`
- `just ci` static portions: formatting, clippy, example check matrix,
  C/C++/Python checks, `zenohd`, doctests, Miri, C codegen, and C message
  generation.

Runtime summary:

- Nextest: 816 tests run, 716 passed, 100 failed, 11 skipped.
- Harness-reported environment skip: `ThreadX-Linux DDS prerequisites not
  available`.
- Doctests: 1 passed, 4 ignored.
- Miri: all selected tests passed; one clock test ignored under Miri.
- C codegen and C message generation passed.

Resolved since the first snapshot:

- Native Zenoh service tests now pass:
  `services::test_service_request_response`,
  `services::test_service_multiple_sequential_calls`, and
  `services::test_service_server_multiple_clients`.
- Native talker/listener and QoS smoke tests now pass:
  `nano2nano::test_talker_listener_communication`,
  `qos::test_qos_keyexpr_encoding`, and the QoS compatibility/reliability
  tests that were previously in the failure bucket.
- Native multi-node/executor/error-handling cases from the initial native
  Zenoh bucket now pass in this run.

Current failure buckets:

| Category | Failures | Notes |
|---|---:|---|
| RTOS/QEMU platform E2E | 39 | FreeRTOS, NuttX, ThreadX Linux/RISC-V, baremetal DDS, and platform DDS runtime cases. |
| Zephyr runtime/E2E | 29 | Zephyr native/host E2E and DDS/XRCE runtime cases; build/smoke cases mostly pass. |
| Native Zenoh/router behavior | 18 | Remaining native failures are action/RTIC, TLS, GID/sequence, large-message throughput/integrity, native C/C++ communication, safety, and zero-copy metadata. |
| ESP32 emulator E2E | 6 | QEMU listener/talker build/boot plus ESP32/native bridge communication. |
| Bare-metal Zenoh QEMU E2E | 3 | RTIC action, RTIC service, and serial pub/sub. |
| XRCE C runtime E2E | 3 | C XRCE starts/talker-listener cases. |
| DDS native runtime E2E | 1 | Native DDS action server/client E2E. |
| ROS 2 lifecycle interop | 1 | Lifecycle full-cycle interop. |

Current native-priority failures:

- None remaining from the focused native-priority set after the RTIC/TLS
  follow-up and earlier C++ action focused pass. Re-run `just ci` to refresh
  the full bucket counts.

Next priority:

1. Re-run `just ci` when ready to refresh the full failure inventory.
2. Defer platform E2E buckets until the focused native behavior stays stable
   in a full run.

Source run:

- Command: `just ci`
- Nextest run id: `5dd76ba1-b148-4b04-9300-3b27f606bc0a`
- JUnit: `target/nextest/default/junit.xml`
- Logs: `test-logs/latest/`

Quality/build gates verified before this runtime failure set:

- `just format`
- `just ci` static portions: formatting, clippy, example check matrix,
  C/C++/Python checks, `zenohd`, doctests, Miri, C codegen, and C message
  generation.
- `just build-all`
- Standalone `just zephyr build-fixtures` after the C++ generated-config fix.

Runtime summary from the `just ci` runtime matrix:

- Nextest: 815 tests run, 687 passed, 128 failed, 11 skipped.
- Reported environment skip: `ThreadX-Linux DDS prerequisites not available`.
- Real failures: 127 of 128 total failures, because the ThreadX-Linux DDS
  prerequisite case is counted as an environment skip by the test harness.
- Doctests: 1 passed, 4 ignored.
- Miri: all selected tests passed; one clock test ignored under Miri.
- C codegen and C message generation passed.

## Category Summary

| Category | Failures | Notes |
|---|---:|---|
| Native Zenoh/router behavior | 44 | Fastest high-value cluster: services, QoS, multi-node, native C/C++ interop, large-message, safety, and zero-copy tests. |
| RTOS/QEMU platform E2E | 39 | FreeRTOS, NuttX, ThreadX Linux/RISC-V, baremetal DDS, and platform DDS runtime cases. |
| Zephyr runtime/E2E | 29 | Zephyr boot/runtime and native/Zephyr cross-language/XRCE/DDS runtime cases. Fixture builds now pass; DDS C/C++ boot tests are no longer in the failure set. |
| ROS 2/RMW interop and discovery | 6 | Discovery visibility plus lifecycle interop. |
| Bare-metal Zenoh QEMU E2E | 3 | RTIC action, RTIC service, and serial pub/sub. |
| ESP32 emulator E2E | 3 | ESP32/native bridge communication. |
| XRCE C runtime E2E | 3 | C XRCE starts/talker-listener cases. Rust XRCE service and large-message runtime cases passed in this run. |
| DDS native runtime E2E | 1 | Native DDS action server/client E2E. |

## Native Zenoh/router Behavior

Executor and error handling:

- `error_handling::test_debug_logging_overhead`
- `error_handling::test_listener_router_disconnect`
- `error_handling::test_router_reconnect`
- `executor::test_executor_multiple_timers_via_publishers`
- `executor::test_mixed_callbacks`

Multi-node, nano-to-nano, and QoS:

- `multi_node::test_concurrent_startup`
- `multi_node::test_many_to_many`
- `multi_node::test_multiple_publishers_single_topic`
- `multi_node::test_multiple_subscribers_single_topic`
- `multi_node::test_publisher_scalability`
- `multi_node::test_subscriber_scalability`
- `multi_node::test_sustained_communication`
- `nano2nano::test_gid_consistency`
- `nano2nano::test_rtic_pattern_action`
- `nano2nano::test_rtic_pattern_communication`
- `nano2nano::test_rtic_pattern_service`
- `nano2nano::test_sequence_number_increment`
- `nano2nano::test_talker_listener_communication`
- `nano2nano::test_tls_talker_listener_communication`
- `qos::test_qos_compatible_settings`
- `qos::test_qos_keyexpr_encoding`
- `qos::test_qos_multiple_subscribers`
- `qos::test_qos_reliable_delivery`

Large message, native API, services, params, safety, and zero-copy:

- `large_msg::test_zenoh_e2e_integrity`
- `large_msg::test_zenoh_e2e_large_receive`
- `large_msg::test_zenoh_throughput_100hz`
- `large_msg::test_zenoh_throughput_burst`
- `native_api::test_c_rust_pubsub_interop`
- `native_api::test_cpp_action_communication`
- `native_api::test_cpp_action_goal_rejection`
- `native_api::test_cpp_rust_pubsub_interop`
- `native_api::test_cpp_rust_service_interop`
- `native_api::test_native_service_communication::lang_1_Language__C`
- `native_api::test_native_service_communication::lang_2_Language__Cpp`
- `native_api::test_native_talker_listener_communication::lang_1_Language__C`
- `native_api::test_native_talker_listener_communication::lang_2_Language__Cpp`
- `params::test_param_integer_type`
- `safety_e2e::test_safety_e2e_talker_listener`
- `safety_e2e::test_safety_talker_standard_listener`
- `services::test_service_multiple_sequential_calls`
- `services::test_service_request_response`
- `services::test_service_server_multiple_clients`
- `zero_copy::test_zero_copy_message_info`
- `zero_copy::test_zero_copy_talker_listener`

## RTOS/QEMU Platform E2E

Baremetal/NuttX/FreeRTOS:

- `baremetal_qemu_dds::test_baremetal_dds_rust_talker_to_listener_e2e`
- `nuttx_qemu_dds::test_nuttx_dds_rust_talker_to_listener_e2e`
- `rtos_e2e::test_rtos_action_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_action_e2e::platform_1_Platform__Freertos::lang_2_Lang__C`
- `rtos_e2e::test_rtos_action_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_2_Lang__C`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_service_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_service_e2e::platform_1_Platform__Freertos::lang_2_Lang__C`
- `rtos_e2e::test_rtos_service_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
- `rtos_e2e::test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
- `rtos_e2e::test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp`

ThreadX:

- `rtos_e2e::test_rtos_action_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_action_e2e::platform_3_Platform__ThreadxLinux::lang_2_Lang__C`
- `rtos_e2e::test_rtos_action_e2e::platform_3_Platform__ThreadxLinux::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_2_Lang__C`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_service_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_service_e2e::platform_3_Platform__ThreadxLinux::lang_2_Lang__C`
- `rtos_e2e::test_rtos_service_e2e::platform_3_Platform__ThreadxLinux::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_action_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_action_e2e::platform_4_Platform__ThreadxRiscv64::lang_2_Lang__C`
- `rtos_e2e::test_rtos_action_e2e::platform_4_Platform__ThreadxRiscv64::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_4_Platform__ThreadxRiscv64::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_service_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_service_e2e::platform_4_Platform__ThreadxRiscv64::lang_2_Lang__C`
- `rtos_e2e::test_rtos_service_e2e::platform_4_Platform__ThreadxRiscv64::lang_3_Lang__Cpp`
- `threadx_linux_dds::test_threadx_linux_dds_rust_talker_to_listener_e2e`
- `threadx_riscv64_qemu_dds::test_threadx_rv64_dds_rust_talker_to_listener_e2e`

## Zephyr Runtime/E2E

- `zephyr::test_bidirectional_native_zephyr_e2e`
- `zephyr::test_native_server_zephyr_client`
- `zephyr::test_native_talker_to_zephyr_cpp_listener`
- `zephyr::test_native_to_zephyr_e2e`
- `zephyr::test_zephyr_action_e2e`
- `zephyr::test_zephyr_cpp_action_server_to_client_e2e`
- `zephyr::test_zephyr_cpp_service_server_to_client_e2e`
- `zephyr::test_zephyr_cpp_talker_to_listener_e2e`
- `zephyr::test_zephyr_cpp_talker_to_native_listener`
- `zephyr::test_zephyr_dds_rust_action_a9_e2e`
- `zephyr::test_zephyr_dds_rust_action_client_boots`
- `zephyr::test_zephyr_dds_rust_action_server_boots`
- `zephyr::test_zephyr_dds_rust_async_service_a9_e2e`
- `zephyr::test_zephyr_dds_rust_async_service_client_boots`
- `zephyr::test_zephyr_dds_rust_listener_boots`
- `zephyr::test_zephyr_dds_rust_service_a9_e2e`
- `zephyr::test_zephyr_dds_rust_service_client_boots`
- `zephyr::test_zephyr_dds_rust_service_server_boots`
- `zephyr::test_zephyr_dds_rust_talker_boots`
- `zephyr::test_zephyr_dds_rust_talker_to_listener_a9_e2e`
- `zephyr::test_zephyr_talker_to_listener_e2e`
- `zephyr::test_zephyr_to_native_e2e`
- `zephyr::test_zephyr_xrce_c_talker_listener`
- `zephyr::test_zephyr_xrce_cpp_action_e2e`
- `zephyr::test_zephyr_xrce_cpp_service_e2e`
- `zephyr::test_zephyr_xrce_cpp_talker_listener`
- `zephyr::test_zephyr_xrce_rust_action_e2e`
- `zephyr::test_zephyr_xrce_rust_service_e2e`
- `zephyr::test_zephyr_xrce_rust_talker_listener`

## Other Backend/Platform Groups

ROS 2/RMW interop and discovery:

- `rmw_interop::test_discovery_node_visible`
- `rmw_interop::test_discovery_pub_sub_combined`
- `rmw_interop::test_discovery_service_visible`
- `rmw_interop::test_discovery_subscriber_visible`
- `rmw_interop::test_discovery_topic_visible`
- `ros2_lifecycle_interop::ros2_lifecycle_full_cycle`

Bare-metal Zenoh QEMU:

- `emulator::test_qemu_rtic_action_e2e`
- `emulator::test_qemu_rtic_service_e2e`
- `emulator::test_qemu_serial_pubsub_e2e`

ESP32:

- `esp32_emulator::test_esp32_talker_listener_e2e`
- `esp32_emulator::test_esp32_to_native`
- `esp32_emulator::test_native_to_esp32`

XRCE C and DDS:

- `c_xrce_api::test_c_xrce_listener_starts`
- `c_xrce_api::test_c_xrce_talker_listener_communication`
- `c_xrce_api::test_c_xrce_talker_starts`
- `dds_api::test_dds_action_server_client_e2e`

## Skipped/Ignored

- Nextest reported 11 skipped tests, but the JUnit file did not enumerate them
  as `<skipped>` testcases.
- Harness-reported environment skip: `ThreadX-Linux DDS prerequisites not available`.
- Doctests ignored: 4.
- Miri ignored: `nros-core::clock::tests::test_system_clock_returns_nonzero`.

## Initial Priority

1. Start with the native Zenoh/router behavior cluster. It accounts for 44
   failures and has small repros (`services`, `qos`, `nano2nano`,
   `native_api`) that should reveal whether the shared issue is session
   registration, wake dispatch, key expression encoding, or timing.
2. Re-run one small native pub/sub or service repro before platform-specific
   debugging. The RTOS, Zephyr, ESP32, and interop failures likely include
   downstream symptoms of the native communication failure.
3. After native communication is stable, split remaining platform failures by
   build/runtime/environment: NuttX fast prerequisite failures, FreeRTOS and
   ThreadX runtime timeouts, Zephyr runtime boot/E2E, and ESP32 bridge cases.
