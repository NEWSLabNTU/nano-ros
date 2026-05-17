# Phase 130 — Platform-Native Wake Primitive

**Goal.** Replace `std::sync::Condvar` in `Executor` with a
platform-supplied wake primitive routed through the canonical
`nros_platform_condvar_*` ABI, and back the Zephyr impl with
`k_condvar_*` (kernel-native) instead of libc `pthread_cond_timedwait`.
Closes the structural cv-wait hang that Phase 127.C.4 patched
with a Zephyr+std-only gate in `Executor::spin_once`.

**Status.** Not started.

**Priority.** P1 — blocks future event-driven RMW backends (Phase 124.B
`set_wake_callback`) from working on Zephyr+std; without this, every new
backend with an async notify path silently degrades to polling on
Zephyr.

**Depends on.** Phase 124.B (`set_wake_callback` ABI), Phase 127.C.4
(the expedient `not(feature = "platform-zephyr")` gate this phase
removes).

## Overview

`Executor::spin_once` blocks on `wake_cv.wait_timeout_while` between
`drive_io` calls so that backends with `set_wake_callback` (Phase
124.B) can pre-empt the spin from a worker thread or ISR. On Zephyr's
libc `pthread_cond_timedwait` ignores its deadline and blocks past
the timeout, which surfaced in Phase 127.C.4 as:

- C++ XRCE action `send_goal` hung in the first `spin_once` and
  never sent the goal request.
- C++ XRCE service replies stuck unACK'd in the server's reliable
  output stream because the bypass workaround (`drive_io(0) +
  nros_zephyr_msleep`) starved retransmission.

Phase 127.C.4 papered over the hang by skipping the cv-wait on
Zephyr+std and routing the full timeout into `drive_io`. That works
today because every shipping cffi backend (XRCE, Cyclone, dust-DDS,
zenoh-pico-cffi) leaves the `set_wake_callback` slot NULL — the
cv-wait was just a poll-on-timer there anyway. Any future RMW that
opts into event-driven wake would silently lose pre-emption on
Zephyr+std.

The fix is to stop depending on std for the wake primitive at all.
`nros_platform_condvar_*` is already in the platform ABI. Today its
Zephyr impl wraps the same broken libc `pthread_cond_timedwait`;
this phase swaps it for `k_condvar_*` (or a `k_sem`-backed fallback
when `CONFIG_POSIX_API=n`), then routes the executor through the
platform primitive.

## Architecture

The existing `nros_platform_condvar_*` ABI was deliberately shaped to
match zenoh-pico's `pthread_cond_t` Zephyr ABI ("matching
zenoh-pico's Zephyr ABI", `nros-platform-zephyr/src/platform.c:22`),
so it's pinned to libc pthread on Zephyr. Rather than break that
interop contract, this phase adds a **separate** wake primitive
sized after a binary semaphore — exactly the shape `Executor`'s
wake_flag/wake_cv pair needs, and a clean fit for `k_sem` /
`tx_semaphore` / `xSemaphoreBinary` / POSIX `sem_t`.

```
Executor::spin_once
  ├── PlatformWake.wait_ms(timeout_ms)          ← no std, no libc pthread
  ├── session.drive_io(...)
  └── arena dispatch

nros-platform-* (per-platform)              backing primitive
  ├── nros_platform_wake_init                <─ POSIX:    sem_t / eventfd
  ├── nros_platform_wake_drop                <─ Zephyr:   k_sem (kernel-native)
  ├── nros_platform_wake_wait_ms             <─ FreeRTOS: xSemaphoreBinary
  ├── nros_platform_wake_signal              <─ NuttX:    POSIX sem_t
  └── nros_platform_wake_signal_from_isr     <─ ThreadX:  tx_semaphore
                                             <─ bare:     atomic flag + spin
```

`nros_platform_condvar_*` stays as-is for zenoh-pico's Zephyr
ABI compat. New `nros_platform_wake_*` is the executor's primitive.

