---
id: 2749
title: "The custom link is client-only, so no test can carry a session over it without a router"
status: open
type: limitation
area: [rmw, zenoh, transport, testing]
severity: low
found: 2026-11-20
related: [issue-2747, RFC-0134]
---

## What is missing

A custom link (`custom/…`, the user transport vtable) can only OPEN:
`_z_f_link_listen_custom` refuses ("v1: client-only"). A session over one
therefore needs a router on the far end. In the merge-gating lanes that means
none can run a session over a custom link, because those lanes have no router
and no fixtures.

Issue 2747 showed what that costs. The custom link's receive path broke, and
the only test that could see it was the tier-1 `custom_transport_loopback`,
which runs against `zenohd`. 2747 added a network-free check of the function
that regressed. A session-level check is still missing.

## Measured: a zenoh-pico peer on the far end does not work

The attempt was a custom-link client, bridged over loopback TCP to a
zenoh-pico peer in a child process. Two things stopped it:

- **Peer accept is not compiled in by default.** It needs
  `Z_FEATURE_UNICAST_PEER`, which only builds with a point-to-point link,
  plus `ZPICO_MULTICAST_TRANSPORT=1`.
- **It races by construction.** The peer's accept task polls every 1000 ms
  (`_zp_unicast_accept_task_fn`, `wake_up_after(1000)`). The client's
  handshake read gives each gap `Z_CONFIG_SOCKET_TIMEOUT`, 100 ms, and that
  timeout is not a knob on posix. A test transport that waited longer than it
  is asked to would break the vtable contract.

## Proposal (needs an RFC-0134 note and the user's ruling)

Give the custom link the point-to-point peer mode RFC-0134 gave IVC:

- listen opens the user transport, and the link reports `_is_single_peer`;
- the shim's point-to-point endpoint set gains `custom/`.

Two peers then exchange data over a socket pair with no router and no accept
poll, exactly as `point_to_point_peer.rs` does for IVC and RPMsg, in a lane
`ci gate` runs. This is a feature, not a fix: the vtable has no accept
callback today, so the note must say whether listen means "open and wait for
the first frame" or a new vtable entry. Adding a vtable entry would be an ABI
version bump.
