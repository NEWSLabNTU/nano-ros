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

So enabling QoS kills the transmit task before the session starts. That was
first recorded as a protocol limit; it is not one — at an 8-bit frame SN the
same `Join` is at most 61 bytes, and what stops that being configured is an
upstream bug (measured below, 2026-10-30). The test
`join_with_qos_does_not_fit_one_can_frame` fails if `Join` ever shrinks enough
to fit.

## Measured 2026-10-30: the protocol has room, a config knob reaches it, and an upstream bug blocks the knob

The table above is right at the DEFAULT frame-SN resolution and wrong as a
statement about the protocol. A `PrioritySn` is two varints, and a varint's
width follows the SN's value: zenoh-transport seeds each multicast SN uniformly
in `0..=mask(resolution)`, so at the default 32 bits a seeded SN is a 4-byte
varint and sixteen of them are 64 bytes. At 8 bits each is at most 2 bytes.

Encoded with zenoh-codec 1.8.0, the field values the multicast TX task writes
(`next_sn = PrioritySn::DEFAULT` beside the QoS extension, `PatchType::CURRENT`,
batch 63 so the size byte is always present, a full 16-byte zid, lease 10 s),
SNs at the TOP of each range — the worst case:

| frame SN | no QoS | QoS, SNs at 0 | QoS, SNs at max |
| --- | ---: | ---: | ---: |
| 8 bit | 29 | 45 | **61** |
| 16 bit | 31 | 45 | 77 |
| 32 bit (default) | 35 | 45 | 109 |

So **a QoS `Join` fits one CAN FD frame at an 8-bit frame SN in every case**,
2 bytes under the MTU at worst; 16 bits never does. A shorter zid only adds
margin (8-bit, zid 4 B: 49). The 99 measured on the fork is the 32-bit row with
the SNs a real seed happened to draw.

The resolution is already a config key —
`transport/link/tx/sequence_number_resolution: "8bit"` (zenoh-config 1.8.0,
`LinkTxConf`) — so no protocol change is needed. **What blocks it is a bug in
zenoh-transport's multicast TX task, upstream and on every fork line**:

```rust
// io/zenoh-transport/src/multicast/link.rs, the periodic Join
let message: TransportMessage = Join {
    ...
    resolution: Resolution::default(),   // whatever the manager was configured with
```

while every receiver does

```rust
// io/zenoh-transport/src/multicast/rx.rs
if join.resolution != self.manager.config.resolution {
    tracing::debug!("Ignoring Join ... Unsupported SN resolution ...");
    return Ok(());
}
```

So two peers configured identically with ANY non-default resolution ignore each
other's every `Join` and never meet. The SN masking already uses the configured
resolution; only the advertisement is wrong.

**Fixed on a local fork commit, not pushed** (fork policy: the maintainer
pushes). `docs/issues/1822-multicast-join-advertises-resolution.patch` is
`git format-patch` of it, on top of `jerry73204/zenoh` `feat/can-link-ros`
`7645252b`: the Join advertises `manager.config.resolution`, carried in
`TransportLinkMulticastConfigUniversal`. Tests in the patch, run against
`zenoh-transport` with `test,transport_udp,transport_can`:

| test | without the fix | with it |
| --- | --- | --- |
| `multicast_resolution::multicast_peers_meet_at_default_sn_resolution` (rig control) | pass | pass |
| `multicast_resolution::multicast_peers_meet_at_8bit_sn_resolution` (two UDP multicast peers, 8 s deadline) | **FAIL**, never meet | pass |
| `multicast_can::join_with_qos_fits_one_can_frame_at_8bit_sn` (8-bit 61 B ≤ 63, 16-bit 77 B > 63) | — (codec only) | pass |
| `multicast_can::join_with_qos_does_not_fit_one_can_frame` (default resolution, unchanged) | pass | pass |
| `multicast_transport`, `multicast_compression` (existing) | pass | pass |

The `vcan0` tests stay ignored here: this host has no `vcan` interface and
loading the module needs root.

**zenoh-pico must agree.** A pico peer on the same bus rejects a `Join` whose
resolution is not its compile-time `Z_SN_RESOLUTION`
(`src/transport/multicast/rx.c`), so every pico peer needs
`Z_SN_RESOLUTION 0x00` too; no nano-ros knob carries that define today.

## What closes it

1. Push the patch to the fork's CAN lines (`feat/can-link-ros`, and
   `feat/can-link` for the upstream PR — the bug is upstream's, so it is worth
   a PR of its own independent of the CAN link).
2. Run the CAN peers with `sequence_number_resolution: "8bit"`, multicast QoS
   on and `prio_bits > 0`, and build zenoh-pico peers with
   `Z_SN_RESOLUTION 0x00`. On `vcan0`, check the QoS session comes up (the
   failure mode before was a silent bus — the TX task died on the oversize
   `Join`).
3. Then take W7's third acceptance item, which has never been measured: an
   urgent message overtaking a bulk burst from the same peer. This needs real
   hardware, because `vcan` does not arbitrate.

An 8-bit SN is a real trade, not a free one: the reliability window is half the
SN space, 128 frames per priority, against 2^31 at the default. On a CAN bus
whose batches are 63 bytes that is ~8 KB in flight per priority before an SN
wraps into ambiguity; whether that is enough for the bulk class is part of the
measurement in step 3.

No upstream PR has been opened for the link or for W7. Per the repo's fork
policy, the maintainer opens it.