Backend obligation unchanged. `set_wake_callback` installs a runtime
closure that writes `wake_flag = true` then calls
`nros_platform_condvar_signal{,_from_isr}` on the executor's
condvar handle. Spin loop blocks in `PlatformCondvar.wait_until`
and wakes on either:
- backend wake-callback firing (event-driven path), or
- the wall-clock deadline expiring (poll-on-timer fallback for
  backends with NULL callback).

## Work Items

### 130.1 — `nros_platform_wake_*` ABI

**Files**
- `packages/core/nros-platform-cffi/include/nros/platform.h`
- `packages/core/nros-platform-cffi/src/lib.rs`

Add the new ABI surface (extern `"C"` decls + Rust FFI bindings):

```c
int8_t  nros_platform_wake_init(void *w);
int8_t  nros_platform_wake_drop(void *w);
int8_t  nros_platform_wake_wait_ms(void *w, uint32_t timeout_ms);
int8_t  nros_platform_wake_signal(void *w);
int8_t  nros_platform_wake_signal_from_isr(void *w);
size_t  nros_platform_wake_storage_size(void);  /* probe for sizing */
size_t  nros_platform_wake_storage_align(void);
```

`wait_ms` returns `0` on signal, `1` on timeout, `-1` on error.
Storage sizing follows the Phase 118.B probe pattern so the Rust
wrapper can sit on `MaybeUninit<[u64; N]>`.

### 130.1.zephyr — Zephyr `k_sem` impl

**Files**
- `packages/core/nros-platform-zephyr/src/platform.c`

