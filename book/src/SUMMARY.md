# Summary

[Introduction](./introduction.md)
[Choose Your Entry](./start-here/choose-your-entry.md)

# Getting Started (Linux first)

- [Setup Compared to Standard ROS 2](./start-here/setup-compared-to-ros2.md)
- [Install + first build](./getting-started/installation.md)
- [First Node — Rust](./getting-started/first-node-rust.md)
- [First Node — C](./getting-started/first-node-c.md)
- [First Node — C++](./getting-started/first-node-cpp.md)
- [Porting a ROS 2 C++ node](./getting-started/porting-a-cpp-node.md)
- [Your own message package](./getting-started/your-own-msg-package.md)
- [Troubleshooting — First 10 Minutes](./getting-started/troubleshooting-first-10-min.md)

# Multi-Node Projects

- [Project layout](./getting-started/workspace-from-app-node.md)
- [Node packages](./getting-started/workspace-node-pkgs.md)
- [Bringup packages](./getting-started/workspace-bringup.md)
- [Entry packages](./getting-started/workspace-entry-pkg.md)
- [C / C++ multi-node workspaces](./getting-started/workspace-cpp.md)
- [Mixed-language workspaces](./getting-started/workspace-mixed-language.md)
- [Role reference](./user-guide/component-and-entry-pkg.md)

# Embedded Starters

- [FreeRTOS (QEMU)](./getting-started/freertos.md)
- [Zephyr (west module)](./getting-started/integration-zephyr.md)
- [NuttX (apps/external)](./getting-started/integration-nuttx.md)
- [ThreadX](./getting-started/threadx.md)
- [ESP32 (esp-hal)](./getting-started/esp32.md)
- [ESP32 (ESP-IDF component)](./getting-started/integration-esp-idf.md)
- [Bare-metal Cortex-M3](./getting-started/bare-metal.md)
- [PX4 Autopilot](./getting-started/px4.md)
- [ARM FVP (Cortex-A SMP)](./getting-started/arm-fvp.md)
- [Native POSIX (reference)](./platform-guides/native-posix.md)

# User Guide

- [Application Workflow](./user-guide/workflow.md)
- [Build as a CMake subdirectory](./getting-started/build-as-subdirectory.md)
- [Message Generation](./user-guide/message-generation.md)
- [Configuration](./user-guide/configuration.md)
- [Logging](./user-guide/logging.md)
- [Profiling Your Build](./user-guide/build-profiling.md)
- [Deployment Workflow](./user-guide/deployment.md)
- [ROS 2 Interoperability](./getting-started/ros2-interop.md)
- [Choosing an RMW Backend](./user-guide/rmw-backends.md)
- [Cross-backend Bridges](./user-guide/cross-backend-bridges.md)
- [QoS, Status Events, and Discovery](./concepts/status-events.md)
- [Serial Transport](./user-guide/serial-transport.md)
- [RTIC Integration](./user-guide/rtic-integration.md)
- [Embassy Integration](./user-guide/embassy-integration.md)
- [Troubleshooting](./user-guide/troubleshooting.md)

# ROS 2 Orientation

- [Differences from Standard ROS 2](./concepts/ros2-comparison.md)
- [nano-ros vs micro-ROS](./concepts/comparison-vs-microros.md)
- [Migration Guide](./start-here/migration-guide.md)

# Concepts

- [Architecture Overview](./concepts/architecture.md)
- [Execution Model and Two-Layer API](./concepts/two-layer-api.md)
- [Platform Model](./concepts/platform-model.md)
- [Board Integration](./concepts/board-integration.md)
- [`no_std`, `alloc`, and `std`](./concepts/no-std.md)
- [RTOS Cooperation](./concepts/rtos-cooperation.md)

# Porting Guide

- [Overview](./porting/overview.md)
- [The `Board` Trait Family](./porting/board-trait.md)
- [Custom Board Package](./porting/custom-board.md)
- [Vendor Overlay Board Crate](./porting/vendor-overlay.md)
- [Importing a Board Crate](./porting/board-crate-import.md)
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
- [Dispatch Strategy](./internals/dispatch-strategy.md)
- [Real-Time Analysis](./internals/realtime-analysis.md)
- [Build System & Caching](./internals/build-system.md)
- [CLI Lives in the Monorepo (Phase 218)](./internals/cli-in-monorepo.md)
- [zpico-sys Build Architecture](./internals/zpico-build.md)
- [Formal Verification](./internals/verification.md)
- [Safety Protocol](./internals/safety.md)
- [Production Readiness Checklist](./internals/production-readiness.md)
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
- [Supported Boards](./reference/supported-boards.md)
- [Environment Variables](./reference/environment-variables.md)
- [Build Commands](./reference/build-commands.md)
- [`nros-bridge.toml` Schema](./reference/nros-bridge-toml.md)

# Release Notes

- [Migrating off `install-local`](./release/migration-install-local-removal.md)
