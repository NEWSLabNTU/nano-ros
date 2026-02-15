# Reference Implementations

Low-level reference implementations for BSP developers. These are libraries (not standalone examples) used by the board support crates.

**Most users should use the examples instead:** see [examples/](../../examples/README.md).

## Contents

| Directory | Description |
|-----------|-------------|
| `qemu-smoltcp-bridge` | Shared library that bridges smoltcp sockets to zenoh-pico. Provides the C FFI required by zenoh-pico's smoltcp platform backend. |
| `stm32f4-porting` | STM32F4 porting references (polling loop + RTIC) using internal platform crates. Templates for BSP developers. |

## See Also

- [examples/qemu-arm/rust/standalone/lan9118/](../../examples/qemu-arm/rust/standalone/lan9118/) - LAN9118 driver validation
- [examples/stm32f4/rust/standalone/smoltcp/](../../examples/stm32f4/rust/standalone/smoltcp/) - smoltcp TCP echo server
- [examples/stm32f4/rust/core/embassy/](../../examples/stm32f4/rust/core/embassy/) - Embassy async
