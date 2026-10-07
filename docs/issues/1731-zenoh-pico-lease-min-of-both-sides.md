---
id: 1731
title: "zenoh-pico uses min(peer lease, own lease) both to expire the peer and
  to pace its own keep-alives, where zenoh gives each side's lease to the
  other side"
status: open
type: limitation
area: [zenoh]
severity: low
found: 2026-09-29
related: [phase-474, issue-1574]
---

## What zenoh-pico does

`_z_unicast_handshake_open` (client) and `_z_unicast_handshake_listen`
(`src/transport/unicast/transport.c`, the pinned fork) both set

    param->_lease = min(peer's OPEN lease, Z_TRANSPORT_LEASE)

and the lease task (`src/transport/unicast/lease.c`) uses that one number
twice: it closes the session when the PEER has been silent for `_lease`, and
it sends its own keep-alives every `_lease / Z_TRANSPORT_LEASE_EXPIRE_FACTOR`.

In zenoh's semantics a lease is what a node ANNOUNCES: "if you hear nothing
from me for this long, consider me gone". A node therefore expires its peer on
the PEER's lease and paces its own keep-alives on its OWN lease. With the
min, a node whose own lease is shorter than the peer's expires a peer that is
keeping its word.

## Where it bit

The safety island (phase8-W8a, W15): a Zephyr image with the old 10 s
default against a stock `rmw_zenohd` (`lease: 60000`, `keep_alive: 2`, so a
router keep-alive every 30 s on an idle link). zenoh-pico judged router
silence against min(60 s, 10 s) = 10 s, decided CLOSE (reason 5, EXPIRED)
about 20 s after OPEN, and the CLOSE left with the next byte the router sent
-- which is why a host peer's join looked like the trigger.

## Why nano-ros leaves it (the answer for now)

Issue 1574 moved the Zephyr default `CONFIG_NROS_ZENOH_LEASE_MS` to 60000,
equal to `rmw_zenohd`'s, so the min is the router's lease and the deviation
has no effect against a stock router. Measured on main for phase-474 T1
(2026-10-07): the native_sim `c/talker` (zenoh, default lease 60000) against
a stock Humble `rmw_zenohd`, RUST_LOG=zenoh_transport=debug: one client
transport opened at +0 s, a host peer (`ros2 topic echo`) joined at +75 s and
left at +115 s, and the image's transport stayed open until the image was
stopped at +160 s -- no CLOSE, no re-INIT, no "Closing session because it has
expired" on the console. The island's phase8-W8a note is answered by that run
for the plain-router case on native_sim; the island's own QEMU image was not
rerun (QEMU's `guestfwd` also gives one host TCP connection for QEMU's
lifetime, so a QEMU lane can never show a RECONNECT, only the absence of a
drop).

## What a fix would be

In the fork: keep two numbers, `_lease_rx` (the peer's announced lease, used
to expire it) and `_lease_tx` (our own, used to pace keep-alives), instead of
their min. That is an upstream eclipse-zenoh/zenoh-pico change; it matters
only to an image that states a lease shorter than its router's. Until then
the rule is: do not state `CONFIG_NROS_ZENOH_LEASE_MS` below the router's
lease.
