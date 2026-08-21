# Platform API

The platform API is the porting boundary between nano-ros and a
concrete OS / RTOS / bare-metal target. Each platform provides a
clock, optionally a heap, optionally threading, optionally
networking. Platform is **internal** — user applications use the
[Rust](rust-api.md) / [C](c-api.md) / [C++](cpp-api.md) APIs, not
the platform vtable directly.

## Canonical reference

The SSoT C headers in
`packages/platform/nros-platform-api/include/nros/` (`platform.h` +
`platform_net.h`, `platform_timer.h`, `platform_zephyr.h`; RFC-0054 —
Rust consumes committed bindgen output from them) are the source of
truth. Every symbol's brief, parameter docs, ownership rules,
blocking / non-blocking classification, and ISR-safe contract live in
the Doxygen compiled from those headers.

| Surface | Link |
|---|---|
| **platform ABI Doxygen** (canonical) | [HTML](../api/platform-cffi/index.html) · [headers](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-api/include/nros) |

To regenerate locally:

```bash
just doc-platform-cffi   # produces target/doxygen/platform-cffi/
```

This page does **not** duplicate the interface specification — read
the Doxygen for that.

## Reference implementations

Each row is a complete worked example. The crate's `README.md`
walks the implementation; the source is the worked solution to
copy.

| Crate | Target | Source |
|---|---|---|
| `nros-platform-posix` | Linux / *BSD | [packages/platform/nros-platform-posix](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-posix) |
| `nros-platform-nuttx` | NuttX RTOS | [packages/platform/nros-platform-nuttx](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-nuttx) |
| `nros-platform-freertos` | FreeRTOS | [packages/platform/nros-platform-freertos](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-freertos) |
| `nros-platform-threadx` | Azure RTOS / ThreadX | [packages/platform/nros-platform-threadx](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-threadx) |
| `nros-platform-zephyr` | Zephyr RTOS | [packages/platform/nros-platform-zephyr](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-zephyr) |
| `nros-platform-mps2-an385` | Cortex-M3 (QEMU) | [packages/platform/nros-platform-mps2-an385](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-mps2-an385) |
| `nros-platform-stm32f4` | STM32F4 | [packages/platform/nros-platform-stm32f4](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-stm32f4) |
| `nros-platform-esp32-qemu` | ESP32-C3 (QEMU) | [packages/platform/nros-platform-esp32-qemu](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-esp32-qemu) |

The POSIX implementation is the canonical reference port.

## Writing a custom platform

- Conceptual walkthrough: [Custom Platform](../porting/custom-platform.md).
- Per-platform behaviour matrix:
  [Platform Differences](./platform-differences.md).
