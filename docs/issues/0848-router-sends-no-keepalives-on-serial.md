---
id: 848
title: "rmw_zenohd stops transmitting to a serial peer after the handshake — no
  keepalives, so zenoh-pico expires the session at 2 x lease"
status: open
type: bug
area: rmw
related: [issue-0839, issue-0821]
---

## Problem

Against the router config this repo ships
(`experiments/serial-interop/router-serial.json5`, `lease: 60000` /
`keep_alive: 6`, i.e. one keepalive every 10 s), the router transmits **twice**
to the board and then goes silent for the rest of the session. Captured at
`RUST_LOG=zenoh_transport=trace`, action-server image on
mr_canhubk3/s32k344 over serial:

```
15:51:12.408  New transport opened                          <- serial link up
15:51:12.585  tx: Scheduled NetworkMessageRef { Declare .. } <- 1st transmit
15:51:12.617  tx: Scheduled NetworkMessageRef { Declare .. } <- 2nd, and last
15:51:19.095  rx: TransportMessage { KeepAlive }             <- the BOARD's
15:51:22.439  rx: TransportMessage { KeepAlive }
15:51:25.766  rx: TransportMessage { KeepAlive }
15:51:29.110  rx: TransportMessage { KeepAlive }
15:51:32.437  rx: TransportMessage { KeepAlive }
15:51:32.629  rx: TransportMessage { Close }                 <- board gives up
```

Counted over the whole 20.2 s session:

| | count |
| --- | --- |
| router transmits (`universal::tx`) | **2**, both in the first 210 ms |
| router keepalives **sent** | **0** |
| board keepalives **received** by the router | 5, every 3.33 s, on schedule |

The board is behaving correctly. It keepalives on cadence, hears nothing for
two lease periods, and closes with `_Z_CLOSE_EXPIRED`. There is nothing
arriving for it to be starved of.

## This corrects an earlier claim

[Issue 0839](0839-action-image-session-expires-every-20s.md) records the router
keepalive cadence as "verified on the wire at 10.0 s" and therefore ruled out.
**That measurement was the TALKER image**, not this one, and it does not
generalise. For the action image the router emits no keepalives at all. 0839
has been corrected.

## Not a config difference

Same router, same config file, same board, same transport, same baud. A
**talker** holds its session through a five-minute soak with `closed=0` and
`ros2 topic hz` steady at 1.99 Hz. Only the guest image differs.

## Where the mechanism lives

zenoh's keepalive is emitted by the transport's TX task when the link has been
idle for the keepalive period
(`io/zenoh-transport/src/unicast/universal/link.rs`, read while chasing
[issue 0821](archived/0821-zenoh-pico-faults-at-lease-expiry-on-zephyr.md)):

```rust
_ = keep_alive_tracker.wait_if(write_priority.unwrap_or(Priority::Control) == Priority::Control) => {
    let message: TransportMessage = KeepAlive.into();
    link.send(&message, Some(Priority::Control)).await?;
}
```

`keep_alive_tracker.reset()` is called on every batch actually written, and the
tracker's timeout is driven by a background task spawned in
`TimeoutTracker::new`. Serial does not override `supports_priorities()`, so it
takes the single-`write_loop(None, ..)` path where the keepalive arm IS
enabled — which is why this is a defect rather than a documented limitation.

**Not established:** why the arm never fires here when it fires for the talker
on the identical link. The two Declares at t+0.2 s are the last thing the tx
task ever schedules.

## Suggested next step

Raise `RUST_LOG` on `zenoh_transport::unicast::universal::link` specifically
and compare a talker session against an action session on the same router
process — the question is narrow: does `keep_alive_tracker` fire and the send
fail, or does it never fire at all. That distinguishes a stalled tx task from a
tracker that is being reset by something.

Worth checking too whether the queryables the action image declares (three,
versus the talker's zero) leave a pipeline in a state the tx task treats as
non-idle.

## Impact

Blocks [issue 0839](0839-action-image-session-expires-every-20s.md), and with
it any action over serial: the session cannot outlive `2 x
Z_TRANSPORT_LEASE`. `CONFIG_NROS_ZENOH_LEASE_MS` mitigates by moving the
deadline, but the link is still one-way after the handshake.
