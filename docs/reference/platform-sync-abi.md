# Platform Synchronisation ABI

Reference for the `nros_platform_*` synchronisation primitives
declared in `<nros/platform.h>` (sources: `packages/core/nros-platform-cffi/include/nros/platform.h`).

Two primitive families are exposed today:

1. **Condvar + mutex** (`nros_platform_condvar_*`, `nros_platform_mutex_*`) —
   used by `nros-platform-zenoh` and other libraries that mirror
   POSIX `pthread_cond_t` / `pthread_mutex_t`.
2. **Wake** (`nros_platform_wake_*`, Phase 130) — binary-semaphore-
   shaped primitive used by `Executor::spin_once`'s wake_flag /
   cv-wait pair. Added because Zephyr's libc
   `pthread_cond_timedwait` hangs past its deadline (Phase
   127.C.4), making the std `Condvar` path unusable there. The
   binary-semaphore shape is a clean fit for kernel-native
   primitives on every supported RTOS.

This document covers (2) — the wake primitive — and the
ISR-safety contract each platform must honour.

## Wake primitive ABI

```c
int8_t  nros_platform_wake_init(void *w);
int8_t  nros_platform_wake_drop(void *w);
int8_t  nros_platform_wake_wait_ms(void *w, uint32_t timeout_ms);
int8_t  nros_platform_wake_signal(void *w);
int8_t  nros_platform_wake_signal_from_isr(void *w);
size_t  nros_platform_wake_storage_size(void);
size_t  nros_platform_wake_storage_align(void);
```

Return contract:

- `_init`     — `0` on success, `-1` on error (out-of-memory, kernel-resource exhaustion).
- `_drop`     — `0` always (idempotent for NULL).
- `_wait_ms`  — `0` on signal, `1` on timeout, `-1` on error.
- `_signal`   — `0` always (success). Coalesces — a signal pending
                when another arrives leaves the primitive at value
                1 (binary semaphore semantics).
- `_signal_from_isr` — `0` on success, `-1` if the platform has no
                        ISR-safe path (caller falls back to
                        `_signal` with the obvious latency cost).
- `_storage_size`  — bytes the caller must provide for `_init`. Pure.
- `_storage_align` — alignment the caller must respect. Pure.

## Per-platform backing primitive

| Platform | Source | Backing primitive | ISR-safe `signal_from_isr`? |
|----------|--------|-------------------|------------------------------|
| POSIX (Linux/glibc)         | `nros-platform-posix/src/platform.c` | `sem_t` + `sem_timedwait`               | aliased to `signal` (no real ISR context on hosted POSIX) |
| POSIX (macOS)               | `nros-platform-posix/src/platform.c` | `pthread_cond_t` + `pthread_mutex_t` + flag (unnamed `sem_t` deprecated) | aliased to `signal` |
| Zephyr                      | `nros-platform-zephyr/src/platform.c` | `k_sem`                                 | **yes** — `k_sem_give` is documented ISR-safe |
| FreeRTOS                    | `nros-platform-freertos/src/platform.c` | `xSemaphoreCreateBinary`                | **yes** — `xSemaphoreGiveFromISR` + `portYIELD_FROM_ISR` |
| ESP-IDF (FreeRTOS-derived)  | `nros-platform-esp-idf/src/platform.c` | `xSemaphoreCreateBinary`                | **yes** — `xSemaphoreGiveFromISR` (per-SoC `portYIELD_FROM_ISR`) |
| NuttX                       | reuses `nros-platform-posix/src/platform.c` | POSIX `sem_t`                           | aliased to `signal` (NuttX `sem_post` is ISR-safe by spec but the wrapper does not yet distinguish; track in a follow-up) |
| ThreadX                     | `nros-platform-threadx/src/platform.c` | `tx_semaphore` + `tx_semaphore_ceiling_put` | **yes** — `tx_semaphore_put`/`_ceiling_put` are ISR-safe per ThreadX spec |
| bare-metal (Cortex-M)       | (none — wake primitive returns `-1` from the trait default) | — | n/a (single-thread, no ISR-driven wake needed) |

## Storage sizing

