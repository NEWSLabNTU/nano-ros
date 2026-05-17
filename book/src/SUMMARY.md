# Summary

[Introduction](./introduction.md)

# User Guide

- [Application Workflow](./user-guide/workflow.md)
- [Setup Compared to Standard ROS 2](./start-here/setup-compared-to-ros2.md)
- [Installation](./getting-started/installation.md)
- [Build as a CMake subdirectory](./getting-started/build-as-subdirectory.md)
- [Integration: Zephyr (`west` module)](./getting-started/integration-zephyr.md)
- [Integration: ESP-IDF component](./getting-started/integration-esp-idf.md)
- [Integration: PlatformIO library](./getting-started/integration-platformio.md)
- [Integration: NuttX external app](./getting-started/integration-nuttx.md)
- [Integration: PX4 external module](./getting-started/integration-px4.md)
- [Package Preparation](./user-guide/package-preparation.md)
- [First Native Rust Node](./getting-started/native.md)
- [Message Generation](./user-guide/message-generation.md)
- [Configuration](./user-guide/configuration.md)
- [Deployment Workflow](./user-guide/deployment.md)
- [ROS 2 Interoperability](./getting-started/ros2-interop.md)
- [Choosing an RMW Backend](./user-guide/rmw-backends.md)
- [QoS, Status Events, and Discovery](./concepts/status-events.md)
- [Serial Transport](./user-guide/serial-transport.md)
- [Troubleshooting](./user-guide/troubleshooting.md)

# ROS 2 Orientation

- [Differences from Standard ROS 2](./concepts/ros2-comparison.md)
- [Migration Guide](./start-here/migration-guide.md)

# Platform Guides

- [Native POSIX](./platform-guides/native-posix.md)
- [Zephyr](./getting-started/zephyr.md)
- [FreeRTOS (QEMU)](./getting-started/freertos.md)
- [NuttX (QEMU)](./getting-started/nuttx.md)
- [ThreadX](./getting-started/threadx.md)
- [Bare-metal (QEMU ARM)](./getting-started/bare-metal.md)
- [ESP32](./getting-started/esp32.md)
- [PX4 Autopilot](./getting-started/px4.md)

# Concepts

- [Architecture Overview](./concepts/architecture.md)
- [Execution Model and Two-Layer API](./concepts/two-layer-api.md)
- [Platform Model](./concepts/platform-model.md)
- [`no_std`, `alloc`, and `std`](./concepts/no-std.md)
- [RTOS Cooperation](./concepts/rtos-cooperation.md)

# Porting Guide

- [Overview](./porting/overview.md)
- [Custom Board Package](./porting/custom-board.md)
- [Custom Platform](./porting/custom-platform.md)
- [Adding a Platform (CMake)](./porting/add-a-platform.md)
- [Custom Transport](./porting/custom-transport.md)
- [Custom RMW Backend](./porting/custom-rmw.md)
- [Platform Porting Pitfalls](./internals/platform-porting-pitfalls.md)

# Design Rationale

- [Overview](./design/overview.md)
- [Client Library Model](./design/client-library.md)
- [RMW API Design](./design/rmw.md)
- [RMW API: Differences from upstream rmw.h](./design/rmw-vs-upstream.md)
- [Platform API Design](./design/platform-api.md)

# Internals

- [Canonical Platform C ABI](./internals/platform-c-abi.md)
- [RMW Backends — Host-Language Policy](./internals/rmw-backends.md)
- [RMW Zenoh Protocol](./internals/rmw-zenoh-protocol.md)
- [FreeRTOS LAN9118 Debugging](./internals/freertos-lan9118-debugging.md)
- [Patched qemu-system-arm](./internals/qemu-patched-binary.md)
- [Opaque Storage Sizing](./internals/opaque-storage-sizing.md)
- [Scheduling Models](./internals/scheduling-models.md)
- [Real-Time Analysis](./internals/realtime-analysis.md)
- [Formal Verification](./internals/verification.md)
- [Safety Protocol](./internals/safety.md)
- [Zenoh-pico Symbol Reference](./internals/porting-platform/zenoh-pico.md)
- [XRCE-DDS Symbol Reference](./internals/porting-platform/xrce-dds.md)
- [Creating Examples](./internals/creating-examples.md)
- [Contributing](./internals/contributing.md)

# Reference

- [`nros` CLI](./reference/cli.md)
- [Rust API](./reference/rust-api.md)
- [C API](./reference/c-api.md)
- [C++ API](./reference/cpp-api.md)
- [RMW API](./reference/rmw-api.md)
- [Platform API](./reference/platform-api.md)
- [Platform Differences](./reference/platform-differences.md)
- [Environment Variables](./reference/environment-variables.md)
- [Build Commands](./reference/build-commands.md)
- [`nros.toml` Schema](./reference/nros-toml.md)
