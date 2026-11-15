---
id: 2596
title: "zenoh-pico `TX_SPLIT_LOCK` is defeated: the session transport mutex holds a publisher append for the whole detached batch write"
status: resolved
type: bug
area: [rmw, zenoh, zephyr]
severity: medium
found: 2026-11-15
related: [rfc-0110, 2575, 0899]
resolved: 2026-11-15
resolved_in: "zenoh-pico fork 19a47ae7 (nano-ros-1.10.1)"
---

**Standalone.** No active phase owns the zenoh-pico TX path. The fix goes on
the zenoh-pico fork's patch line (`packages/rmw/zenoh/zpico-sys/zenoh-pico`,
branch `nano-ros-1.10.1`). Found while measuring issue 2575.

## What happens

`Z_FEATURE_TX_SPLIT_LOCK` (phase-282) exists so that a slow socket write does
not stall publishers. The batch flush steals the pending batch under
`_mutex_tx`, releases `_mutex_tx`, and writes under `_mutex_link_tx` only, so
a publisher can append to the fresh batch while the write is in flight.

Issue 0899's fix made `_z_send_n_msg` and `_z_send_n_batch` hold the session's
`_mutex_transport` (recursive) for the WHOLE send. That keeps the transport
alive against a teardown, but it also covers the detached write. A publisher's
append, `_z_send_n_msg`, waits for that mutex, so it waits out the whole write,
exactly as if split lock were off.

It is also why issue 2575's feared two-writer interleave could not happen
through these entry points.

## Measured (fork at fb5f5cdc + 2575's fix, host)

A fake datagram link whose write takes 50 ms. Batching and split lock are on,
and a thread runs one `_z_send_n_batch` (the detached write). While that write
is in progress, the main thread times one batched `_z_send_n_msg`:

| tree | append during a 50 ms detached write (5 rounds) |
| --- | --- |
| as pinned | 50, 50, 49, 50, 49 ms |
| control: `_z_send_n_batch` without the session-mutex hold | 0, 0, 0, 0, 0 ms |

The control is the measurement's cause, not a fix: removing the hold reopens
issue 0899.

## Consequence

Any image with `TX_BATCH` + `TX_SPLIT_LOCK` pays the socket wait on the
publisher again: the 2.5x streaming cap phase-282 measured and removed. The
S32K344 safety island runs that config (`CONFIG_NROS_ZENOH_TX_SPLIT_LOCK=y`
with `TX_BATCH`); not yet measured on the board.

## Fix shape (approved by the user 2026-11-15, with four conditions)

Turn the lifetime guard from an exclusive hold into a use count. Senders are
"readers" of the transport and the teardown is the one "writer":

- enter: lock `_mutex_transport`; if `_tp_ready` is false, unlock and fail
  (as now); else `_tp_users++`; unlock.
- exit: lock; `_tp_users--`; if 0, signal `_tp_users_cv`; unlock.
- teardown (`_zp_unicast_failed` and the reconnect): lock; `_tp_ready = false`;
  wait on `_tp_users_cv` until `_tp_users == 0`; free; unlock.

The send itself then runs under `_mutex_tx` / `_mutex_link_tx` alone, as
phase-282 designed, and a teardown still cannot free under a sender. The
reconnect keeps failing sends fast, because `_tp_ready` is false while it
holds the mutex.

## How to verify

- The measurement above: the append must take well under the write's duration
  with the session guard in place.
- 0899's own reproduction must still pass, i.e. a teardown racing a publisher
  must not use-after-free.

## Resolution (2026-11-15)

Fixed on the fork's patch line, 19a47ae7, in the shape above:
`_z_session_transport_use_begin` / `use_end` in `session/utils.c`, used by
`_z_send_n_msg` and `_z_send_n_batch` in place of the `_mutex_transport` hold,
and the waiting `_z_session_transport_teardown`. How each of the user's
conditions is met:

1. **The wait releases the lock senders need.** It is a condvar on
   `_mutex_tp_users`, a plain leaf mutex: the one a sender takes to leave.
   It never waits on the recursive `_mutex_transport`, and it is a loop, so
   spurious wakes are harmless.
