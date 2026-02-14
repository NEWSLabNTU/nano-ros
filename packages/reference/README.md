# Platform Integration Examples

These examples are **low-level reference implementations** for developers creating new Board Support Packages (BSPs) or integrating nros with new hardware platforms.

**Most users should use the platform-crate-based examples instead:**
- [qemu/](../qemu/) - QEMU bare-metal examples
- [stm32f4/](../stm32f4/) - STM32F4 examples
- [zephyr/](../zephyr/) - Zephyr RTOS examples

## Contents

### QEMU Platform

| Directory | Description |
|-----------|-------------|
| `qemu-smoltcp-bridge` | Shared library that bridges smoltcp sockets to zenoh-pico. Provides the C FFI required by zenoh-pico's smoltcp platform backend. |
| `qemu-lan9118` | Standalone LAN9118 Ethernet driver validation. Tests the driver without zenoh. |

### STM32F4 Platform

| Directory | Description |
|-----------|-------------|
| `stm32f4-smoltcp` | TCP echo server using RTIC + smoltcp. Validates Ethernet and networking without zenoh. |
| `stm32f4-rtic` | Full nros integration with RTIC framework. Shows task priorities and interrupt handling. |
| `stm32f4-polling` | Simple polling loop without RTIC. Minimal dependencies, suitable for bare-metal. |
| `stm32f4-embassy` | Embassy async framework integration. Shows cooperative multitasking approach. |

### Embedded C++

| Directory | Description |
|-----------|-------------|
| `embedded-cpp-talker` | C++ talker template for embedded systems |
| `embedded-cpp-listener` | C++ listener template for embedded systems |

## When to Use These

Use these examples when:
- Developing a new BSP crate for a new platform
- Debugging low-level networking issues
- Understanding how nros integrates with smoltcp/zenoh-pico
- Porting to a new MCU family

## Architecture

```
User Application
       │
       ▼
  nros Platform (e.g., nano-ros-platform-stm32f4)
       │
       ▼
┌──────┴──────┐
│   smoltcp   │  ← These examples show this layer
│  TCP/IP     │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ Ethernet    │  ← Platform-specific driver
│ Driver      │
└─────────────┘
```

## Building

These examples require cross-compilation for ARM Cortex-M:

```bash
# Install target
rustup target add thumbv7em-none-eabihf

# Build example
cd stm32f4-polling
cargo build --release

# Flash (requires probe-rs)
cargo run --release
```

## See Also

- [packages/platform/nano-ros-platform-qemu/](../../packages/platform/nano-ros-platform-qemu/) - QEMU platform crate source
- [packages/platform/nano-ros-platform-stm32f4/](../../packages/platform/nano-ros-platform-stm32f4/) - STM32F4 platform crate source
