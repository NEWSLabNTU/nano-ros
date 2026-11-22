---
id: 2819
title: "Every Cyclone service client carries a 64 KiB pre-match staging buffer, sized to the largest request rather than to its own type"
status: resolved
type: tech-debt
severity: medium
area: [rmw, cyclonedds, memory]
related: [2820, 0778]
found: 2026-11-22
resolved: 2026-11-22
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

## Fix

The slot is allocated for the request it holds, and only before the request
writer has matched. A matched client sends at once through
`write_with_header` (the inserted-header blob the reply path uses) and holds
no buffer; an unmatched one stages its request in a ddsrt allocation the size
of that request, freed once the request is written, dropped, or the client is
destroyed. The largest request is now a property of the request, not of the
backend.

Measured: on Zephyr, Cyclone's `new (std::nothrow)` is picolibc `malloc`, so
each client gave back 65,528 B of `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`
(262,144 on the native_sim Cyclone images), and an action client has three.
The nros heap that `nros: heap peak` reports never held this buffer: the
native_sim Rust Cyclone action client's line moved from 316,000 B to
310,368 B, the request-sized staging and nothing more.
`tests/service_large_request.cpp` sends 70,000-byte requests before and after
the match; with the old code both returned BUFFER_TOO_SMALL.

