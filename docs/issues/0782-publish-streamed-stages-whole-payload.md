---
id: 782
title: "XRCE's `publish_streamed` heap-allocates the whole message on every
  publish — the only message-sized, per-publish allocation in that backend"
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

## Checked 2026-08-25 — the report holds, and three things in it are wrong

Verified against the code. The core claim stands: **XRCE's `publish_streamed`
`malloc`s the entire payload**, so on that backend the slot delivers none of
the memory benefit it advertises. But the reasoning around it needs correcting,
because as filed this issue would send someone at the wrong fix.

### 1. "Strictly worse" is not right — the malloc buys atomicity

XRCE has **no way to abort a prepared output stream.** Once
`uxr_prepare_output_stream` reserves a slot, that slot goes out on the next
`uxr_run_session_time`; there is no cancel in the client API. Staging the whole
message first is what lets the backend validate that `chunk_cb` actually
delivered what `size_cb` promised BEFORE anything is committed to the wire.

That matters because both halves are CALLER-supplied:
`ExecutorPublisher::publish_streamed(total_len, writer)` takes the length and
the writer from user code, through the C and C++ APIs too. A caller that
miscounts is a plausible bug, not just an internal invariant — and without the
staging buffer it puts a partially-filled slot on the wire.

So the trade is: unbounded heap, or validate-before-commit. Not "worse".

### 2. The reason the CODE gives for staging is not the binding one

The comment says it stages because the 4-byte CDR encapsulation header must not
reach the XRCE wire and "we can't strip from the zero-copy stream region after
the fact". That part is solvable with a 4-byte scratch: reserve
`total - 4`, absorb the header through a tiny stack buffer by passing
`cap = 4` on the first `chunk_cb`, then hand `chunk_cb` `ub.iterator` directly
for the rest. Zero staging of any size.

What actually forces the buffer is (1) above, which the comment does not
mention.

### 3. The slot's own justification describes a design that does not exist

`rmw_vtable.h` said the slot "saves the per-publisher staging buffer … where
the staging buffer dominates `.bss`". There is no per-publisher buffer and none
of this is in `.bss`:

* `EmbeddedPublisher::publish` serialises into a per-CALL **stack** array of
  `DEFAULT_TX_BUF` (= `NROS_SUBSCRIPTION_BUFFER_SIZE`, 1024 by default);
* the runtime's NULL-slot fallback stages into a 4 KiB **stack** array and
  returns `BufferTooSmall` above that.

The saving the slot offers is real and it is STACK — on an MCU with small
per-task stacks, the tighter budget of the two. Corrected in the header.

## So what should happen

Three options, and the choice is a genuine trade rather than a bug fix:

1. **Stream directly** (4-byte header scratch, `chunk_cb` writes into
   `ub.iterator`). Delivers the slot's actual promise: no staging of any size.
   Costs validate-before-commit — a caller that under-delivers puts a
   zero-padded message on the wire, and XRCE cannot abort the slot to prevent
   it.
2. **NULL the slot on XRCE.** Honest: the runtime falls back to a bounded 4 KiB
   stack stage that refuses oversize rather than heap-allocating it. Loses the
   >4 KiB ceiling XRCE currently supports.
3. **Implement XRCE fragmentation** across stream slots — the actual feature,
   and the only option that both streams and keeps the ceiling. Much larger.

(1) is what the slot is FOR, and its failure mode requires a caller bug. (2) is
smaller and safer. Not choosing here: the ABI is honest now either way, and the
choice belongs with whoever owns the XRCE target's memory budget.

zenoh's half of the report is unchanged and unverified in this pass.

## Scope, restated 2026-08-25

The title and the original framing were both about the wrong thing, so here is
what this issue actually covers after checking all four backends.

**It is NOT "the slot fails to deliver its promise."** The promise — after the
header correction above — is that the CALLER never has to hold a whole
serialised message. Measured:

| backend | slot | caller holds whole message? |
| --- | --- | --- |
| cyclonedds | NULL | no slot; runtime stages 4 KiB on the STACK, refuses more |
| uORB | NULL | same fallback |
| zenoh | implemented | **no** — `chunk_cb` fills a 1 KiB stack chunk repeatedly |
| xrce | implemented | **no** — `chunk_cb` fills a heap buffer XRCE owns |

Both implementations deliver the caller-side saving. zenoh does it properly and
even aborts cleanly on a short delivery (`z_bytes_writer_drop`), which is the
atomicity XRCE cannot have.

**It IS this:** XRCE's implementation is the only place in that backend that
allocates *a message-sized block on the hot path*. Every other allocation there
is create-time entity state — once per entity, `sizeof(state)`, bounded and
knowable at design time. `publish_streamed` allocates **once per publish, sized
by the message**, on targets (FreeRTOS / Zephyr / NuttX) whose heaps are small
and whose failure mode is fragmentation rather than an OOM killer. On Zephyr the
Rust-side arena defaults to 16 KiB.

So the harm is: a caller choosing this slot to control memory gets a
variable-size hot-path heap allocation it did not ask for and cannot see, and a
`BAD_ALLOC` at run time where the fallback would have given a bound known at
link time.

That is a real defect, it is narrow, and it has a clean fix — stream straight
into `ub.iterator` with a 4-byte header scratch, which removes the allocation
entirely. The cost is validate-before-commit, and only because `uxr` has no
abort. zenoh's version is the proof that the shape works.

The zenoh criticism in the original report is withdrawn: `z_owned_bytes_writer_t`
accumulating internally is the middleware's own buffer, and every backend must
hold the message somewhere between serialisation and the wire.

