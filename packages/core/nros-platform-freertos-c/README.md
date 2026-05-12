# nros-platform-freertos-c

Native C implementation of the nano-ros canonical platform ABI (`<nros/platform.h>`) for [FreeRTOS](https://www.freertos.org/).

Behavioural parity with [`nros-platform-freertos`](../nros-platform-freertos)'s Rust impl:

| Capability | FreeRTOS primitive |
|---|---|
| Clock      | `xTaskGetTickCount()` scaled by `configTICK_RATE_HZ` |
| Allocation | `pvPortMalloc` / `vPortFree`. `realloc` emulated (malloc + memcpy + free; copies up to `new_size`). |
| Sleep      | `vTaskDelay(pdMS_TO_TICKS(ms))` |
| Yield      | `vTaskDelay(1)` (tick-quantum reschedule; matches Rust impl) |
| Random     | Deterministic xorshift64; seedable via `nros_platform_freertos_seed_rng(u32)` |
| Time       | Wall clock unsupported; returns 0 |
| Tasks      | `xTaskCreate` + self-`vTaskDelete`; storage shape matches zenoh-pico's `_z_task_t` |
| Mutexes    | `xSemaphoreCreateMutex` / `xSemaphoreCreateRecursiveMutex` |
| Condvars   | Mutex + counting-semaphore + waiter counter (mirrors zenoh-pico's `_z_condvar_t`) |

The internal struct layouts for `task`, `mutex`, and `condvar` storage match the Rust impl's `ZTask` / `ZMutex` / `ZCondvar` byte-for-byte, so a binary linking this C port is wire-compatible with zenoh-pico's FreeRTOS expectations.

## Build

```bash
cmake -B build \
  -DFREERTOS_KERNEL_TARGET=freertos_kernel \
  -DFREERTOS_CONFIG_TARGET=my_board_freertos_config
cmake --build build
```

The parent build must declare two imported CMake targets:

- **`freertos_kernel`** (or whatever `FREERTOS_KERNEL_TARGET` names) — provides `FreeRTOS.h`, `task.h`, `semphr.h`, and the kernel sources.
- **`my_board_freertos_config`** (or `FREERTOS_CONFIG_TARGET`) — provides `FreeRTOSConfig.h` on the include path.

Both shapes are common: vanilla FreeRTOS-Kernel checkouts ship a CMake target named `freertos_kernel`; vendor SDKs name it differently. The defaults assume the vanilla naming.

## License

Apache-2.0 or MIT at your option.
