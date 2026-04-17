# Summary

[Introduction](./introduction.md)

# Getting Started

- [Installation](./getting-started/installation.md)
- [Native (Linux / macOS)](./getting-started/native.md)
- [Zephyr](./getting-started/zephyr.md)
- [FreeRTOS (QEMU)](./getting-started/freertos.md)
- [NuttX (QEMU)](./getting-started/nuttx.md)
- [ThreadX](./getting-started/threadx.md)
- [Bare-metal (QEMU ARM)](./getting-started/bare-metal.md)
- [ESP32](./getting-started/esp32.md)
- [ROS 2 Interoperability](./getting-started/ros2-interop.md)

# User Guide

- [Choosing an RMW Backend](./user-guide/rmw-backends.md)
- [Configuration](./user-guide/configuration.md)
- [Message Generation](./user-guide/message-generation.md)
- [Serial Transport](./user-guide/serial-transport.md)
- [Troubleshooting](./user-guide/troubleshooting.md)

# Reference

- [Rust API](./reference/rust-api.md)
- [C API](./reference/c-api.md)
- [C++ API](./reference/cpp-api.md)
- [Platform API](./reference/platform-api.md)
- [Environment Variables](./reference/environment-variables.md)
- [Build Commands](./reference/build-commands.md)

# Concepts

- [Architecture Overview](./concepts/architecture.md)
- [no_std Support](./concepts/no-std.md)
- [Platform Model](./concepts/platform-model.md)

# Internals

- [RMW API Design](./internals/rmw-api-design.md)
- [RMW API Reference](./internals/rmw-api.md)
- [RMW Zenoh Protocol](./internals/rmw-zenoh-protocol.md)
- [Scheduling Models](./internals/scheduling-models.md)
- [Formal Verification](./internals/verification.md)
- [Real-Time Analysis](./internals/realtime-analysis.md)
- [Safety Protocol](./internals/safety.md)
- [Porting to a New Platform](./internals/porting-platform/README.md)
  - [Implementing a Platform](./internals/porting-platform/implementing-a-platform.md)
  - [Zenoh-pico Symbol Reference](./internals/porting-platform/zenoh-pico.md)
  - [XRCE-DDS Symbol Reference](./internals/porting-platform/xrce-dds.md)
- [Adding an RMW Backend](./internals/adding-rmw-backend.md)
- [Board Crate Implementation](./internals/board-crate.md)
- [Platform Customization](./internals/platform-customization.md)
- [Creating Examples](./internals/creating-examples.md)
- [Platform Porting Pitfalls](./internals/platform-porting-pitfalls.md)
- [Contributing](./internals/contributing.md)
