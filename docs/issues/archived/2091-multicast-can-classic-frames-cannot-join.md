---
id: 2091
title: "The multicast CAN link cannot open on a classic (8-byte) interface: the JOIN does not fit a 7-byte batch, and RFC-0080 says it does"
status: resolved
type: limitation
area: [zenoh, can, zenoh-pico]
severity: medium
found: 2026-10-27
resolved: 2026-10-27
resolved_in: "zenoh-pico fork nano-ros-1.10.1 2594f6b4 (pinned by this fix's PR)"
related: [phase-514, 2096, 0080, 0081, 0083]
---

**Home: [phase-514](../../roadmap/phase-514-network-config-layers.md)** (found
while measuring W5).

## Measured

Two zenoh-pico peers (`z_pub` / `z_sub`, `-m peer -l
'can/vcan0#id=0x10N;match=0;mask=0'`) on a host `vcan0`, the library built
with `-DZ_FEATURE_LINK_CAN=1 -DZ_FEATURE_LINK_ISOTP=1` in `CMAKE_C_FLAGS`:

| `vcan0` MTU | link mode | result |
| --- | --- | --- |
| 72 (CAN FD) | FD, link MTU 63 | 7 of 7 samples delivered |
| 16 (classic) | classic, link MTU 7 | both peers `Unable to open session!`; no frame on the bus |

It predates W5, by a different road. The fork commit before W5 (`06ac6bf8`)
asked for CAN FD whenever the locator's `dbitrate` was non-zero, which the
default is, so on a classic interface its first FD-sized write failed with
`CAN: write failed: 22` (EINVAL). W5 reads FD from the interface instead, so the
link came up classic at MTU 7 and the JOIN encode failed before any write —
strace: the socket opens, the filter is set, `bind()` succeeds, and nothing is
written. Both end in the same `Unable to open session!`, and neither says why.

## Why

`_z_multicast_transport_create` sizes the write batch as
`min(link MTU, Z_BATCH_MULTICAST_SIZE)` — 7 bytes on classic CAN. The first
thing `_z_multicast_open_peer` sends is a JOIN, about 31 bytes (it carries the
16-byte ZID; the FD capture shows it as a 32-byte frame, the length prefix
included). A JOIN is a TRANSPORT message, and zenoh fragments only network
messages, so it cannot be split across frames: its encoding into a 7-byte
buffer fails and the open fails with it.

RFC-0080 line 125 states the opposite — "Classic CAN (8-byte) falls out of the
same scheme at MTU 7" — which holds for fragmented NETWORK messages and not for
the transport messages a multicast session cannot do without.

## What it means today

Multicast CAN needs a CAN FD interface. On classic-only hardware, which is
most of the hardware in the field, the working link is ISO-TP (RFC-0083):
ISO-TP segments one PDU of up to 4095 bytes over 8-byte frames, so a JOIN is
never the link's problem.

## Fix direction (not decided)

Either correct RFC-0080 to say the multicast link is FD-only and have the link
REFUSE a classic interface at open, naming ISO-TP (one loud error instead of a
generic session failure), or give the link its own segmentation below the
transport, which is what ISO-TP already is.

## Resolution — a protocol limit, and two links that reported it badly

**The limit is zenoh's, not the link's.** Only network messages fragment; a
transport message (JOIN, and INIT/OPEN on unicast) must fit one batch, and a
datagram link's batch is one datagram — one 7-byte frame here. The smallest
JOIN `_z_join_encode` can produce is 13 bytes (header, version, flags, a 1-byte
ZID, lease, two sequence numbers of up to 4 bytes each); zenoh-pico's 16-byte
ZID makes it the 31 measured on the bus. No framing choice in the CAN link can
change that; a link with its own segmentation underneath can, and that link is
ISO-TP (RFC-0083).

**What was a real defect** was how both implementations met it:

| implementation | on a classic `vcan0` (MTU 16), before | after |
| --- | --- | --- |
| zenoh-pico (`2594f6b4`) | `Unable to open session!`, no reason | refused at open: `CAN: 'vcan0' is a classic (8-byte) CAN interface. The multicast CAN link is CAN FD only … use the ISO-TP link (isotp/vcan0#...)` |
| zenoh-rs (fork `jerry73204/zenoh`: `feat/can-links-ros` c6b53383 for 1.8, `feat/can-links` 1c38b13e for 1.10) | the session OPENS and `z_pub` reports a put every second, with **0 frames on the bus** | refused at open: `CAN: interface "vcan0" is classic CAN (MTU 16, not 72) …` |

zenoh-rs's was the worse one: it meant to refuse a classic interface, but
tested it with `setsockopt(CAN_RAW_FD_FRAMES)`, which the kernel accepts on
every CAN_RAW socket — it checks the device only at send. Both links now read
the interface MTU (`SIOCGIFMTU`), which is what actually says FD or classic.

Measured after: FD multicast unchanged — pico ↔ pico 7/7, zenoh-rs ↔ zenoh-rs
8/8, and zenoh-rs ↔ zenoh-pico 8/8 each way with the pico side built at
`BATCH_MULTICAST_SIZE=63` (phase-378's batch-size requirement, unchanged by
this). RFC-0080 §4.1 and RFC-0081 §4.5 now say CAN FD only and why.
