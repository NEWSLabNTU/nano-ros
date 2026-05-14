# Phase 124 Test Triage — 2026-05-14

Source run:

- Command: `just ci`
- Nextest run id: `97dac9b0-5a46-4707-8c62-a6a4bdc39806`
- JUnit: `target/nextest/default/junit.xml`
- Logs: `test-logs/latest/`

Static quality gates passed before the runtime failure set:

- `cargo +nightly-2026-04-11 fmt --check`
- workspace clippy, embedded clippy, and feature-combination clippy
- example check matrix
- C, C++, and Python checks
- `zenohd` build
- doctests, Miri, C codegen, and C message generation

Runtime summary:

- Nextest: 815 tests run, 719 passed, 1 flaky, 96 failed, 11 skipped.
- Reported environment skip: `ThreadX-Linux DDS prerequisites not available`.
- Real failures: 95 of 96 total failures, because the ThreadX-Linux DDS case
  is counted as an environment skip by the test harness.
- Doctests: 1 passed, 4 ignored.
- Miri: all selected tests passed; one clock test ignored under Miri.

## Category Summary

| Category | Failures | Notes |
|---|---:|---|
| Native Zenoh/router behavior | 44 | Broad pub/sub, service, QoS, native C/C++ interop, large message, safety, and zero-copy failures. This is the largest cluster and likely shares one or more router/session-level causes. |
| RTOS/QEMU platform E2E | 28 | FreeRTOS, NuttX C/C++, ThreadX Linux, ThreadX RISC-V, and baremetal DDS E2E cases. Most are runtime timeouts or harness-level failures after fixtures build successfully. |
| Zephyr cross-language/XRCE E2E | 7 | Mostly C++ native/Zephyr cross-language and XRCE Zephyr action/pubsub. |
| ROS 2/RMW interop and discovery | 6 | Discovery visibility plus lifecycle full-cycle failures. |
| XRCE backend/runtime E2E | 4 | C XRCE starts/communication plus XRCE large-message publish. |
| Bare-metal Zenoh QEMU E2E | 3 | RTIC action, service, and serial pub/sub. |
| ESP32 emulator E2E | 3 | Native-to-ESP32, ESP32-to-native, and ESP32 talker/listener. |
| DDS backend/runtime E2E | 1 | Native DDS action server/client E2E. |

## Native Zenoh/router Behavior

Executor and callback scheduling:

- `executor::test_executor_multiple_timers_via_publishers`
- `executor::test_mixed_callbacks`

Router/error handling:

- `error_handling::test_debug_logging_overhead`
- `error_handling::test_router_reconnect`
- `error_handling::test_listener_router_disconnect`

Nano-to-nano pub/sub/service/action:

- `nano2nano::test_rtic_pattern_action`
- `nano2nano::test_tls_talker_listener_communication`
- `nano2nano::test_gid_consistency`
- `nano2nano::test_sequence_number_increment`
- `nano2nano::test_rtic_pattern_service`
- `nano2nano::test_rtic_pattern_communication`
- `nano2nano::test_talker_listener_communication`

QoS:

- `qos::test_qos_keyexpr_encoding`
- `qos::test_qos_compatible_settings`
- `qos::test_qos_multiple_subscribers`
- `qos::test_qos_reliable_delivery`

Multi-node:

- `multi_node::test_concurrent_startup`
- `multi_node::test_many_to_many`
- `multi_node::test_multiple_subscribers_single_topic`
- `multi_node::test_publisher_scalability`
- `multi_node::test_subscriber_scalability`
- `multi_node::test_multiple_publishers_single_topic`
- `multi_node::test_sustained_communication`

Large message:

- `large_msg::test_zenoh_e2e_integrity`
- `large_msg::test_zenoh_e2e_large_receive`
- `large_msg::test_zenoh_throughput_100hz`
- `large_msg::test_zenoh_throughput_burst`

Native C/C++ API interop:

- `native_api::test_c_rust_pubsub_interop`
- `native_api::test_cpp_action_communication`
- `native_api::test_cpp_action_goal_rejection`
- `native_api::test_cpp_rust_pubsub_interop`
- `native_api::test_cpp_rust_service_interop`
- `native_api::test_native_service_communication::lang_1_Language__C`
- `native_api::test_native_service_communication::lang_2_Language__Cpp`
- `native_api::test_native_talker_listener_communication::lang_1_Language__C`
- `native_api::test_native_talker_listener_communication::lang_2_Language__Cpp`

Services, params, safety, and zero-copy:

