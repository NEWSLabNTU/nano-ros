---
id: 906
title: "Every zenoh-pico session drops and rebuilds every ~20 s — the ROS router sends it no KeepAlive"
status: open
type: bug
area: rmw-zenoh, interop
related: [issue-0899, rfc-0075]
---

## What happens

A nano-ros zenoh-pico client connected to `rmw_zenohd` closes its session and
reconnects roughly every twenty seconds, forever, with no error reported to the
application. It is not load-dependent, not platform-dependent, and not
provoked by anything the app does.

`tshark` on the loopback side of the link, one 45 s run of the FreeRTOS
`c_talker` under QEMU (slirp forwards to the host router), payload frames only:

    router -> guest, tcp.len>0:   0.004 s  76 B
                                  0.006 s   8 B
                                  0.011 s  12 B
                                 19.522 s  76 B   <- a NEW handshake
                                 19.522 s   8 B
                                 19.523 s  12 B
                                 40.086 s  76 B   <- and another
    guest -> router, tcp.len>0:  40 frames (the 1 Hz publishes)

Three frames in the first twelve milliseconds, then silence until the session
has already been declared dead and rebuilt.

## Why

`_zp_unicast_lease_task` (`src/transport/unicast/lease.c`) closes a CLIENT
session when `curr_peer->common._received` was false for one whole lease
period. Probed on the expiry branch, that is exactly what fires:

    !!! LEASE-EXPIRED nothing received in 10000ms

`_received` is set by the read task on each RX batch. A probe there shows it
running **once** per session — at open — and then blocking for the full ten
seconds, which the capture above confirms is honest: there is nothing to read.

The talker never sends a keepalive either, and cannot: the lease task only
sends one `if (!ztu->_common._transmitted)`, and a 1 Hz publisher has
transmitted every time the check comes round.

## It is NOT a FreeRTOS or an embedded problem

The same capture against the NATIVE Linux `examples/native/c/talker`, same
router, same port:

    0.000 s 76 B / 0.000 s 8 B / 0.000 s 12 B / 20.075 s 76 B ...

Identical. Every zenoh-pico client we ship behaves this way against the ROS
router. Native survives it silently — the reconnect succeeds and publishing
continues — which is why it has gone unnoticed.

## Why it matters even though "it reconnects"

* Each cycle drops and re-declares every publisher, subscriber and queryable.
  Anything a peer had matched is unmatched and rematched on a 20 s beat.
* It is the trigger for [[issue-0899]]: on FreeRTOS the teardown races the
  publishing task and the image asserts. 0899's own defect is the unsynchronised
  teardown, but this issue is what fires it, over and over.
* A session that dies on a timer is a strong candidate for interop flakiness
  reported elsewhere and attributed to discovery or to QEMU load.

## What to establish first

Whose job the keepalive is, on this pairing:

* If the ROUTER is expected to send periodic KeepAlive to an idle client, then
  `rmw_zenohd` not doing so is the defect, and the pin/version matters
  (RFC-0075 — the router is whatever ROS ships, so this can move under us).
* If the CLIENT is expected to keep its own lease refreshed, then zenoh-pico's
  `if (!_transmitted)` suppression is wrong for a publisher: transmitting does
  not prove the peer is alive, which is the entire point of a lease.

Decide that before writing a fix; the two answers land in different repos.

## Acceptance

* A zenoh-pico client publishing at 1 Hz against `rmw_zenohd` holds ONE session
  for at least five lease periods, proven by a payload-only packet capture
  showing no second handshake — not by "messages still arrive".
* Whichever side owns the keepalive is named in the RMW notes, so the next
  person does not have to re-derive it from a capture.
