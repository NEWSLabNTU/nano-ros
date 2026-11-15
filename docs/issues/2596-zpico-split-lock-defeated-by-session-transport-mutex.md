---
id: 2596
title: "zenoh-pico `TX_SPLIT_LOCK` is defeated: the session transport mutex holds a publisher append for the whole detached batch write"
status: open
type: bug
area: [rmw, zenoh, zephyr]
severity: medium
found: 2026-11-15
related: [rfc-0110, 2575, 0899]
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

## Fix shape (proposed, awaiting a ruling: it touches issue 0899's guarantee)

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
