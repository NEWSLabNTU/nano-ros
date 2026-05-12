# nros platform-cffi {#mainpage}

Canonical C ABI for porting the nano-ros platform abstraction in C (or
any language with a C ABI). Use this surface when nano-ros's pre-built
platform support (POSIX, Zephyr, FreeRTOS, NuttX, ThreadX) does not
cover your target and your port stays in C.

The ABI is a set of free `extern "C"` symbols — one per platform
capability. Each rule (buffer ownership, threading, blocking allowance)
is documented on the corresponding function declaration below. The
binary links exactly one platform implementation; resolution is
performed at link time. There is no runtime registration step.

This sits one tier below the Phase 117 RMW vtable
(`<nros/rmw_vtable.h>`). The RMW layer is a runtime-pluggable struct
of function pointers because RMW backends genuinely swap per session.
The platform layer is link-time-bound free symbols because a platform
is fixed for the life of a binary.

## Quick start

1. Build nano-ros with the `platform-cffi` option enabled:

   ```bash
   cmake -DNROS_PLATFORM=cffi -DNROS_RMW=zenoh -B build
   cmake --build build
   ```

2. Implement the platform symbols in C:

   ```c
   #include <nros/platform.h>

   uint64_t nros_platform_clock_ms(void) { return /* monotonic ms */; }
   void    *nros_platform_alloc(size_t size) { return malloc(size); }
   void     nros_platform_dealloc(void *p) { free(p); }
   /* ... define every symbol declared in <nros/platform.h> ... */
   ```

3. Link the translation unit (or its static library) into your nros
   binary. No registration call is required — nros invokes the symbols
   directly.

## API reference

See `<nros/platform.h>` for the full list and the per-function
return-value / threading / blocking conventions. The capability split:

- **Clock** (`nros_platform_clock_ms`, `..._clock_us`) — monotonic counter
- **Alloc** (`nros_platform_alloc`, `..._realloc`, `..._dealloc`) — heap interface
- **Sleep** (`nros_platform_sleep_us`, `..._sleep_ms`, `..._sleep_s`) — blocking sleep
- **Yield** (`nros_platform_yield_now`) — cooperative-yield primitive
- **Random** (`nros_platform_random_u8` … `..._random_u64`, `..._random_fill`) — entropy
- **Time** (`nros_platform_time_now_ms`, `..._time_since_epoch_*`) — wall clock
- **Threading** (`nros_platform_task_*`) — spawn / join / detach / cancel / exit / free
- **Mutexes** (`nros_platform_mutex_*` non-recursive, `..._mutex_rec_*` recursive)
- **Condvars** (`nros_platform_condvar_*`, including `..._condvar_wait_until`)

## Stub strategy

A platform that lacks a capability (e.g., bare-metal with no kernel
threads) can still satisfy the ABI by stubbing out the missing ops:

| Op family | Stub behaviour |
|-----------|----------------|
| `task_*` | `task_init` returns `-1`; the rest unreachable. |
| `mutex_*` / `mutex_rec_*` | All return `0`; storage is a no-op. Safe on single-core no-preempt systems. |
| `condvar_*` | `signal`/`signal_all` return `0`; `wait`/`wait_until` return `-1` so callers fall back to polling. |
| `random_*` | Seeded LCG is fine if the platform has no entropy source. **Must be deterministic** for reproducible tests. |

## Rust implementors

Rust platform crates (e.g. `nros-platform-posix`) keep implementing the
[`nros_platform_api`] traits. A sibling `-cffi` shim crate re-exports
the Rust impl as `#[unsafe(no_mangle)] extern "C"` symbols matching the
names in `<nros/platform.h>`. The same Rust impl serves both
trait-driven Rust callers and C-ABI consumers.

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
  — header + Rust mirror for this ABI.
