---
id: 782
title: "`publish_streamed` exists to avoid a per-publisher staging buffer, and
  XRCE implements it with a `malloc` of the whole payload"
status: open
type: bug
area: rmw, memory
related: [phase-376, issue-0777]
---

## Problem

The slot's justification in `rmw_vtable.h` is real and target-shaped:

> Saves the per-publisher staging buffer on RAM-constrained nodes — useful for
> large messages on MCUs where the staging buffer dominates `.bss`.

Neither implementation delivers it.

**XRCE** (`nros-rmw-xrce/src/publisher.c`) calls `malloc(total)` for the entire
payload, drives `chunk_cb` with `cap = total - staged` (so typically a single
call), `memcpy`s into the stream slot, and frees. On the target class this slot
exists for — an MCU with no allocator, where the static staging buffer is the
cost being avoided — it replaces a `.bss` array with a same-sized runtime heap
allocation. That is strictly worse: same peak, plus fragmentation, plus a
failure mode at run time instead of link time. It also returns
`MESSAGE_TOO_LARGE` when the body exceeds one stream slot.

**zenoh** (`zpico-sys/c/zpico/zpico.c`) accumulates into a growing
`z_owned_bytes_writer_t` before `z_publisher_put`. The only bounded thing on
that path is a 1 KiB stack `chunk[1024]`.

## Note on the recorded reason

The `ADDED` reason said "publish a payload larger than any single buffer the
target can hold", which no implementation provides and which XRCE explicitly
refuses. Corrected to the header's own wording as part of phase-376 W5; the
IMPLEMENTATIONS are what this issue tracks.

## Fix

Either make XRCE fragment properly across stream slots — the point of a
streaming API — or make it NULL and let the runtime use the buffered path
honestly. A slot whose implementation defeats its own purpose is worse than an
absent one, because callers select it believing they saved the memory.

Same family as issue 0777: a memory claim in the ABI that no backend keeps.