Implement `nros_platform_wake_*` against `k_sem_*`. Binary
semaphore (max_count=1). `k_sem_take(K_MSEC(timeout_ms))` honors
the deadline; `k_sem_give` is documented ISR-safe. No libc
pthread dependency. Lives alongside the existing pthread-backed
`nros_platform_condvar_*` (kept for zenoh-pico's Zephyr ABI).

### 130.2 — Rust `PlatformWake` wrapper

**Files**
- `packages/core/nros-platform/src/wake.rs` (new)
- `packages/core/nros-platform/src/lib.rs` (re-export)

`no_std`-safe Rust wrapper around `nros_platform_wake_*`:

```rust
pub struct PlatformWake { storage: MaybeUninit<[u64; WAKE_OPAQUE_U64S]> }

pub enum WakeReason { Signaled, Timeout }

impl PlatformWake {
    pub fn new() -> Self;                                  // calls _init
    pub fn wait_ms(&self, timeout_ms: u32) -> WakeReason;  // calls _wait_ms
    pub fn signal(&self);                                  // calls _signal
    pub fn signal_from_isr(&self);                         // calls _signal_from_isr
}

impl Drop for PlatformWake { ... }                         // calls _drop
```

`WAKE_OPAQUE_U64S` derived via a `nros_sizes_build`-style probe
(same pattern as `EXECUTOR_OPAQUE_U64S`, Phase 118.B). On
`--no-default-features` check builds the probe returns 0 and
emits a one-word placeholder; the resulting rlib must not be
linked.

### 130.3 — Executor swap

**Files**
- `packages/core/nros-node/src/executor/spin.rs`
- `packages/core/nros-node/src/executor/types.rs`
- `packages/core/nros-node/src/executor/mod.rs`

Replace the std `wake_cv: Arc<Condvar>` + `wake_mu: Arc<Mutex<()>>`
pair with a single `wake: Arc<PlatformWake>`. `wake_flag` stays —
it's the lost-wakeup guard the wake-callback writes before
`wake.signal()`.

Spin loop becomes:

```rust
#[cfg(feature = "rmw-cffi")]
if !was_woken {
    let _ = self.wake.wait_ms(timeout_ms as u32);
}
```

Gate is `feature = "rmw-cffi"` only — no platform-specific cfg.
`primary_drive_timeout_ms = 0` for any `rmw-cffi` build (wait
above already burned the timeout).

### 130.4 — Revert Phase 127.C.4 expedient gate

**Files**
- `packages/core/nros-node/src/executor/spin.rs`

Drop the `not(feature = "platform-zephyr")` gate added in `ba76394b`.
Drop the `any(not(feature = "std"), feature = "platform-zephyr")`
gate on `primary_drive_timeout_ms`. Single std cv-wait path again.

Verify `nros_cpp_spin_once` still compiles after the revert —
it already routes through `executor.spin_once` and needs no further
change.

### 130.5 — Other-platform `nros_platform_wake_*` impls

**Files**
- `packages/core/nros-platform-posix/src/platform.c`
- `packages/core/nros-platform-freertos/src/platform.c`
- `packages/core/nros-platform-nuttx/src/platform.c`
- `packages/core/nros-platform-threadx/src/platform.c`
- `packages/core/nros-platform-baremetal/src/platform.c`

Implement `nros_platform_wake_*` per platform using the
platform-native binary semaphore:
- POSIX: `sem_t` + `sem_timedwait` (`CLOCK_MONOTONIC`) or `eventfd`.
  ISR signal not meaningful on POSIX; alias to `signal`.
- FreeRTOS: `xSemaphoreCreateBinary` + `xSemaphoreGiveFromISR`.
- NuttX: POSIX `sem_t` (NuttX libc honors `sem_timedwait`).
- ThreadX: `tx_semaphore` + `tx_semaphore_put_from_isr` (via
  `tx_semaphore_put` — ThreadX semaphores are ISR-safe by spec).
- bare-metal: atomic flag + `wait_ms` busy-spin with the platform
  clock (no MT to signal cross-context anyway).

Document each impl's ISR-safety in
`docs/reference/platform-sync-abi.md` (new).

## Follow-ups (130.6 – 130.8)

- [x] **130.6 — tunable `XRCE_STREAM_HISTORY`.** `internal.h`
  guards the default behind `#ifndef XRCE_STREAM_HISTORY` and
  rejects values `< 4` at compile time. `nros-rmw-xrce-cffi/build.rs`
  reads `NROS_XRCE_STREAM_HISTORY` env var and passes it through
  as a `cc::Build::define`. Tight-RAM RTOS builds that don't run
  server-side action callbacks can drop to 8 (saving 32 KiB of
  per-session output buffer) or to the minimum 4.
- [x] **130.7 — verify wake_* on other RTOS.** `cargo check`
  passes for `rmw-cffi + platform-{zephyr,freertos,nuttx,threadx}`;
  per-platform C ports compile via `cc::Build` under their
  respective platform features. Runtime validation only done on
  POSIX (15 wake tests) and Zephyr (13 XRCE E2E). FreeRTOS /
  NuttX / ThreadX / ESP-IDF still need QEMU smoke harness runs;
  status table in `docs/reference/platform-sync-abi.md`.
- [x] **130.8 — deprecate legacy blocking `call_raw` fallback.**
  `CffiServiceClient::send_request_raw` now emits a one-shot
  `warn_legacy_send_recv_fallback()` warning when the backend's
  vtable omits the non-blocking `send_request_raw` /
  `try_recv_reply_raw` slots; documents the removal target
  (when Cyclone + dust-DDS ship the split).

## Acceptance

- [x] 130.1: `nros_platform_wake_*` declared in `platform.h` +
  Rust FFI bindings + Zephyr `k_sem` + POSIX `sem_t` impls.
  POSIX integration tests (7) pass.
- [x] 130.2: `PlatformWake` trait + `Wake<P>` RAII wrapper in
  `nros-platform-api`, default-unsupported bodies so existing
  Platform impls don't need updates. 8 wrapper tests pass.
- [x] 130.3: `cargo test -p nros-node --lib (--features rmw-cffi)`
  131/131 + 63/63 passes — no behavior change on POSIX hosts.
  Zephyr+std now routes spin_once through `NodeWake` (k_sem)
  instead of the broken `Condvar::wait_timeout_while`. Falls back
  to `drive_io(timeout_ms)` when the platform provider hasn't
  linked a wake primitive.
- [x] 130.4 service: `test_zephyr_xrce_cpp_service_e2e` PASSES
  with the wake gate routing through `drive_io(timeout_ms)`
  when no backend installed `set_wake_callback`. Server's
  reliable XRCE reply stream now gets retransmitted because
  `spin_once` actually runs the session for its full timeout
  instead of sleeping in a never-signaled wake-primitive wait.
  Phase 127.C.4 service case closeable.
- [x] 130.4 action: `test_zephyr_xrce_cpp_action_e2e` PASSES.
  Three contributing causes addressed:

  1. **Non-blocking CFFI send/recv split.** Added vtable slots
     `send_request_raw` + `try_recv_reply_raw` to
     `nros_rmw_vtable_t`; XRCE backend implements them (buffer
     + flush on send, slot-check on recv). CFFI prefers the
     non-blocking path when present, keeps the legacy blocking
     `call_raw` as fallback. Eliminates the
     `pending_len = 0`-on-timeout state loss that made arena
     trampolines miss late-arriving replies.
  2. **XRCE reliable stream history exhaustion.** Bumped
     `XRCE_STREAM_HISTORY` from 4 to 16. The C++ action server's
     `on_goal` callback publishes feedback ×3 +
     `complete_goal` (publishes status_array) + result inside
     one trampoline invocation before the arena drains ACKs;
     with history=4 the 5th `uxr_buffer_reply` (accept-goal
     response) returned `UXR_INVALID_REQUEST_ID` and the accept
     reply never reached the client. History=16 covers a typical
     action lifecycle with room to spare; costs an extra 48 KiB
     of per-session output buffer.
  3. **130.4 has_async_wake gate already in place.** Without it
     the spin loop would have slept in a never-signaled
     wake-primitive wait, masking the stream-history symptom
     for even longer.

  All 13 Zephyr XRCE E2E tests pass
  (`cargo nextest run -p nros-tests -E 'test(test_zephyr_xrce_)'`).
  Phase 127.C.4 is now closeable in full.
- [x] 130.4 `set_wake_callback` probe: `Session` gains
  `supports_wake_callback() -> bool` (default `false`); CFFI
  returns whether the vtable slot is non-NULL.
  `Executor::install_wake_signal_on_*` collects the probe into
  `has_async_wake`. `spin_once` only uses the wake-primitive
  wait (`NodeWake` or `std::Condvar`) when a backend actually
  honours the callback — poll-only backends route to
  `drive_io(timeout_ms)` so the transport's blocking `recv`
  keeps reliable streams ticking.
- [x] 130.5: `nros_platform_wake_*` impls landed for FreeRTOS
  (`xSemaphoreCreateBinary` + `xSemaphoreGiveFromISR`), ESP-IDF
  (FreeRTOS-derived; same surface + per-SoC `portYIELD_FROM_ISR`),
  ThreadX (`tx_semaphore_ceiling_put`), and NuttX (POSIX `sem_t`
  via the shared POSIX C source). Bare-metal platforms inherit
  the trait's default-unsupported bodies — single-threaded, no
  ISR-driven wake needed. Executor's `NodeWake` gate widened
  from `platform-zephyr` to
  `any(platform-zephyr, platform-freertos, platform-nuttx,
   platform-threadx)` so every RTOS std build picks up the
  kernel-native primitive automatically. ISR-safety contract
  per-platform documented in
  `docs/reference/platform-sync-abi.md` (new).

## Notes

- The `set_wake_callback` ABI doesn't change. Backends keep
  filling NULL when they have no async notify path; this phase
  only changes the executor side's wake primitive.
- `nros_platform_condvar_*` (the existing pthread-shaped ABI)
  stays for zenoh-pico's Zephyr interop. No callers migrate;
  only `Executor` does, and it moves to `nros_platform_wake_*`.
- Binary semaphore was chosen over condvar+mutex because the
  executor's wake_flag+wake_cv pair already collapses to
  "wake-with-flag" semantics. A semaphore is a smaller, more
  ISR-friendly primitive on every supported RTOS.
- Phase 124.B.7 specced ISR-safe condvar signaling; the wake
  primitive inherits that contract directly via its
  `signal_from_isr` slot.
