---
id: 2747
title: "A session on a custom link receives nothing: `_z_link_socket_recv_zbuf` routed it to the socket-shaped read"
status: resolved
type: bug
area: [rmw, zenoh, transport]
severity: high
found: 2026-11-20
resolved: 2026-11-20
resolved_in: "the change that filed it"
related: [issue-2749, phase-575, RFC-0134]
---

## What happened

`custom_transport_loopback` went red on tier 1: the listener exited before
`I heard:` appeared, printing only `Listener done`. On main, fresh fixtures
failed it 5 of 5 times solo. A bisect over 276 commits named fa2637fb4c
(PR #4233, two peers on IVC with no router). Its parent passes; it and every
later commit tested fail.

## Why

That commit moved the zenoh-pico pin to a fork patch that made IVC sessions
read. In `_z_link_socket_recv_zbuf` it replaced

```c
if (link->_type == _Z_LINK_TYPE_CUSTOM) return _z_link_recv_zbuf(link, zbf, NULL);
```

with a test that was meant to cover IVC, RPMsg and CAN too:

```c
if (_z_link_get_socket(link) == NULL) return _z_link_recv_zbuf(link, zbf, NULL);
```

`_z_link_get_socket` returns a NON-NULL stub for the custom link, deliberately,
because the unicast manager dereferences the result. So a custom link fell
through to its `_read_socket_f`, which returns 0, and a session on it never
received anything.

The handshake still completed, because it reads through a different entry
(`_read_exact_f`). That is why the existing `custom_transport` smoke test,
which only checks that the callbacks are reached during bring-up, stayed
green.

## Fix

- zenoh-pico (`nano-ros-1.10.1`): one predicate,
  `_z_link_reads_through_link`, decided by link TYPE (custom, IVC, RPMsg,
  CAN), used by `_z_link_socket_recv_zbuf`. Whether a link has a socket is a
  separate question, and `_z_link_get_socket` answers only that.
- Sweep: no other site routes a read by socket absence or by
  `_type == _Z_LINK_TYPE_CUSTOM`, in the fork or in the superproject.
- The 0-byte rule from the same change (a 0-byte datagram read means
  "nothing yet") does not reach the custom link. Its flow is STREAM, and the
  stream path already treats a short read as "come back later".
- The new pin also carries the fork's framed-link wait entry, whose RPMsg
  side (`_z_rpmsg_rx_wait`) had no definition outside draft PR #4383, so the
  IVC/RPMsg peer test failed to link. `zpico-link-rpmsg` now defines a
  minimal one that reports whether a frame is waiting and does not wait, the
  behaviour before that entry existed. #4383's board wait replaces it.

## Regression check

`nros-rmw-zenoh/tests/custom_link_round_trip.rs`, in the
`required-features-tests` lane (build tier, so `just ci gate` runs it):

- It calls the shim's `zpico_custom_link_recv_probe`. The probe opens a link
  on `custom/probe` and receives once through `_z_link_socket_recv_zbuf`,
  the exact function that regressed.
- It asserts the transport's `read` was reached and its bytes arrived.
- No session, peer or network.
- Measured: it passes with the fork commit and fails without it, with "the
  receive path never called the transport's read".

A session-level test with no router needs the custom link to support
point-to-point peer mode, which it does not. That is issue 2749.
