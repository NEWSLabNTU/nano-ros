---
id: 1251
title: "A Cyclone peer for a QEMU guest cannot be loopback-pinned: the
  isolation profile `dds_isolation` writes and the NAT rewrite the guest
  needs are mutually exclusive"
status: open
type: tech-debt
area: testing, rmw
severity: medium
found: 2026-09-10
related: [issue-1009, issue-1137, phase-441]
---

## What this is

Phase-441 W2 measured whether CycloneDDS 0.10.5 can discover across QEMU's
user-mode (slirp) networking. **It can** — the recipe and the evidence are in
[phase-441](../roadmap/phase-441-rmw-on-target-verification.md) §W2. This issue
records the one part of that answer that is a limitation of THIS tree rather
than of Cyclone: the host-side profile our test harness writes cannot be the
host side of such a pair, and nothing today says so.

## The collision

`nros_tests::dds_isolation` pins every Cyclone participant in an interop pair to
loopback (issue 1009; issue 1137 added our own half):

```xml
<AllowMulticast>false</AllowMulticast>
<Peers><Peer Address="127.0.0.1"/></Peers>
```

A participant inside a QEMU guest is not on the host's loopback. For it to be
reachable at all, two things must hold, and both were measured:

1. The guest must advertise an address the host can dial — its own `10.0.2.x`
   is unroutable from the host, so it needs
   `<General><ExternalNetworkAddress>` naming a forwarded host address, plus a
   matching `hostfwd=udp:` on the QEMU command line.
2. The host must advertise an address the GUEST can dial. `127.0.0.1` is not
   one: inside the guest that address is the guest's own loopback.

And `ExternalNetworkAddress` is refused when the only selected interface is
loopback — `q_init.c:398`, *"external network address specification only
supported if there is a unique non-loopback interface"* — so the host side
cannot both keep the loopback pin and rewrite what it advertises.

Measured, host pinned to loopback with everything else correct (phase-441 W2,
variant V4): both sides receive each other's SPDP and neither matches. The
guest's trace shows it addressing the host at **its own** address, because
Cyclone treats an advertised locator equal to its own external locator as
"same machine, use the real interface address"
(`q_ddsi_discovery.c:208-221`) — and with the host advertising `127.0.0.1`
while the guest's external address IS `127.0.0.1`, that rule fires:

```
guest: SPDP ST0 …:1c1 NEW (jerry-aeon/0.10.5/Linux/Linux)
        (data udp/10.0.2.15:17913@2 meta udp/10.0.2.15:17912@2)
```

`10.0.2.15` is the guest. Nothing is listening there; no match, no delivery,
no error.

## Why it matters beyond one cell

The loopback pin is not decoration — it is what keeps a foreign participant on
the LAN out of our interop measurements (1009 cost five wrong diagnoses of
issue 0741). Dropping it for an on-target cell reintroduces exactly that, so
"just take the pin off" is not the fix.

The measured alternative is `<Discovery><Tag>`: a string extension of the
domain id that both peers must match. Phase-441 W2 variants V7/V8 measured it —
matching tag delivers across the NAT, mismatched tag delivers nothing — so it
isolates a pair without constraining which interface either side uses. It is
also expressible on the guest, where a per-run random value is NOT (the guest's
config is baked into the image at build time), so a tag for an on-target cell
has to be a build-time constant rather than a per-process one, the way
`unique_ros_domain_id()` is.

## What to do

An on-target Cyclone cell (phase-441 W1's shape, Cyclone instead of zenoh)
needs a THIRD isolation profile beside the two `dds_isolation` writes today:

- host side: interface `auto` (not loopback), `AllowMulticast=false`,
  `ParticipantIndex` fixed, `<Peers>` naming the forwarded guest port, and a
  `<Tag>` shared with the image;
- guest side: baked, with `ExternalNetworkAddress` naming the forwarded
  address and `<Peers>` naming `10.0.2.2:<host meta port>`.

Until that exists, an interop cell that pairs a QEMU-guest Cyclone image with a
host `rmw_cyclonedds_cpp` peer through `dds_isolation` will report no discovery,
and the reason will look like the image rather than the profile.

## Not this issue

Not a Cyclone bug. Every behaviour above is documented or deliberate in the
pinned 0.10.5, and the pin does not move (issue 0507). Not `ROS_LOCALHOST_ONLY`
either — that reaches ROS processes only and was already measured useless here
(1009).
