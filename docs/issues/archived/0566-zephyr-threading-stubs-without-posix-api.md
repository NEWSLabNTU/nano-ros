---
id: 566
title: "Without CONFIG_POSIX_API the Zephyr port's mutex / condvar / task primitives are stubs that return -1 at RUNTIME, on a kernel that has all of them natively"
status: resolved
resolved_in: "same-day fix; native k_mutex/k_condvar/k_thread arm"
type: bug
area: platform-zephyr
related: [issue-0531]
---

## Symptom

`tests/zephyr-c-smoke` on `qemu_cortex_m3` gets past the clock and alloc
checks and then fails:

```
nros-platform-zephyr-c smoke begin
  clock_ms: 0 -> 60
  random_u32: 0x4c982993
FAIL: mutex_init (src/main.c:63)
```

`nros_platform_mutex_init` returned -1 on a freshly-zeroed, successfully
allocated object.

## Cause

`packages/platform/nros-platform-zephyr/src/platform.c:177` gates the
whole threading half of the port on `CONFIG_POSIX_API`:

```c
#ifdef CONFIG_POSIX_API
    /* real implementations, pthread-backed */
#else
    /* ~20 functions that return -1 */
#endif
```

The `#else` arm stubs **every** task, mutex, recursive-mutex and condvar
entry point (`platform.c:322-410`): `task_init/join/detach/cancel/free`,
`mutex_{init,drop,lock,try_lock,unlock}`, the five `mutex_rec_*` aliases,
and `condvar_{init,drop,signal,signal_all,…}`. Each returns -1 or does
nothing.

The smoke build has `# CONFIG_POSIX_API is not set`, so it takes the stub
arm — as would any Zephyr application that does not opt into the POSIX
compatibility layer.

## Why this is worth fixing rather than documenting

1. **Zephyr has all of these natively.** `k_mutex_init/lock/unlock`,
   `k_condvar_*` and `k_thread_create` are kernel API, available on every
   board with no Kconfig opt-in. The port reaches for the POSIX
   compatibility layer and, failing to find it, gives up — rather than
   using the primitives the kernel already provides. Compare the FreeRTOS
   and ThreadX ports, which call their kernels directly.
2. **The failure is at RUNTIME, and silent.** A -1 from `mutex_init` is a
   value a caller may not check. Anything built on these primitives —
   zenoh-pico's session locks, the executor's multi-tier path — gets a
   non-functional mutex rather than a build error naming the missing
   Kconfig.
3. **It narrows which Zephyr boards are actually usable.** `native_sim`
   configs in this tree enable the POSIX API, which is why the lane has
   always looked healthy; a small Cortex-M app that does not is silently
   degraded.

## Resolution (2026-08-14)

The `#else` arm is implemented against native Zephyr primitives instead
of stubbed. `mutex_*` and `mutex_rec_*` are backed by `k_mutex` (which is
recursive for the owning thread via `lock_count` — `kernel/mutex.c:117`,
exactly what the ABI requires of the `_rec_` family), `condvar_*` by
`k_condvar`, and `task_*` by `k_thread` where a stack can be allocated.

Storage: the caller's opaque object holds a POINTER to a heap-allocated
kernel object rather than the object inline. It has to — the smallest
consumer on this platform sizes these from `pthread_mutex_t`, a
`uint32_t`, which cannot hold a `struct k_mutex`. This is the same shape
the FreeRTOS port uses for its `SemaphoreHandle_t`.

`task_*` still returns -1 without `CONFIG_DYNAMIC_THREAD` +
`CONFIG_THREAD_STACK_INFO`, because a dynamically created thread needs a
dynamically allocated stack. That case is now narrow and documented at
the call site, and it no longer takes mutexes and condvars down with it.

Verified on both arms of the `#ifdef`:

    qemu_cortex_m3 (no POSIX_API)   smoke PASS — mutex round-trip ok,
                                    timer fires over 200ms: 10
    native_sim (POSIX_API)          smoke PASS — unchanged

## Fix direction (as filed)

- Implement the `#else` arm against native Zephyr primitives (`k_mutex`,
  `k_condvar`, `k_thread`) instead of stubbing it. The opaque-storage
  sizes already flow through `nros-platform-api`, so this is a
  per-function body change, not an ABI change.
- Failing that (as an interim), make the stub arm a **build** error:
  `#error "nros-platform-zephyr requires CONFIG_POSIX_API — or build with
  … "`. A compile-time refusal is strictly better than twenty functions
  that return -1.
- Either way the smoke test should assert this on a non-POSIX board, so
  the gap cannot reopen silently. It was only found because #531's
  verification needed a board that had never run the suite.

## Not investigated

Whether the executor or zenoh-pico actually reach these paths on a
POSIX-less Zephyr build, or fail earlier for other reasons. The port
claiming a capability it does not provide is the defect regardless.
