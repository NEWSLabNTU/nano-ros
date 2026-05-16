# Phase 129 — Platform-Native Wake Primitive

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

```
Executor::spin_once
  ├── PlatformCondvar.wait_until(deadline_ms)   ← no std dependency
  ├── session.drive_io(...)
  └── arena dispatch

nros-platform-* (per-platform)
  ├── nros_platform_condvar_init
  ├── nros_platform_condvar_wait_until
  ├── nros_platform_condvar_signal
  └── nros_platform_condvar_signal_from_isr     ← Phase 124.B.7 hook
```

Backend obligation unchanged. `set_wake_callback` installs a runtime
closure that writes `wake_flag = true` then calls
`nros_platform_condvar_signal{,_from_isr}` on the executor's
condvar handle. Spin loop blocks in `PlatformCondvar.wait_until`
and wakes on either:
- backend wake-callback firing (event-driven path), or
- the wall-clock deadline expiring (poll-on-timer fallback for
  backends with NULL callback).

## Work Items

### 129.1 — Zephyr `k_condvar` platform impl

**Files**
- `packages/core/nros-platform-zephyr/src/platform.c`

Replace the `CONFIG_POSIX_API` branch of `nros_platform_condvar_*`
with a `k_condvar_*` + `k_mutex_*` implementation that does not
depend on Zephyr's libc pthread shim. Honors `wait_until(abstime_ms)`
via `K_MSEC(remaining)` and returns `1` on timeout, `0` on signal,
`-1` on error (existing ABI). Provide an ISR-safe
`nros_platform_condvar_signal_from_isr` via the documented
`k_condvar_signal` ISR contract (or a `k_sem_give` fallback if the
running kernel build rejects ISR-context `k_condvar_signal`).

The non-`CONFIG_POSIX_API` stub branch (currently returns `-1`)
becomes the same `k_condvar_*` path — Zephyr ships `k_condvar`
unconditionally.

### 129.2 — Rust `PlatformCondvar` wrapper

**Files**
- `packages/core/nros-platform/src/sync.rs` (new)
- `packages/core/nros-platform/src/lib.rs` (re-export)
- `packages/core/nros-platform-cffi/src/lib.rs` (FFI surface)

`no_std`-safe Rust wrapper around `nros_platform_condvar_*` +
`nros_platform_mutex_*`. Surface mirrors `std::sync::Condvar` /
`Mutex` enough for `Executor` to drop in:

```rust
pub struct PlatformCondvar { /* opaque storage + init flag */ }
pub struct PlatformMutex   { /* opaque storage + init flag */ }

impl PlatformCondvar {
    pub fn new() -> Self;
    pub fn wait_until(&self, mu: &PlatformMutex, deadline_ms: u64) -> WakeReason;
    pub fn signal(&self);
    pub fn signal_from_isr(&self);
}
```

Storage sized via the existing platform-cffi build-time probe
(same pattern as `EXECUTOR_OPAQUE_U64S`).

### 129.3 — Executor swap

**Files**
- `packages/core/nros-node/src/executor/spin.rs`
- `packages/core/nros-node/src/executor/types.rs`
- `packages/core/nros-node/src/executor/mod.rs`

Replace `wake_cv: std::sync::Arc<std::sync::Condvar>` +
`wake_mu: std::sync::Arc<std::sync::Mutex<()>>` with
`wake_cv: PlatformCondvar` + `wake_mu: PlatformMutex`.
Cv-wait branch becomes:

```rust
#[cfg(feature = "rmw-cffi")]
if !was_woken {
    let deadline = now_ms() + timeout_ms as u64;
    let _ = self.wake_cv.wait_until(&self.wake_mu, deadline);
}
```

Gate is `feature = "rmw-cffi"` only — no platform-specific cfg.
`primary_drive_timeout_ms = 0` for any `rmw-cffi` build (drive_io
is non-blocking because cv-wait above already burned the timeout).

### 129.4 — Revert Phase 127.C.4 expedient gate

**Files**
- `packages/core/nros-node/src/executor/spin.rs`

Drop the `not(feature = "platform-zephyr")` gate added in `ba76394b`.
Drop the `any(not(feature = "std"), feature = "platform-zephyr")`
gate on `primary_drive_timeout_ms`. Single std cv-wait path again.

Verify `nros_cpp_spin_once` still compiles after the revert —
it already routes through `executor.spin_once` and needs no further
change.

### 129.5 — Other-platform parity check

**Files**
- `packages/core/nros-platform-freertos/src/platform.c`
- `packages/core/nros-platform-nuttx/src/platform.c`
- `packages/core/nros-platform-threadx/src/platform.c`
- `packages/core/nros-platform-posix/src/platform.c`

Confirm each platform's `nros_platform_condvar_*` impl honors
`wait_until` deadline and that `signal_from_isr` is ISR-safe per
the platform spec. POSIX (`pthread_cond_timedwait` against
`CLOCK_MONOTONIC`) already works. RTOS impls should already exist
from Phase 121 — audit and document the ISR-safety claim per
platform.

## Acceptance

- [ ] 129.1: Zephyr native_sim and `qemu_cortex_a9` boot through
  the new `k_condvar` impl without regressing 127.C.3 DDS pass.
- [ ] 129.2: `PlatformCondvar` compiles `no_std` on every
  supported platform (POSIX/Zephyr/FreeRTOS/NuttX/ThreadX/bare-metal).
- [ ] 129.3: `cargo test -p nros-node --lib --features rmw-cffi`
  131/131 passes — no behavior change on POSIX hosts.
- [ ] 129.4: With the expedient gate reverted,
  `test_zephyr_xrce_cpp_service_e2e` and
  `test_zephyr_xrce_cpp_action_e2e` pass under
  `just zephyr build-fixtures && just zephyr test --no-capture`.
- [ ] 129.4: Phase 127.C.4 is closeable on Zephyr without the
  per-platform cfg gate.
- [ ] 129.5: Each platform's condvar ISR-safety is documented in
  `docs/reference/platform-sync-abi.md` (new).

## Notes

- The `set_wake_callback` ABI doesn't change. Backends keep
  filling NULL when they have no async notify path; this phase
  only changes the executor side's wake primitive.
- POSIX path keeps `pthread_cond_timedwait` against
  `CLOCK_MONOTONIC` — already correct on Linux/macOS.
- ISR-safe signal on Zephyr is the one open spec question.
  `k_condvar_signal` is documented as thread-context on newer
  kernels; if the integration tests trip that, the impl falls
  back to `k_sem_give` for the ISR variant (also documented
  ISR-safe).
- Phase 124.B.7 already specced the ISR contract for
  `nros_platform_condvar_signal_from_isr` — this phase honors it
  per-platform rather than papering over with `signal()` on
  Zephyr.
