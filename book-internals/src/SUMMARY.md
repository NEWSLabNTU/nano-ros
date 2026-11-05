# Summary

# Orientation

- [Start Here](./orientation/start-here.md)
- [Architecture](./orientation/architecture.md)
- [Repo Map](./orientation/repo-map.md)
- [Contributing](./orientation/contributing.md)
- [Writing the Book](./orientation/writing-the-book.md)

# Development Environment

- [Build Commands](./dev-env/build-commands.md)
- [CLI Lives in the Monorepo (Phase 218)](./dev-env/cli-in-monorepo.md)
- [The CLI's developer tier](./dev-env/cli-dev-tier.md)
- [SDK Tiers for Setup](./dev-env/sdk-tiers.md)
- [nano-ros versioning](./dev-env/versioning.md)
- [CI conventions for nano-ros](./dev-env/ci-conventions.md)
- [ROS 2 on a non-Ubuntu host (Arch, Fedora, NixOS …)](./dev-env/ros2-on-non-ubuntu.md)

# Build Pipeline and Codegen

- [Build System & Caching](./pipeline/build-system.md)
- [Codegen — the pack pipeline](./pipeline/codegen-packs.md)
- [The canonical build path](./pipeline/canonical-build-path.md)
- [Codegen Type Mapping Reference](./pipeline/codegen-type-mapping.md)

# RMW Internals

- [RMW Backends — Host-Language Policy](./rmw/backend-host-language-policy.md)
- [zpico-sys Build Architecture](./rmw/zpico-build.md)
- [RMW Zenoh Protocol](./rmw/zenoh-protocol.md)
- [RMW API vs upstream — every symbol](./rmw/rmw-api-comparison.md)
- [RMW vs upstream: the decision ledger](./rmw/rmw-vs-upstream-ledger.md)
- [The cyclonedds fork: what it carries, and why](./rmw/cyclonedds-fork-delta.md)
- [RMW API: Differences from upstream `rmw.h`](./rmw/rmw-vs-upstream.md)
- [Zenoh-pico Symbol Reference](./rmw/zenoh-pico-symbols.md)
- [XRCE-DDS Symbol Reference](./rmw/xrce-dds-symbols.md)

# Platform Internals

- [Opaque Storage Sizing](./platform/opaque-storage-sizing.md)
- [Per-Platform Contributor Lanes](./platform/platform-lanes.md)
- [Zephyr in-tree examples (contributor)](./platform/zephyr-in-tree.md)
- [NuttX in-tree examples (contributor)](./platform/nuttx-in-tree.md)
- [Platform & RMW implementation notes](./platform/implementation-notes.md)
- [`board.cmake` schema](./platform/board-cmake-schema.md)
- [Zephyr patch upstreaming (Phase 205.B)](./platform/zephyr-upstreaming.md)
- [Zephyr version support](./platform/zephyr-version-support.md)
- [ARM FVP (`FVP_BaseR_AEMv8R`)](./platform/arm-fvp.md)

# Real-Time Internals

- [Running the real-time tests](./realtime/running-rt-tests.md)
- [Dispatch ABI history](./realtime/dispatch-abi-history.md)
- [The memory report](./realtime/mem-report.md)
- [RTIC Integration](./realtime/rtic.md)

# Testing and Verification

- [Creating Examples](./testing/creating-examples.md)
- [Formal Verification](./testing/verification.md)
- [Patched qemu-system-arm](./testing/qemu-patched-binary.md)
- [FreeRTOS LAN9118 Debugging](./testing/freertos-lan9118-debugging.md)
- [Test failures](./testing/test-failures.md)
- [Test Harness](./testing/test-harness.md)
- [QEMU icount: Virtual Clock Synchronization](./testing/qemu-icount.md)
- [Verus Verification Coding Practices](./testing/verus-verification.md)
- [FreeRTOS Development Setup](./testing/freertos-dev-setup.md)
- [ThreadX Development Setup](./testing/threadx-dev-setup.md)
- [Zephyr Development Environment Setup](./testing/zephyr-dev-setup.md)
- [QEMU Bare-Metal Testing](./testing/qemu-bare-metal.md)
- [FreeRTOS LAN9118 Networking Debugging Guide](./testing/lan9118-reference.md)