2. **The teardown holds no tx or link mutex while it waits.** Its callers
   are the unicast lease (`_zp_unicast_failed`), the multicast lease, the
   reconnect replay and `_z_session_clear`. Each holds `_mutex_transport`
   only. A send needs neither that nor anything else the teardown holds.
3. **Every port reaches the condvar through one path.** `_z_condvar_*` is
   the zpico shim over `nros_platform_condvar_*`, and `_z_condvar_wait` is
   its bounded-slice wrapper over `nros_platform_condvar_wait_until`
   (issue 1196).

   | port | condvar wait | measured |
   | --- | --- | --- |
   | POSIX (host) | `nros-platform-posix` pthread condvar | host ASan tests below |
   | FreeRTOS | `nros-platform-freertos` | QEMU mps2-an385 pubsub, below |
   | NuttX | `nuttx_clock.c`: `_z_condvar_wait_until` over `nros_platform_condvar_wait_until` | QEMU armv7a pubsub, below |
   | Zephyr, ThreadX | each port's `nros_platform_condvar_wait_until` | not run here |

   NuttX needs no counting-semaphore fallback. The hang behind its
   `sem_timedwait` spin wake (Phase 55.12) was in that one wake path. The
   condvar path above is the one zenoh-pico 1.10's own executor waits on for
   every wake-up, and the pubsub run measures it.
4. **A link write that never returns makes the teardown wait for ever.**
   The send keeps its use, so the teardown never sees the count reach zero.
   That is not a regression: before this fix the same send held
   `_mutex_transport`, and the teardown waited for ever to take it.

### Measured

The fork test `tests/z_transport_use_test.c`, under ASan, with split lock:

| case | result |
| --- | --- |
| An append during a 50 ms detached write | 0 ms (was 50 ms) |
| A teardown while a send is mid-write | waits for it (returned after 99 ms, against a 100 ms write), then a send is refused |
| 200 teardown/re-open cycles while 4 threads send small and fragmenting messages, express and batched (60 s watchdog) | no use-after-free, no hang, no use left over |
| Negative control: the teardown's wait removed | ASan SEGV in `_z_transport_tx_send_fragment_inner` on the freed transport |

On target (QEMU, `rtos_e2e`), each cell run twice:

| cell | result | image carries `use_begin` |
| --- | --- | --- |
| FreeRTOS C pubsub (issue 0899's original reproduction) | 70/70 both runs, 2 router sessions | yes |
| NuttX Rust pubsub | 70/70 both runs, 2 router sessions | yes |

The S32K344 safety island runs this config (`TX_BATCH` +
`CONFIG_NROS_ZENOH_TX_SPLIT_LOCK=y`). An on-board check is to be batched to
the board's owner.

## On the S32K344 board (2026-11-15)

The safety island's own config, run on the CANHUBK344 by the board's owner:
`TX_BATCH` + `CONFIG_NROS_ZENOH_TX_SPLIT_LOCK=y`, serial at 921600 baud, a
4 KB UART ring, and four 80 s runs each of zenoh-pico 19a47ae7 (this fix)
and 01f5761a (before it).

| | before (01f5761a) | after (19a47ae7) |
| --- | --- | --- |
| stall, reset or fault | none | none |
| samples lost | 0 | 0 |
| island topics at their rate | yes | yes |
| publish p99 | ~53–58 ms | ~53–58 ms |
| publish p50, uncontended | 148 µs | 162 µs |

The fix is safe on the board, and it buys nothing on this link. A publish
there does not wait on the detached batch write. It waits about 58 ms on a
different hold of `_mutex_tx`: the fragment train, plus the inline drain of a
parked overflow, both on issue 2575's path, and this fix leaves both alone.
With a 4 KB ring the detached write is mostly a copy into the ring, so there
was little for split lock to take off the publishers' path anyway. The cost
is the use count's two lock round trips: +14 µs on an uncontended publish.

That hold is now [issue 2614](../2614-zpico-fragment-train-holds-tx-mutex.md).
The run logs are on the board owner's host, at
`/mnt/mx500/aeon/worktrees/k3937-logs`.
