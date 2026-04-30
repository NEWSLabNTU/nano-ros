# nros-platform-esp32

Bare-metal platform for **ESP32** (Xtensa LX6 dual-core, real hardware).
Pairs with the [`nros-board-esp32`](../../boards/nros-board-esp32) board
crate. For QEMU smoke tests on RISC-V ESP32-C3 see the sibling
[`nros-platform-esp32-qemu`](../nros-platform-esp32-qemu) crate.

## Role

Implements the trait family in
[`nros-platform-api`](../../core/nros-platform-api) for ESP32 against
the [`esp-hal`](https://github.com/esp-rs/esp-hal) HAL: `esp_timer_get_time`
for the monotonic clock, [`esp-alloc`](https://github.com/esp-rs/esp-alloc)
for the heap, single-threaded (no FreeRTOS task pinning), networking
through the board crate via
[`nros-smoltcp`](../../drivers/nros-smoltcp).

## Source layout

| File | Role |
|------|------|
| `src/lib.rs` | `Esp32Platform` zero-sized type + trait impls. |
| `src/clock.rs` | `esp_timer_get_time` → ms/us. |
| `src/memory.rs` | esp-alloc heap allocator. |
| `src/random.rs` | ESP32 hardware RNG. |

## When to use

- Real ESP32 (Xtensa) hardware. Wi-Fi or Ethernet via esp-hal.

## Caveats

- ESP32 has hardware atomic CAS (Xtensa LX6) — works with stdlib `Arc`,
  unlike the ESP32-C3 RISC-V variant.
- `unstable-assume-single-core` cfg can simplify atomics if the
  application pins to a single core; not required for default builds.

## See also

- [Custom Platform porting guide](../../../book/src/porting/custom-platform.md)
- Source on GitHub: <https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platforms/nros-platform-esp32>
