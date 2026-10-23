---
id: 2001
title: "A zenoh-pico publisher drops a put whenever another thread holds the TX lock, RELIABLE QoS or not"
status: open
type: bug
area: [rmw-zenoh, qos]
severity: medium
found: 2026-10-23
related: [1980, 0145, phase-279]
---

## Measured

`zpico_declare_publisher` (`packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c`)
sets `congestion_control = Z_CONGESTION_CONTROL_BLOCK` only under
`ZPICO_TX_BATCH == 1`. Every other publisher keeps zenoh-pico's default,
`Z_CONGESTION_CONTROL_DROP`, whatever the endpoint's QoS reliability says.

Under DROP, `_z_transport_tx_send_n_msg`
(`zenoh-pico/src/transport/common/tx.c`) calls
`_z_transport_tx_mutex_lock(ztc, cong_ctrl == Z_CONGESTION_CONTROL_BLOCK)`,
i.e. a TRY-lock. When another thread holds the transport TX mutex, the put is
discarded ("Dropping zenoh message because of congestion control", `_Z_INFO`
only) and `z_publisher_put` returns an error.

The other holder needs no congestion at all. The lease task's keep-alive, a
service reply, or a second publisher on the same session is enough, and on a
loaded host that holder can be descheduled mid-write, which widens the window.
Issue 1980 measured it: 1 of 25 runs of
`an_arrival_racing_a_drain_is_never_left_undispatched` under `2 x nproc` busy
loops failed `publish_raw` with exactly this path.

## Why it matters

- **It is not what "DROP" means in zenoh.** Full zenoh's DROP discards a
  message when the TX QUEUE is full. zenoh-pico has no queue on this path, so
  its DROP discards on mere lock contention, between two threads that would
  each have finished in microseconds.
- **A RELIABLE publisher drops at the first hop.** ROS's RELIABLE promises
  retransmission of a sample that was sent; a sample refused before it reached
  the wire is outside that promise and is never retried by anything below the
  application.
- **The caller cannot tell why.** `handles.rs` (`publish_raw` and siblings,
  `packages/core/nros-node/src/executor/handles.rs:184`, `:220`, `:976`) maps
  every backend error to `TransportError::PublishFailed`, so a contention drop
  and a dead session read the same. Issue 1980's test had to retry blindly.

## Options

1. **Map QoS reliability to congestion control at declare time**: RELIABLE ⇒
   `BLOCK`, BEST_EFFORT ⇒ `DROP`. Matches what `rmw_zenoh_cpp` does for
   RELIABLE + KEEP_ALL, and what `ZPICO_TX_BATCH` already chose for the same
   reason. Cost: a RELIABLE publish can now wait on the TX mutex, i.e. on a
   socket write by another thread. Needs a check that no caller publishes while
   holding a lock the TX holder needs (callback context on a single-threaded
   backend in particular).
2. **Keep DROP, surface it**: return a distinct error (`ZPICO_ERR_WOULD_BLOCK`
   or similar) up to a `TransportError::Busy`, so the caller can retry.
   Changes no timing, leaves every caller to write 1980's retry loop.
3. **Bounded BLOCK**: block up to a deadline, then drop. zenoh-pico's mutex
   API has no timed lock today, so this needs a fork patch.

Option 1 plus the distinct error from option 2 (for BEST_EFFORT) is the
recommended shape. Whichever is chosen, `PublishFailed` should stop absorbing
the backend's reason.

## Acceptance

- A RELIABLE publisher on a non-batching build does not fail a put because
  another thread holds the TX mutex (repro: 1980's test with its retry loop
  removed, `2 x nproc` load, 0 of 50 publish failures).
- The congestion-control choice per publisher is stated in one place and
  covered by a test, not implied by `ZPICO_TX_BATCH`.
