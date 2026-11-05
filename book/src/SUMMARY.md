# Summary

[Introduction](./start/introduction.md)
[Choose Your Entry](./start/choose-by-purpose.md)

# Get Started

- [Install](./get-started/installation.md)
- [Installation Details](./get-started/installation-details.md)
- [First Project](./get-started/first-project.md)
- [Anatomy of What You Just Built](./get-started/anatomy.md)
- [First Node — Rust](./get-started/first-node-rust.md)
- [First Node — C](./get-started/first-node-c.md)
- [First Node — C++](./get-started/first-node-cpp.md)
- [Your own message package](./get-started/your-own-msg-package.md)
- [Troubleshooting — First 10 Minutes](./get-started/first-10-minutes.md)

# Build a Project

- [Building and Configuring](./build/build-and-configure.md)
- [Project layout](./build/growing-your-workspace.md)
- [C / C++ multi-node workspaces](./build/cpp-workspaces.md)
- [Mixed-language workspaces](./build/mixed-language.md)
- [Role reference](./build/roles.md)
- [Message Generation](./build/message-generation.md)
- [Configuration](./build/configuration.md)
- [Build Profiles](./build/build-profiles.md)
- [What `nros build` Produces](./build/build-artifacts.md)
- [Integrating into a Vendored Tree](./build/vendored-tree.md)
- [Build as a CMake subdirectory](./build/cmake-subdirectory.md)

# Run on a Target

- [The Target Model](./targets/target-model.md)
- [Running an Image: `nros run`](./targets/run.md)
- [Flashing and Monitoring a Board](./targets/flash-and-monitor.md)
- [Your Bench: `nros bench`](./targets/bench.md)
- [Run Plans](./targets/run-plans.md)
- [How Integration Works](./targets/how-integration-works.md)
- [Native host build (reference)](./targets/linux-host.md)
- [FreeRTOS (QEMU)](./targets/freertos.md)
- [Zephyr (west module)](./targets/zephyr.md)
- [A Zephyr App You Build with West](./targets/zephyr-west-app.md)
- [Nucleo-F767ZI (Zephyr, Renode twin)](./targets/nucleo-f767zi.md)
- [NuttX (apps/external)](./targets/nuttx.md)
- [ThreadX](./targets/threadx.md)
- [Bare-metal Cortex-M3](./targets/bare-metal.md)
- [PX4 Autopilot](./targets/px4.md)
- [ARM FVP (Cortex-A SMP)](./targets/arm-fvp.md)

# Communicate

- [Choosing an RMW Backend](./communicate/choosing-an-rmw.md)
- [Switching RMW in Config](./communicate/switching-rmw.md)
- [QoS, Status Events, and Discovery](./communicate/qos-status-events.md)
- [ROS 2 Interoperability](./communicate/ros2-interop.md)
- [Cross-backend Bridges](./communicate/bridges.md)
- [Serial Transport](./communicate/serial-transport.md)
- [TX Throughput & Latency Tuning](./communicate/tx-tuning.md)

# Real-Time and Safety

- [RTOS Cooperation](./realtime/rtos-cooperation.md)
- [Scheduling Models](./realtime/scheduling-models.md)
- [The Real-Time Execution Model](./realtime/execution-model.md)
- [Dispatch Strategy](./realtime/dispatch-strategy.md)
- [Real-Time Analysis](./realtime/rt-analysis.md)
- [Measuring Static Memory](./realtime/static-memory.md)
- [Safety Protocol](./realtime/safety-protocol.md)
- [Production Readiness Checklist](./realtime/production-readiness.md)

# Operate and Debug

- [Logging](./operate/logging.md)
- [Simulated Time](./operate/simulated-time.md)
- [Deployment Workflow](./operate/deployment.md)
- [Observing a Board With No Console](./operate/console-less-boards.md)
- [Profiling Your Build](./operate/build-profiling.md)
- [Troubleshooting](./operate/troubleshooting.md)

# Coming from ROS 2

- [Setup Compared to Standard ROS 2](./from-ros2/setup-compared.md)
- [Porting a ROS 2 C++ node](./from-ros2/porting-a-cpp-node.md)
- [Differences from Standard ROS 2](./from-ros2/differences.md)
- [nano-ros vs micro-ROS](./from-ros2/vs-micro-ros.md)
- [Migration Guide](./from-ros2/migration-guide.md)

# Concepts and Rationale

- [Architecture Overview](./concepts/architecture.md)
- [Execution Model and Two-Layer API](./concepts/two-layer-api.md)
- [Platform Model](./concepts/platform-model.md)
- [`no_std`, `alloc`, and `std`](./concepts/no-std.md)
- [Overview](./concepts/design-overview.md)
- [Client Library Model](./concepts/client-library.md)

