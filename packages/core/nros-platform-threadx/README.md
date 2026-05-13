# nros-platform-threadx-c

Native C implementation of the nano-ros canonical platform ABI (`<nros/platform.h>`) for [Azure RTOS ThreadX](https://azure.microsoft.com/en-us/products/rtos/).

Behavioural parity with [`nros-platform-threadx`](../nros-platform-threadx)'s Rust impl:

| Capability | ThreadX primitive |
|---|---|
| Clock      | `tx_time_get()` scaled by `TX_TIMER_TICKS_PER_SECOND` |
| Allocation | `tx_byte_allocate` / `tx_byte_release` against a caller-provided `TX_BYTE_POOL` (set once via `nros_platform_threadx_set_byte_pool`) |
| Sleep      | `tx_thread_sleep(ms_to_ticks)` |
| Yield      | `tx_thread_relinquish()` |
| Random     | Deterministic xorshift64; seedable via `nros_platform_threadx_seed_rng(u32)` |
| Time       | Wall clock unsupported; returns 0 |
| Tasks      | `tx_thread_create` + `tx_thread_terminate` + `tx_thread_delete`. `task_init`'s `attr` parameter is a `nros_threadx_task_attr_t` carrying name + priority + stack pointer + stack depth — ThreadX does not allocate task stacks. |
| Mutexes    | `tx_mutex_create(TX_INHERIT)`. ThreadX mutexes are recursive by design; `mutex_*` and `mutex_rec_*` share the same primitive. |
| Condvars   | `tx_semaphore`-backed. `condvar_signal_all` matches the Rust impl's "wake one" approximation (ThreadX has no broadcast). |

## Byte-pool wiring

ThreadX has no global heap. Before the first `nros_platform_alloc` call, the application creates a `TX_BYTE_POOL` and registers it:

```c
#include <tx_api.h>

extern void nros_platform_threadx_set_byte_pool(void *pool);

static TX_BYTE_POOL heap_pool;
static uint8_t heap_storage[64 * 1024];

void tx_application_define(void *first_unused_memory) {
    tx_byte_pool_create(&heap_pool, "nros heap",
                        heap_storage, sizeof(heap_storage));
    nros_platform_threadx_set_byte_pool(&heap_pool);
    /* ... */
}
```

## Build

```bash
cmake -B build -DTHREADX_KERNEL_TARGET=threadx
cmake --build build
```

The parent build must declare an imported CMake target whose name matches `THREADX_KERNEL_TARGET` and that provides `tx_api.h` + the kernel sources for the host port (Cortex-M, Cortex-R, RISC-V, x86_64-Linux simulator, …).

## License

Apache-2.0 or MIT at your option.
