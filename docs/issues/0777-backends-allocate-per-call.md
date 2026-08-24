---
id: 777
title: "\"Pools are baked\" is true of one backend in five — every RMW deviation
  reason built on that clause was false"
status: open
type: bug
area: rmw, memory
related: [phase-376, issue-0776]
---

## Problem

Seven declared RMW-ABI differences justified themselves with a clause saying
this tree preallocates: *"no runtime allocation to pre-size; pools are baked"*.
It reads like a design property of an embedded system. It is true of exactly one
backend.

Measured in the tree, 2026-08-24:

| backend | allocates | where |
| --- | --- | --- |
| cyclonedds | per PUBLISH and per TAKE | `publisher.cpp:202` `ddsrt_malloc(body_len)`, `:235` `ddsrt_calloc(1, desc->m_size)`; `subscriber.cpp:143` `ddsrt_calloc(1, desc->m_size)` |
| zenoh | per publish and per take | inside zenoh-pico |
| xrce | per streamed publish | |
| **the cffi shim itself** | per fallback loan | `packages/rmw/cffi/src/lib.rs:2025` `alloc::vec![0u8; len]` |
| uORB | — | the only one that matches the claim |

The clause was load-bearing in seven places: the four declined
`rmw_{init,fini}_{publisher,subscription}_allocation` symbols, and the declared
argument deviations on `publish`, `take` and `take_sequence`.

## Why the conclusion survives anyway, and why that is not a defence

Declining upstream's allocation arguments is still right — but for a DIFFERENT
reason, which is now recorded in their place: upstream's
`rmw_publisher_allocation_t` pre-sizes a per-entity `rcutils_allocator_t` that
the CALLER owns, and this ABI has no allocator to hand one. There is nothing for
the argument to point at.

That the right answer was reached through a false premise is the finding. A
reason nobody checks is a reason that can be wrong for years while the
conclusion it supports stays green — and this one was reused six times after it
was first written, which is how a single unchecked sentence becomes a property
of the design nobody can see is untrue.

## What is NOT claimed here

That the allocations are wrong. Cyclone allocating a typed sample per take is
Cyclone's design, and this issue does not propose changing it.

What IS claimed: an image built on cyclonedds or zenoh calls into a general
allocator on the hot path, so any target-side reasoning that assumes otherwise —
worst-case latency, heap exhaustion, `no_std` reachability — is unsound for four
of five backends. Whether that matters is a question this issue exists to make
askable; it was previously unaskable because the tree asserted it did not happen.

## Direction

1. **Done here:** the false clause is retired from all seven declarations
   (phase-376's `ARG_DEVIATIONS` and the parity map), replaced by the reason that
   holds.
2. **Worth measuring:** which allocations are on the steady-state path versus
   entity creation. `ddsrt_calloc` per take is the one that would matter on a
   target with a real-time budget.
3. **Worth deciding:** whether a `no_std`-reachable image is claimed for cyclone
   and zenoh at all. If it is, the allocation sites are the gap; if not, saying
   so is better than a clause implying the opposite.

## Correction, 2026-08-24: the replacement reason was also false

This issue said the seven declarations should say *"upstream pre-sizes a
per-entity `rcutils_allocator_t` the CALLER owns, and this ABI has no allocator
to hand one"*. That is wrong in the same way the clause it replaced was wrong —
plausible, embedded-sounding, and never checked against the thing it describes.

Humble's `rmw/types.h`, read in the ros2 distrobox:

```c
typedef struct RMW_PUBLIC_TYPE rmw_publisher_allocation_s
{
  const char * implementation_identifier;
  void * data;
} rmw_publisher_allocation_t;
```

No allocator. It is an opaque per-implementation handle, and
`rmw_subscription_allocation_t` is identical.

The reason that survives checking: the only thing that produces one is
`rmw_init_{publisher,subscription}_allocation`, whose other two parameters are a
`rosidl_message_type_support_t *` and a `rosidl_runtime_c__Sequence__bound *` —
both declined ABI-wide since W3.c. Nothing here can make one, so the argument
has nothing to point at. That holds regardless of what any backend allocates.

Two wrong reasons in one week for one parameter is the finding. Both passed
`rmw-abi-shape --check`, because that gate gets a difference DECLARED and cannot
tell a true declaration from a false one — which is exactly why W5's
reason-by-reason audit exists and why it should not be skipped for the parts
that "obviously" hold.

The CAPABILITY question this issue opened stays open and is unaffected: cyclone
still calls `ddsrt_calloc(1, desc->m_size)` on every publish and every take, and
`desc->m_size` is knowable at create time. If a pre-size slot is wanted it is
ours to design (no allocator, no typesupport — something like
`subscription_set_sample_pool(sub, max_serialized_size, depth)`), not upstream's
symbol to adopt.

