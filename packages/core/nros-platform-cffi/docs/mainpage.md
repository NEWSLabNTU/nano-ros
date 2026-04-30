# nros platform-cffi {#mainpage}

C function-pointer table for porting the nano-ros platform abstraction
in C (or any language with a C ABI). Use this surface when nano-ros's
pre-built platform support (POSIX, Zephyr, FreeRTOS, NuttX, ThreadX)
does not cover your target and your port stays in C.

The vtable is a struct of function pointers covering one capability
each. Every behaviour rule (buffer ownership, threading, blocking
allowance) is documented on the corresponding function pointer below.

## Quick start

1. Build nano-ros with the `platform-cffi` option enabled:

   ```bash
   cmake -DNROS_PLATFORM=cffi -DNROS_RMW=zenoh -B build
   cmake --build build
   ```

2. Implement the vtable in C:

   ```c
   #include <nros/platform_vtable.h>

   static uint64_t my_clock_ms(void) { return /* monotonic ms */; }
   static void*    my_alloc(size_t size) { return malloc(size); }
   /* ... fill in every field ... */

   static const struct NrosPlatformVtable VTABLE = {
       .clock_ms        = my_clock_ms,
       .alloc           = my_alloc,
       /* ... */
   };
   ```

3. Register before any nros call:

   ```c
   int main(void) {
       nros_platform_cffi_register(&VTABLE);
       /* now you can call nros_init(), nros_node_init(), ... */
   }
   ```

## Vtable reference

See @ref NrosPlatformVtable for the full struct and the per-field
return-value / threading / blocking conventions. The grouping inside
the struct follows the platform capability split:

- **Clock** (`clock_ms`, `clock_us`) — monotonic counter
- **Alloc** (`alloc`, `realloc`, `dealloc`) — heap interface
- **Sleep** (`sleep_us`, `sleep_ms`, `sleep_s`) — blocking sleep
- **Yield** (`yield_now`) — cooperative-yield primitive
- **Random** (`random_u8`–`random_u64`, `random_fill`) — entropy
- **Time** (`time_now_ms`, `time_since_epoch_*`) — wall clock
- **Threading** (`task_*`) — spawn / join / detach / cancel / exit / free
- **Mutexes** (`mutex_*` non-recursive, `mutex_rec_*` recursive)
- **Condvars** (`condvar_*`) — including `condvar_wait_until`

## Stub strategy

A platform that lacks a capability (e.g., bare-metal with no kernel
threads) can still register a complete vtable by stubbing out the
missing ops:

| Op family | Stub behaviour |
|-----------|----------------|
| `task_*` | `task_init` returns `-1`; the rest unreachable. |
| `mutex_*` / `mutex_rec_*` | All return `0`; storage is a no-op. Safe on single-core no-preempt systems. |
| `condvar_*` | `signal`/`signal_all` return `0`; `wait`/`wait_until` return `-1` so callers fall back to polling. |
| `random_*` | Seeded LCG is fine if the platform has no entropy source. **Must be deterministic** for reproducible tests. |

## Pitfalls

- **Recursive mutexes** — zenoh-pico holds the same `mutex_rec_*`
  re-entrantly from the same thread. A non-recursive mutex backing
  `mutex_rec_*` will deadlock under load.
- **Poll-driven clocks** — `condvar_wait_until` callers compare
  against `clock_ms()`. The two must share the same monotonic origin.
- **Allocator behaviour during ISRs** — nano-ros never calls `alloc`
  from an ISR, but if your `random_*` does it must be lock-free.
- **Stack overflow on `task_init`** — RTOS task stacks ship with low
  defaults; raise via the `attr` parameter.

## See also

- The [Custom Platform porting guide](https://github.com/NEWSLabNTU/nano-ros/blob/main/book/src/porting/custom-platform.md)
  — step-by-step walkthrough.
- The [`nros-platform-cffi` source tree](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-cffi)
  — header + library sources for this vtable.