# Extending nano-ros

- [Overview](./extending/overview.md)
- [The `Board` Trait Family](./extending/board-trait.md)
- [Custom Board Package](./extending/custom-board.md)
- [Adding a FreeRTOS Board](./extending/freertos-board.md)
- [Vendor Overlay Board Crate](./extending/vendor-overlay.md)
- [Worked Example — STM32F4 Out of Tree](./extending/stm32f4-out-of-tree.md)
- [Importing a Board Crate](./extending/board-crate-import.md)
- [Board Integration](./extending/board-integration.md)
- [Adding a Board to the Registry](./extending/adding-a-board-to-the-registry.md)
- [Custom Platform](./extending/custom-platform.md)
- [Adding a Platform (CMake)](./extending/add-a-platform.md)
- [Canonical Platform C ABI](./extending/platform-c-abi.md)
- [Platform API Design](./extending/platform-api-design.md)
- [Custom Transport](./extending/custom-transport.md)
- [Custom RMW Backend](./extending/custom-rmw.md)
- [RMW API Design](./extending/rmw-design.md)
- [RMW API: Differences from upstream rmw.h](./extending/rmw-vs-upstream.md)
- [Zenoh-pico Symbol Reference](./extending/zenoh-pico-symbols.md)
- [XRCE-DDS Symbol Reference](./extending/xrce-dds-symbols.md)
- [Platform Porting Pitfalls](./extending/porting-pitfalls.md)
- [Runner packages](./extending/runner-packages.md)
- [Command packages](./extending/command-packages.md)

# Reference

- [`nros` CLI](./reference/cli.md)
- [Rust API](./reference/rust-api.md)
- [C API](./reference/c-api.md)
- [C++ API](./reference/cpp-api.md)
- [RMW API](./reference/rmw-api.md)
- [Platform API](./reference/platform-api.md)
- [Platform Differences](./reference/platform-differences.md)
- [Support Status](./reference/support-status.md)
- [Per-RMW Feature Matrix](./reference/rmw-feature-matrix.md)
- [Scheduling Wiring Matrix](./reference/sched-matrix.md)
- [Supported Boards](./reference/supported-boards.md)
- [Environment Variables](./reference/environment-variables.md)
- [`nros-bridge.toml` Schema](./reference/nros-bridge-toml.md)
- [Static Pool Inventory](./reference/static-pool-inventory.md)
- [Configuration Surface](./reference/configuration-surface.md)
<!-- BEGIN GENERATED by scripts/gen-board-support.py: board pages -->
- [Board Support Levels](./reference/board-support-levels.md)
  - [nros-board-linux (Linux)](./reference/boards/linux.md)
  - [nros-board-zephyr (ZephyrNativeSim)](./reference/boards/zephyr-native-sim.md)
  - [nros-board-mps3-an536-freertos](./reference/boards/mps3-an536-freertos.md)
  - [nros-board-mps2-an385-freertos (FreertosMps2)](./reference/boards/mps2-an385-freertos.md)
  - [nros-board-s32z270-freertos](./reference/boards/s32z270-freertos.md)
  - [nros-board-nuttx-qemu (NuttxArm)](./reference/boards/nuttx-arm.md)
  - [nros-board-threadx-linux (ThreadxLinux)](./reference/boards/threadx-linux.md)
  - [nros-board-freertos-posix (FreertosPosix)](./reference/boards/freertos-posix.md)
  - [nros-board-threadx-qemu-riscv64 (ThreadxRiscv64)](./reference/boards/threadx-qemu-riscv64.md)
  - [nros-board-mps2-an385 (QemuBaremetal)](./reference/boards/mps2-an385.md)
  - [nros-board-mps2-an385-zephyr (ZephyrQemuCortexM)](./reference/boards/mps2-an385-zephyr.md)
  - [nros-board-nuttx-qemu (NuttxRiscv)](./reference/boards/nuttx-riscv.md)
  - [nros-board-zephyr (Fvp)](./reference/boards/fvp.md)
  - [s32k344-safety-island](./reference/boards/s32k344-safety-island.md)
  - [nros-board-nucleo-f767zi-zephyr (ZephyrNucleoF767zi)](./reference/boards/nucleo-f767zi-zephyr.md)
  - [nros-board-rpi-pico-w-zephyr](./reference/boards/rpi-pico-w-zephyr.md)
<!-- END GENERATED board pages -->

# Migrating from Older Releases

- [Retired names](./migrating/retired-names.md)
- [Migrating off `install-local`](./migrating/install-local-removal.md)
- [ESP32 (esp-hal, retired)](./migrating/esp32.md)
- [Retired Configuration Files](./migrating/retired-config-files.md)
