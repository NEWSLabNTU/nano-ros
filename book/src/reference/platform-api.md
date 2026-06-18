# Platform API

The platform API is the porting boundary between nano-ros and a
concrete OS / RTOS / bare-metal target. Each platform provides a
clock, optionally a heap, optionally threading, optionally
networking. Platform is **internal** — user applications use the
[Rust](rust-api.md) / [C](c-api.md) / [C++](cpp-api.md) APIs, not
the platform vtable directly.

## Canonical reference

The C vtable in
`packages/core/nros-platform-cffi/include/nros/platform_vtable.h`
is the source of truth. Every function pointer's brief, parameter
docs, ownership rules, blocking / non-blocking classification, and
ISR-safe contract live in the Doxygen output.

| Surface | Link |
|---|---|
| **platform-cffi Doxygen** (canonical) | [HTML](../api/platform-cffi/index.html) · [header](https://github.com/NEWSLabNTU/nano-ros/blob/main/packages/core/nros-platform-cffi/include/nros/platform_vtable.h) |

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
| `nros-platform-posix` | Linux / *BSD | [packages/core/nros-platform-posix](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-posix) |
| `nros-platform-nuttx` | NuttX RTOS | [packages/core/nros-platform-nuttx](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-nuttx) |
| `nros-platform-freertos` | FreeRTOS | [packages/core/nros-platform-freertos](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-freertos) |
| `nros-platform-threadx` | Azure RTOS / ThreadX | [packages/core/nros-platform-threadx](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-threadx) |
| `nros-platform-zephyr` | Zephyr RTOS | [packages/core/nros-platform-zephyr](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-zephyr) |
| `nros-platform-mps2-an385` | Cortex-M3 (QEMU) | [packages/platforms/nros-platform-mps2-an385](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platforms/nros-platform-mps2-an385) |
| `nros-platform-stm32f4` | STM32F4 | [packages/platforms/nros-platform-stm32f4](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platforms/nros-platform-stm32f4) |
| `nros-platform-esp32-qemu` | ESP32-C3 (QEMU) | [packages/platforms/nros-platform-esp32-qemu](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platforms/nros-platform-esp32-qemu) |

The POSIX implementation is the canonical reference port.

## Writing a custom platform

- Conceptual walkthrough: [Custom Platform](../porting/custom-platform.md).
- Per-platform behaviour matrix:
  [Platform Differences](./platform-differences.md).
