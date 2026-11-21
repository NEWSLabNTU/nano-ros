---
id: 2819
title: "Every Cyclone service client carries a 64 KiB pre-match staging buffer, sized to the largest request rather than to its own type"
status: open
type: tech-debt
severity: medium
area: [rmw, cyclonedds, memory]
related: [2820, 0778]
found: 2026-11-22
---

## What it is

`ClientState` in `packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/service.cpp`
holds `uint8_t pending_request[kWireScratch]`, a fixed 65,536 bytes per
client. It stages one request written before the request writer has matched a
server reader, because service QoS is VOLATILE and an early write is dropped.
Issue 0778 kept it to ONE slot per client because replicating it per
outstanding call "is not affordable".

The cost is per CLIENT, whatever the request type needs: an `AddTwoInts`
client (a 20-byte request plus header) pays the same 64 KiB as a client of
the largest type. Every action client brings service clients, and so do the
stock node interfaces an image now serves by default. On a heap-arena board
this comes out of the RMW heap.

The bound is also the largest request a client can SEND: since issue 2820 the
take side has no 64 KiB limit, but `build_wire_with_header` still refuses a
request larger than `pending_request`.

## Not measured yet

How many clients in-tree images create, and so what this costs per image.
Start with the AN536 Cyclone C++ entry (`nros: heap peak` line): build it
with this buffer sized to the request type and read the difference.

## Fix direction

Size the staging buffer from the request type's bound, which the client
knows at create time (the same derived bound the C++ subscription arena
uses). Or allocate it only for the window it is needed (before the first
match) and free it once the writer has matched. Either way the 64 KiB
constant stops being a per-client cost and the largest request becomes a
property of the type.
