# nros-platform-zephyr-c

Native C implementation of the nano-ros canonical platform ABI (`<nros/platform.h>`) for [Zephyr RTOS](https://www.zephyrproject.org/) (2.5+ for `k_condvar_*`).

Behavioural parity with [`nros-platform-zephyr`](../nros-platform-zephyr)'s Rust impl. The Rust port had to route through C shims for Zephyr's static-inline macros; the native C port calls them directly.

| Capability | Zephyr primitive |
|---|---|
| Clock      | `k_uptime_get()` (ms); `k_cyc_to_us_floor64(k_cycle_get_64())` (us) |
| Allocation | `k_malloc` / `k_free`. `realloc` emulated (malloc + memcpy + free). |
| Sleep      | `k_msleep` / `k_usleep` / `k_sleep(K_SECONDS(s))` |
| Yield      | `k_yield()` |
| Random     | `sys_rand32_get`; `sys_rand_get` for byte fills |
| Time       | Wall clock unsupported without `CONFIG_RTC`; returns 0 |
| Tasks      | `k_thread_create` + `k_thread_join` + `k_thread_abort`. attr carries name, priority, and the caller's `K_THREAD_STACK_DEFINE`'d stack region. |
| Mutexes    | `k_mutex` (recursive by design); `mutex_*` and `mutex_rec_*` share the primitive. |
| Condvars   | `k_condvar_*` (Zephyr 2.5+) |

## Build (Zephyr module)

Register this directory as a Zephyr module in your `west.yml`:

```yaml
manifest:
  projects:
    - name: nano-ros
      path: modules/lib/nano-ros
      url: https://github.com/NEWSLabNTU/nano-ros
```

The `zephyr` interface library auto-supplies kernel headers. A Zephyr application that pulls this module then has `libnros_platform_zephyr.a` available as a CMake target.

## License

Apache-2.0 or MIT at your option.
