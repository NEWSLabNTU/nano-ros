---
id: 1822
title: "The zenoh-rs CAN link's per-message priority mapping is built but cannot be switched on: a QoS `Join` is 99 bytes against a 63-byte CAN FD MTU"
status: open
type: enhancement
area: [rmw, zenoh]
severity: low
found: 2026-10-12
related: [phase-378, phase-377, rfc-0080, rfc-0081, phase-474]
---

**Home: [phase-474](../roadmap/phase-474-safety-island-board-findings.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What is left

[phase-378](../roadmap/archived/phase-378-can-link-for-zenoh-rs.md) delivered a
CAN link for zenoh-rs (W0-W6) and was archived on 2026-10-12 with W7 open. W7
is the only remainder, and it is not phase work any more: it waits on a change
upstream of this repository.

**What W7 built** (on the fork's `feat/can-link` line, both the 1.8.0 and the
1.10.0 branches):

- `LinkMulticastTrait::write_all_with_priority`, defaulted to `write_all`, so no
  other link changes. The multicast TX task now passes the priority that
  `pipeline.pull()` already returns.
- An identifier layout with a `prio_bits` class field at the MSB end and the
  peer field below it. The class dominates arbitration, and peer identity is
  recovered by masking the class off. `prio_bits` defaults to 0, which is the
  wire zenoh-pico speaks.

**Why it cannot be switched on.** Priority on the wire only does something when
the transport keeps one queue per priority, which needs multicast QoS. With QoS
on, `Join` carries eight `PrioritySn` instead of one, and `Join` goes out as a
single datagram that is never fragmented:

| | bytes |
| --- | ---: |
| `Join` without QoS | 33 |
| `Join` with per-priority SNs | 99 |
| CAN FD MTU | 63 |

So enabling QoS kills the transmit task before the session starts. This is a
protocol limit, not a link limit. The test
`join_with_qos_does_not_fit_one_can_frame` fails if `Join` ever shrinks enough
to fit.

## What closes it

One of these, both above the link:

1. Upstream zenoh fragments `Join`, or adds a compact QoS extension, so that a
   QoS `Join` fits in one CAN FD frame. Then turn on QoS and
   `prio_bits > 0`.
2. Then take W7's third acceptance item, which has never been measured: an
   urgent message overtaking a bulk burst from the same peer. This needs real
   hardware, because `vcan` does not arbitrate.

No upstream PR has been opened for the link or for W7. Per the repo's fork
policy, the maintainer opens it.
