# nros-platform-freertos

FreeRTOS platform implementation for nano-ros. Targets the
`portable/GCC/ARM_*` ports of FreeRTOS (Cortex-M3 / M4 / M7 by default;
Cortex-R5 reachable via Phase 100 work).

## Role

Implements the trait family in
[`nros-platform-api`](../nros-platform-api) on top of FreeRTOS:
`xTaskGetTickCount` for monotonic time, `pvPortMalloc` / `vPortFree`
for heap, FreeRTOS tasks + queues + semaphores for threading, lwIP
sockets (via [`freertos-lwip-sys`](../../drivers/freertos-lwip-sys))
for networking.

## Source layout

| File | Role |
|------|------|
| `src/lib.rs` | `FreeRtosPlatform` zero-sized type + trait impls. |
| `src/clock.rs` | `xTaskGetTickCount` → ms/us. |
| `src/alloc.rs` | `pvPortMalloc` / `vPortFree` shims. |
| `src/thread.rs` | FreeRTOS task / mutex / condvar (semaphore-backed). |
| `src/net.rs` | lwIP socket bindings via `freertos-lwip-sys`. |

## When to use

- FreeRTOS-based MCU board with `portable/GCC/ARM_CMx`.
- lwIP for networking; needs `FreeRTOSConfig.h` + `lwipopts.h` from
  the board crate's `config/` dir.

## Caveats

- Stack overflow on `task_init` is the most common bring-up failure —
  raise the `attr.stack_size` parameter (default FreeRTOS demos ship
  with low values).
- Must be paired with a board crate (e.g.
  [`nros-board-mps2-an385-freertos`](../../boards/nros-board-mps2-an385-freertos))
  that provides FreeRTOSConfig.h + lwipopts.h + an Ethernet driver.

## See also

- [Custom Platform porting guide](../../../book/src/porting/custom-platform.md)
- [FreeRTOS LAN9118 debugging guide](../../../docs/guides/freertos-lan9118-debugging.md)
- Source on GitHub: <https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-freertos>
