---
id: 1251
title: "A Cyclone peer for a QEMU guest cannot be loopback-pinned: the
  isolation profile `dds_isolation` writes and the NAT rewrite the guest
  needs are mutually exclusive"
status: resolved
type: tech-debt
area: testing, rmw
severity: medium
found: 2026-09-10
related: [issue-1009, issue-1137, phase-441, phase-480]
---

## What this is

Phase-441 W2 measured whether CycloneDDS 0.10.5 can discover across QEMU's
user-mode (slirp) networking. **It can** — the recipe and the evidence are in
[phase-441](../../roadmap/phase-441-rmw-on-target-verification.md) §W2. This issue
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

## Resolution

Resolved 2026-10-06 (phase-480 W6). Both halves of the ask landed: the profile,
and a refusal by name for a pair that does not use it.

**The third profile**, `nros_tests::dds_isolation::CycloneSlirpPair` — the
shape "What to do" above describes. Both halves carry one `<Discovery><Tag>`,
are unicast only (`AllowMulticast=false` plus `Peers`), and use fixed
participant indices (guest 0, host 1), so each side knows the other's DDSI
ports (`ddsi_unicast_ports`). The host selects ONE non-loopback interface
(`default_route_interface`, from `/proc/net/route`) and advertises the slirp
alias `10.0.2.2`. The guest advertises `127.0.0.1`, which is where its forwarded
ports land on the host. `qemu_hostfwd(guest_ip)` emits the two `hostfwd=udp:`
clauses. Isolation is by tag, not by interface: a participant without the tag
never matches.

**The refusal.** `cyclone_peer_config_uri(platform)` hands out the loopback
profile only when the nano side shares the host's network stack
(`shares_host_network_stack`: Linux, Zephyr native_sim, FreeRTOS POSIX, ThreadX
Linux, PX4 SITL). It refuses every QEMU guest and the FVP with a message naming
this issue and `CycloneSlirpPair`. The unit test
`no_runnable_cyclone_interop_cell_puts_its_nano_side_behind_slirp` holds
`interop::CELLS` to that rule. A runnable Cyclone cell on a slirp guest must
build the pair and be listed in `interop::SLIRP_PAIR_CELLS`, which is empty
today.

**Measured through libslirp itself.** `tests/cyclone_slirp_pair.rs` puts our
Cyclone backend's `ros2_pub` in a user and network namespace whose only way out
is `slirp4netns`, the same libslirp that QEMU's `-netdev user` uses: the
`10.0.2.2` host-loopback alias, and inbound traffic only through `hostfwd`. The
host side is a stock humble `ros2 topic echo` on `rmw_cyclonedds_cpp`.

| host profile | samples received |
| --- | --- |
| `CycloneSlirpPair`, same tag | delivered, 3 of 3 runs (about 36 s per run, all three cases) |
| `CycloneSlirpPair`, a different tag | 0: the isolation |
| the issue-1009 loopback profile | 0: the collision this issue names, now reproduced on libslirp rather than pasta |

Mutation: giving every pair the same tag makes the test fail with "a host
participant with a DIFFERENT tag received the guest's samples".

**Not measured:**

- A real QEMU guest. The RTOS IP stacks (lwIP, NetX Duo, Zephyr's native stack)
  and the embedded Cyclone build are still the open items phase-441 W2 lists.
  The guest here ran Linux's stack and the hosted backend. The NAT is the real
  libslirp, though, not an emulation of it.
- A genuinely foreign participant on another host. The isolation is measured
  against a same-host participant with a different tag. No cross-host LAN peer
  was run.
- No interop cell uses the pair yet. Prerequisites 1, 2 and 4 of phase-441 W2's
  "What this makes affordable" are still open.