Callers allocate aligned storage and call `_init`. The size is
queried at runtime via `nros_platform_wake_storage_size()`; the
Rust executor's `NodeWake` wrapper rounds capacity up to the
nearest `u64` to satisfy 8-byte alignment, which covers every
backing primitive listed above.

Indicative sizes (subject to platform ABI / build flags):

| Platform | `sizeof` wake storage |
|----------|----------------------|
| POSIX (Linux x86_64) | 32 (`sem_t`) |
| POSIX (macOS)        | ~72 (pthread cond + mutex + int) |
| Zephyr               | 16–24 (`k_sem` = `_wait_q_t` + optional `obj_core`) |
| FreeRTOS / ESP-IDF   | 8 (pointer to dynamically allocated semaphore) |
| NuttX                | 16–32 (`sem_t`) |
| ThreadX              | ~64 (`TX_SEMAPHORE` control block) |

## Coalescing semantics

The wake primitive is a **binary** semaphore: multiple `_signal`
calls between waits coalesce into a single pending wake. The
executor relies on this so an ISR or worker thread can call
`_signal` repeatedly without overflowing a counter or accumulating
stale wake credits.

- POSIX: `sem_post` is guarded by `sem_getvalue() > 0 ? noop : post`.
- Zephyr/FreeRTOS/ESP-IDF: binary semaphore (init max 1).
- ThreadX: `tx_semaphore_ceiling_put` with ceiling 1.

## Verification status (Phase 130.7)

| Platform | Compile-check (`cargo check`) | Link + integration test |
|----------|-------------------------------|--------------------------|
| POSIX (Linux) | ✅ | ✅ 15 tests pass (`c_port_posix_wake.rs`, `wake_wrapper.rs`) |
| POSIX (macOS) | ✅ | not tested in CI |
| Zephyr native_sim | ✅ | ✅ all 13 Zephyr XRCE E2E pass |
| Zephyr qemu_cortex_a9 | ✅ | not run since 130.x landed |
| FreeRTOS | ✅ | needs FreeRTOS QEMU smoke |
| NuttX | ✅ (reuses POSIX C source) | needs NuttX QEMU smoke |
| ThreadX | ✅ | needs ThreadX QEMU smoke |
| ESP-IDF | ✅ | needs ESP32-QEMU smoke |
| bare-metal (Cortex-M) | trait default = unsupported | n/a |

RTOS targets are compile-clean (the `cc::Build` step succeeds for
each `platform-*` feature), but the binary-semaphore impls have
only been runtime-exercised on POSIX and Zephyr. FreeRTOS / NuttX
/ ThreadX / ESP-IDF runtime validation tracks as a Phase 130
follow-up — needs each platform's QEMU smoke harness to run a
service or action E2E.

## Consumer expectations

`Executor::spin_once` on `std + rmw-cffi + any(platform-{zephyr,
freertos, nuttx, threadx})` constructs a `NodeWake` at
`Executor::open` time. When the platform provider hasn't linked a
wake primitive (the trait's default body returns
`storage_size() == 0`), `NodeWake::new()` returns `None` and
`spin_once` falls back to driving the transport for the full
timeout via the platform's blocking `recv` — same wall-clock
behaviour as the cv-wait branch.

Backends with `set_wake_callback` (Phase 124.B) installed signal
the wake primitive from their async notify path; `spin_once`
returns sub-poll-period after the callback fires.

## Why a separate ABI from `nros_platform_condvar_*`

The existing condvar ABI on Zephyr wraps `pthread_cond_t` to
match zenoh-pico's expected Zephyr binding (per
`nros-platform-zephyr/src/platform.c:22`). Replacing it with a
`k_condvar_*` implementation would silently change behaviour for
zenoh-pico's internal locking. The wake ABI is purpose-built for
the executor's wake_flag / spin_once pair and lives alongside
the condvar ABI without disturbing existing consumers.

## Cross-references

- `packages/core/nros-platform-cffi/include/nros/platform.h` —
  canonical declarations.
- `packages/core/nros-platform-api/src/wake.rs` — Rust
  `Wake<P>` ergonomic wrapper.
- `packages/core/nros-node/src/executor/node_wake.rs` — executor's
  internal `NodeWake` (heap-backed, uses the FFI directly).
- `docs/roadmap/phase-130-platform-wake-primitive.md` — phase
  plan + acceptance.
