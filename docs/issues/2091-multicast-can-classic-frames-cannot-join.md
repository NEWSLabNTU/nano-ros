---
id: 2091
title: "The multicast CAN link cannot open on a classic (8-byte) interface: the JOIN does not fit a 7-byte batch, and RFC-0080 says it does"
status: open
type: limitation
area: [zenoh, can, zenoh-pico]
severity: medium
found: 2026-10-27
related: [phase-514, 0080, 0083]
---

**Home: [phase-514](../roadmap/phase-514-network-config-layers.md)** (found
while measuring W5).

## Measured

Two zenoh-pico peers (`z_pub` / `z_sub`, `-m peer -l
'can/vcan0#id=0x10N;match=0;mask=0'`) on a host `vcan0`, the library built
with `-DZ_FEATURE_LINK_CAN=1 -DZ_FEATURE_LINK_ISOTP=1` in `CMAKE_C_FLAGS`:

| `vcan0` MTU | link mode | result |
| --- | --- | --- |
| 72 (CAN FD) | FD, link MTU 63 | 7 of 7 samples delivered |
| 16 (classic) | classic, link MTU 7 | both peers `Unable to open session!`; no frame on the bus |

The classic row is the same on the fork commit before phase-514 W5
(`06ac6bf8`, where a locator without `dbitrate` also chose classic framing),
so this predates W5. strace on the classic run: the socket opens, the filter
is set, `bind()` succeeds — and nothing is written.

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
