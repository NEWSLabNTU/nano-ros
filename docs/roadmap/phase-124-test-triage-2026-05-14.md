# Phase 124 Test Triage - 2026-05-14

## Update - 2026-05-15

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

- `nano2nano::test_rtic_pattern_action`
- `nano2nano::test_tls_talker_listener_communication`
- `nano2nano::test_gid_consistency`
- `nano2nano::test_sequence_number_increment`
- `nano2nano::test_rtic_pattern_service`
- `nano2nano::test_rtic_pattern_communication`
- `large_msg::test_zenoh_e2e_integrity`
- `large_msg::test_zenoh_e2e_large_receive`
- `large_msg::test_zenoh_throughput_100hz`
- `large_msg::test_zenoh_throughput_burst`
- `native_api::test_cpp_action_communication`
- `native_api::test_cpp_action_goal_rejection`
- `native_api::test_native_service_communication::lang_1_Language__C`
- `native_api::test_native_service_communication::lang_2_Language__Cpp`
- `native_api::test_native_talker_listener_communication::lang_1_Language__C`
- `native_api::test_native_talker_listener_communication::lang_2_Language__Cpp`
- `safety_e2e::test_safety_e2e_talker_listener`
- `zero_copy::test_zero_copy_message_info`

Next priority:

1. Fix the remaining native Zenoh identity/metadata cluster first:
   `gid_consistency`, `sequence_number_increment`, and
   `zero_copy_message_info`.
2. Then rerun the native C/C++ service and talker/listener tests. These are
   likely downstream of the same attachment/message-info path.
3. Defer platform E2E buckets until the remaining native metadata behavior is
   stable.

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
