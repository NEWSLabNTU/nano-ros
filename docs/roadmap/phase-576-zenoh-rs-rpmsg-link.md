# Phase 576 — An RPMsg link for zenoh-rs

**Status (2026-11-18). OPEN: no item started; W3 onward waits on phase-575 W4
step 1. Owner: unassigned.** Opened by the user's ruling of 2026-11-18: "Have
Zenoh-rs follow-up in a proper phase."

Each item is one PR, claimed with `just claim phase-576-wN` before it starts.

## Why

[Phase-575](phase-575-r5f-out-of-tree-board.md) W4 builds the embedded half of
an RPMsg link: a zenoh-pico `rpmsg/<endpoint>` link on the IVC link's pattern,
for real-time cores that talk over RPMsg. That half serves two topologies:

- **Core to core, no router** (two Cortex-R5F cores). Phase-575 finishes this.
- **Real-time core to Linux** (the vision island of
  [phase-574](phase-574-perception-large-messages.md)). The Linux end runs
  `rmw_zenohd` and ROS 2 nodes on zenoh-rs, which has no RPMsg link.

Until this phase lands, the Linux end has two workarounds, and neither is the
end state:

- **Where the kernel offers an RPMsg TTY** (NXP `imx_rpmsg_tty`), `rmw_zenohd`'s
  `serial/` link reaches it. Confirmed on 2026-11-17: Humble's
  `libzenohc.so` embeds `zenoh-link-serial`. The cost is the serial link's own
  framing on a channel that is already message-framed and reliable.
- **Where it offers only `rpmsg_char`** (TI), a small bridge process copies
  between `/dev/rpmsgN` and TCP into the router. The cost is one more process
  and one more copy on every message. The RP-TL measurements (Conus et al.,
  2026) put the userspace RPMsg path 34–55 % below the kernel path in round
  trip, so the extra hop is the part worth removing.

This phase gives zenoh-rs a native `rpmsg/` link, so `rmw_zenohd` and every
`rmw_zenoh_cpp` application can open the channel directly.

## Shape

The same shape as the CAN link of
[phase-378](archived/phase-378-can-link-for-zenoh-rs.md) and RFC-0081, because
the constraint is the same: zenoh-rs has no out-of-tree link plugins, and its
`LinkKind` is a closed enum. The link is self-contained in our fork of
`eclipse-zenoh/zenoh`, shaped as an upstream pull request, behind a
`transport_rpmsg` feature that defaults off. nano-ros gains no Rust transport
code.

```
zenoh:   io/zenoh-links/zenoh-link-rpmsg/src/frame.rs    the wire format, no I/O
         io/zenoh-links/zenoh-link-rpmsg/src/sys.rs      rpmsg_char binding, Linux only
         io/zenoh-links/zenoh-link-rpmsg/src/unicast.rs  LinkUnicastRpmsg + manager
         io/zenoh-links/zenoh-link-rpmsg/src/lib.rs      prefix, config keys, inspector
         io/zenoh-link/src/lib.rs                        LinkKind::Rpmsg registration
         zenoh/Cargo.toml, io/zenoh-transport/Cargo.toml transport_rpmsg pass-through
zenoh-c: Cargo.toml + CMake                              transport_rpmsg pass-through
```

Two differences from CAN make this link simpler:

- **Unicast, not multicast.** RPMsg joins exactly two endpoints. CAN had to be
  a multicast link, and phase-378 found that zenoh 1.10's router does not
  route to multicast groups, and that a multicast face receives every route.
  An RPMsg link is an ordinary unicast link, which router, client and peer
  modes all handle, so `rmw_zenohd` can listen on `rpmsg/...` directly.
- **Reliable and ordered.** The vrings lose nothing (a sender waits for a free
  buffer) and keep order, so the link is declared reliable and the transport
  adds no recovery. The datagram MTU is the RPMsg buffer payload, typically
  496 B, and zenoh's fragmentation carries anything larger.

Delivery to ROS 2 follows phase-378: stock `rmw_zenoh_cpp` loads
`libzenohc.so` by soname with no RPATH, so substituting a `libzenohc.so` built
from the fork with `transport_rpmsg` reaches every ROS 2 application with no ROS
rebuild. That stays true until upstream accepts the link.

## Work items

### W0 — Design

- **Owner:** unassigned.
- **Status:** OPEN
- An RFC for the zenoh-rs half, beside RFC-0081, covering:
  - the endpoint grammar, which must match the zenoh-pico link's;
  - the frame format;
  - the MTU;
  - the reliability declaration;
  - the rpmsg_char API variants across kernels and vendors: endpoint creation
    through `RPMSG_CREATE_EPT_IOCTL` on `/dev/rpmsg_ctrlN`, and the newer
    per-channel character devices;
  - how a test substitutes for the device.
