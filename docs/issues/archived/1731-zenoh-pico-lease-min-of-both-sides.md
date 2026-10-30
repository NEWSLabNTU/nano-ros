---
id: 1731
title: "zenoh-pico uses min(peer lease, own lease) both to expire the peer and
  to pace its own keep-alives, where zenoh gives each side's lease to the
  other side"
status: resolved
resolved: 2026-10-30
type: limitation
area: [zenoh]
severity: low
found: 2026-09-29
related: [phase-474, issue-1574]
resolved_in: "zenoh-pico fork nano-ros-1.10.1 3c5cbc47 + 8d5ac2fe"
---

**Resolved 2026-10-30** — see the last section. **Home: [phase-474](../../roadmap/phase-474-safety-island-board-findings.md)** — homed 2026-10-17; the phase doc already tracks it in its body.

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

## Resolution (2026-10-30)

Fixed in the pinned fork (`nano-ros-1.10.1`, `3c5cbc47` + `8d5ac2fe`), the
two-numbers fix above, generalised:

- **Unicast, client and peer.** The handshake keeps the peer's OPEN lease as
  announced and stores it on the peer (`_z_transport_peer_unicast_t._lease`);
  `_common._lease` is always OUR lease. The client lease task judges the router
  on the router's lease and the client keep-alive is paced on ours -- what zenoh
  does (`zenoh-transport/src/unicast/universal/transport.rs`, `add_link`:
  `start_rx(transport, other_lease)`, `start_tx(.., config.lease /
  config.keep_alive)`).
- **Peer mode and multicast** had the same rule in another shape: one lease task
  for all peers, waking on the shortest lease and dropping any peer silent for
  one period. Each peer's silence is now summed over whole silent periods and
  compared with that peer's own lease (`_z_transport_peer_common_lease_check`).
  Keep-alives to peers stay paced on min(ours, peers'), because a zenoh-pico peer
  without this change still judges us on that min -- extra traffic, never a drop.

Measured in the ros2 box, `z_sub` built from the fork with zenoh-pico's own
default `Z_TRANSPORT_LEASE` 10000, against a stock Humble `rmw_zenohd`:

| case | old pin `f90e7388` | fixed |
| --- | --- | --- |
| stock router (lease 60000, keep_alive 2), 75 s | 8 transports, one per ~10.2 s | 1 |
| router lease 60000 / keep_alive 4, its traffic to the client muted 30 s by a proxy | 5 connections | 1 (60 s not reached) |
| router lease 10000 / keep_alive 4, muted 30 s | 3 | 3 -- expired after 12.9 s of silence: a silent router is still dropped, on ITS lease |

`z_lease_check_test` (fork ctest) covers the summing rule; making it expire on
any silent period aborts it. The fork's 45 ctests pass.

What it changes for nano-ros: `CONFIG_NROS_ZENOH_LEASE_MS` / the cargo lane's
60 s are no longer load-bearing against a stock router -- a shorter lease costs
keep-alive traffic and buys the router a faster view of this image's death, and
no longer drops the session. The default stays 60 s (what every ROS 2 node on
that router announces). The pub/sub cell's session counting in `rtos_e2e.rs`
still guards session stability, but no longer detects a lease regressed below
the router's cadence, because that is no longer a failure.