- `services::test_service_multiple_sequential_calls`
- `services::test_service_request_response`
- `services::test_service_server_multiple_clients`
- `params::test_param_integer_type`
- `safety_e2e::test_safety_e2e_talker_listener`
- `safety_e2e::test_safety_talker_standard_listener`
- `zero_copy::test_zero_copy_message_info`
- `zero_copy::test_zero_copy_talker_listener`

## RTOS/QEMU Platform E2E

Baremetal DDS:

- `baremetal_qemu_dds::test_baremetal_dds_rust_talker_to_listener_e2e`

NuttX C/C++:

- `rtos_e2e::test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
- `rtos_e2e::test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
- `rtos_e2e::test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp`

FreeRTOS:

- `rtos_e2e::test_rtos_action_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_action_e2e::platform_1_Platform__Freertos::lang_2_Lang__C`
- `rtos_e2e::test_rtos_action_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_2_Lang__C`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_service_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_service_e2e::platform_1_Platform__Freertos::lang_2_Lang__C`
- `rtos_e2e::test_rtos_service_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp`

ThreadX Linux:

- `rtos_e2e::test_rtos_action_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_service_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_action_e2e::platform_3_Platform__ThreadxLinux::lang_2_Lang__C`
- `rtos_e2e::test_rtos_action_e2e::platform_3_Platform__ThreadxLinux::lang_3_Lang__Cpp`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_2_Lang__C`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_3_Lang__Cpp`
- `threadx_linux_dds::test_threadx_linux_dds_rust_talker_to_listener_e2e`

ThreadX RISC-V:

- `rtos_e2e::test_rtos_action_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_pubsub_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust`
- `rtos_e2e::test_rtos_service_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust`
- `threadx_riscv64_qemu_dds::test_threadx_rv64_dds_rust_talker_to_listener_e2e`

## Other Backend/Platform Groups

DDS:

- `dds_api::test_dds_action_server_client_e2e`

XRCE:

- `c_xrce_api::test_c_xrce_listener_starts`
- `c_xrce_api::test_c_xrce_talker_listener_communication`
- `c_xrce_api::test_c_xrce_talker_starts`
- `xrce::test_xrce_large_message_publish`

Zephyr:

- `zephyr::test_native_talker_to_zephyr_cpp_listener`
- `zephyr::test_zephyr_cpp_service_server_to_client_e2e`
- `zephyr::test_zephyr_cpp_action_server_to_client_e2e`
- `zephyr::test_zephyr_cpp_talker_to_listener_e2e`
- `zephyr::test_zephyr_cpp_talker_to_native_listener`
- `zephyr::test_zephyr_xrce_c_talker_listener`
- `zephyr::test_zephyr_xrce_cpp_action_e2e`

ESP32:

- `esp32_emulator::test_esp32_talker_listener_e2e`
- `esp32_emulator::test_esp32_to_native`
- `esp32_emulator::test_native_to_esp32`

Bare-metal Zenoh QEMU:

- `emulator::test_qemu_rtic_action_e2e`
- `emulator::test_qemu_rtic_service_e2e`
- `emulator::test_qemu_serial_pubsub_e2e`

ROS 2/RMW interop:

- `rmw_interop::test_discovery_node_visible`
- `rmw_interop::test_discovery_pub_sub_combined`
- `rmw_interop::test_discovery_service_visible`
- `rmw_interop::test_discovery_subscriber_visible`
- `rmw_interop::test_discovery_topic_visible`
- `ros2_lifecycle_interop::ros2_lifecycle_full_cycle`

## Skipped/Ignored

Nextest reported 11 skipped tests, but the JUnit file does not enumerate them
as `<skipped>` testcases. The harness did print one environment skip category:

- `ThreadX-Linux DDS prerequisites not available`

Additional ignored tests outside nextest:

- Doctests ignored: 4.
- Miri ignored: `nros-core::clock::tests::test_system_clock_returns_nonzero`.

## Initial Priority

1. Start with the native Zenoh/router behavior cluster. It accounts for nearly
   half of the real failures and includes fast repros (`services`,
   `nano2nano`, `qos`, `executor`) that should reveal whether the current
   issue is session registration, wake dispatch, key expression encoding, or
   runtime timing.
2. Re-run one small native pub/sub or service repro before platform-specific
   debugging. The RTOS, Zephyr, ESP32, and interop failures likely include
   downstream symptoms of the same native communication breakage.
3. After native communication is stable, split remaining platform failures by
   build/runtime/environment: NuttX C/C++ fast failures, FreeRTOS runtime
   timeouts, ThreadX Linux/RISC-V DDS gaps, and Zephyr C++/XRCE regressions.