- **Acceptance:** the RFC is accepted by the user, and phase-575 W4's link uses
  the same endpoint grammar and frame format.

### W1 — Frame format and golden frames

- **Owner:** unassigned.
- **Status:** OPEN
- `frame.rs` encoding and decoding, with no I/O, unit-tested. Add golden-frame
  tests: byte-exact buffers produced by the zenoh-pico encoder from phase-575
  W4.
- **Acceptance:** the two implementations are checked as one wire format, in
  CI, with no device.

### W2 — Device binding and link

- **Owner:** unassigned.
- **Status:** OPEN
- `sys.rs`, the rpmsg_char binding (`AsyncFd` on the endpoint descriptor), and
  `unicast.rs`, the link and its manager. The device sits behind a trait, so a
  `SOCK_SEQPACKET` socketpair can stand in for it in tests.
- **Acceptance:** a Rust process opens an `rpmsg/...` link over the stand-in
  and moves datagrams in both directions.

### W3 — Registration and end to end

- **Owner:** unassigned.
- **Status:** OPEN (waits on phase-575 W4 step 1)
- `LinkKind::Rpmsg`, the manager arm, and the `transport_rpmsg` feature through
  `zenoh-link`, `zenoh-transport` and `zenoh`. Then, over the stand-in device:
  - two zenoh-rs sessions exchange pub/sub with a payload well above the MTU;
  - a zenoh-rs session and a zenoh-pico session (the phase-575 link, built for
    the host) exchange pub/sub in both directions.
- **Acceptance:** both exchanges pass, with the payload's content asserted.

### W4 — ROS 2 delivery

- **Owner:** unassigned.
- **Status:** OPEN
- The zenoh-c feature pass-through. Then a ROS 2 application with
  `RMW_IMPLEMENTATION=rmw_zenoh_cpp`, and `rmw_zenohd` listening on an
  `rpmsg/...` endpoint, both on the substituted `libzenohc.so`, exchange
  traffic with a nano-ros image.
- **Acceptance:** stock `rmw_zenoh_cpp` and stock `rmw_zenohd` carry traffic
  over the link with no ROS rebuild.

### W5 — On a real channel

- **Owner:** unassigned.
- **Status:** OPEN
- Run W4 over a real RPMsg channel: an emulator that models Linux plus a remote
  core if one exists by then (the QEMU i.MX 8M Plus Cortex-M7 AMP patch was
  unmerged as of 2026-11), otherwise a board from the user's project.
- Measure round-trip latency and throughput against the TTY and bridge
  workarounds, using phase-574 W4's medium-class load.
- **Acceptance:** the measurements, and a decision recorded here on whether
  the native link replaces both workarounds.

### W6 — Upstream contribution

- **Owner:** unassigned.
- **Status:** OPEN
- Offer the link to `eclipse-zenoh/zenoh` as a pull request against upstream
  `main`. This is a separate line from the fork's patch branch, which carries
  the link on the version ROS ships, and the two may differ in shape. Nothing
  in this phase waits for upstream.
- **Acceptance:** the pull request is open, or the reason it was not offered
  is recorded.

## Acceptance for the phase

The phase is complete when:

- stock `rmw_zenohd` and `rmw_zenoh_cpp`, on a `libzenohc.so` built with
  `transport_rpmsg`, carry traffic over an RPMsg channel to and from a nano-ros
  image (W4);
- the wire format is pinned by golden frames shared with the zenoh-pico link
  (W1);
- the link's latency and throughput are measured against the workarounds (W5).

## Scope

- **In scope:** the zenoh-rs link and its zenoh-c pass-through, in our fork,
  plus the tests and measurements above.
- **Out of scope:** the zenoh-pico link and the vendor RPMsg bindings
  (phase-575 W4 and the board package in the user's project), and any board.

## Related

- [Phase-575](phase-575-r5f-out-of-tree-board.md) W4: the zenoh-pico RPMsg link
  and router-less peering (RFC-0134).
- [Phase-574](phase-574-perception-large-messages.md): perception data and
  large messages, whose W5 measures the 496 B MTU.
- [Phase-378](archived/phase-378-can-link-for-zenoh-rs.md) and RFC-0081: the
  CAN link for zenoh-rs, whose shape this phase follows.

## Issues homed here

| issue | why it belongs here |
| --- | --- |
