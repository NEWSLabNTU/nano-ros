---
id: 667
title: "`nros_platform_task_init` refused a stack request below the port's own
  minimum, so `Executor::signal_fd()` returned `NotInitialized` on every Linux host"
status: resolved
type: bug
severity: high
area: platform, build
related: [issue-0612, issue-0570, phase-359, phase-364]
---

## Symptom

`Executor::signal_fd()` fails with `NodeError::NotInitialized` on every Linux
host. Nothing else reports anything: the eventfd is created and closed again,
and the caller is told only that the capability is unavailable.

```
PROBE eventfd -> 5
PROBE task_storage size=8 align=8
PROBE task_init rc=-7          # NROS_PLATFORM_RET_INVALID
PROBE PlatformTask::spawn -> None
```

## Cause

`WakeSignalFd::new` asks for an 8192-byte stack:

```rust
// packages/core/nros-node/src/executor/spin.rs
// A read(2) loop and one atomic store — the smallest stack any port will
// honour is plenty.
8192,
```

The comment is the bug. glibc's `PTHREAD_STACK_MIN` on x86_64 is **16384**, so
`pthread_attr_setstacksize(&pattr, 8192)` fails, and the POSIX port turned that
into a permanent refusal:

```c
/* Below PTHREAD_STACK_MIN the call fails; treat that as the caller
 * asking for something impossible rather than a shortage. */
if (pthread_attr_setstacksize(&pattr, a->stack_bytes) != 0) {
    (void) pthread_attr_destroy(&pattr);
    return NROS_PLATFORM_RET_INVALID;
}
```

Asking for a SMALLER stack than the platform's minimum is not asking for
something impossible. It is asking for "small", and the platform's minimum is
the smallest small there is.

No portable caller can avoid this by picking a better number, either: the floor
is 16384 on glibc/x86_64 and 131072 on glibc/aarch64, and `TX_MINIMUM_STACK` /
`configMINIMAL_STACK_SIZE` are different again. Only the port knows its own
floor — the same reason the storage sizes are PROBED rather than mirrored
(issue 0570, phase-364 W3).

Introduced by phase-359 W10, which moved the signalfd worker from `std::thread`
(which clamps) to a platform task (which did not).

### The quiet form, on three other ports

FreeRTOS, ESP-IDF and ThreadX did not refuse a below-floor request — they passed
it through to `xTaskCreate` / `tx_thread_create`. That is the same defect in its
worse form: not a refusal you can see, but a task whose stack overflows later,
somewhere else. ThreadX's `tx_thread_create` answers `TX_SIZE_ERROR` under
`TX_MINIMUM_STACK`, which this port reported as a generic failure.

Zephyr is the exception and is already correct: it documents that
`stack_bytes` cannot be honoured through its POSIX layer and does not pretend
otherwise.

## Why nobody noticed

`signal-fd-wake` had exactly one test, and issue 0612 is the record of why it
could not run: the feature set that makes the wake path live is the one that
removes the only session `nros-node` could open. Moving that test to
`nros-tests` — where a registered backend and a router fixture exist — is what
executed the path for the first time, and it failed on the first run.

The other `PlatformTask::spawn` caller survived by one byte of luck:
`WORKER_STACK_BYTES = 16384`, exactly `PTHREAD_STACK_MIN` on this arch. On
glibc/aarch64 (floor 131072) the OS-priority worker pool would have failed the
same way.

## Fix (2026-08-18)

`stack_bytes` is documented as a **FLOOR, not an exact size**, in
`<nros/platform.h>`, and each port raises a below-floor request to its own
minimum instead of refusing or forwarding it:

| port | floor |
| --- | --- |
| POSIX | `PTHREAD_STACK_MIN` |
| FreeRTOS | `configMINIMAL_STACK_SIZE * sizeof(StackType_t)` |
| ESP-IDF | `configMINIMAL_STACK_SIZE * sizeof(StackType_t)` (its API takes bytes) |
| ThreadX | `TX_MINIMUM_STACK` — before the stack ALLOCATION, which is sized from the same value |
| Zephyr | n/a; documented as unappliable through its POSIX layer |

A port raises this value; it never lowers it, and never refuses for being too
small.

### Verified

`nros-tests::signal_fd_wake` — both cases, on the host that produced the trace
above. They assert a LOWER bound as well as an upper one, so a `spin_once` that
returns without ever blocking fails rather than passing:

```
spin_once returned after 58.979µs, before the eventfd write at +30 ms —
it did not block, so this run proves nothing about the wake path
```

(that line is the mutation check — `spin_once(0)` instead of `spin_once(1000ms)`
— not a real run.) With the clamp, 2 passed.

The three RTOS ports are compile-verified only; this host runs none of them, and
the change on each is a clamp against the port's own published constant.
