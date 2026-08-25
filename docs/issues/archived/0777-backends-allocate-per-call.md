---
id: 777
title: "\"Pools are baked\" is true of one backend in five — every RMW deviation
  reason built on that clause was false"
status: resolved
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


## Direction item 2, done 2026-08-26 — the measurement, and it is re-runnable

`scripts/rmw-alloc-sites.py` (lane `just check-rmw-alloc-sites`, on the fast
line). It attributes every allocation call in a backend's own sources to its
enclosing function and splits them by whether that function is reached per
MESSAGE or at entity creation. `--check` fails on a steady-state allocation with
no declared reason, so the next one has to be argued for instead of merged. A
prose count would have rotted exactly the way the clause this issue is about
rotted.

| backend | steady-state | create / init |
| --- | --- | --- |
| cyclonedds | **6** | 6 |
| xrce | **0** | 9 |
| uorb | **0** | 3 |

The table in the Problem section is now stale in one row and it is worth saying
which: **XRCE reached zero when issue 0782 landed.** Its one per-publish
`malloc` was the streamed-publish staging buffer, and removing it left every
remaining allocation in an `xrce_*_create` / `*_init`. uORB never had one. So
"pools are baked" is now true of two backends of five rather than one — still
not the four it was claimed for.

Cyclone's six, and what each would cost to remove:

- `publisher_publish_raw:262` — `ddsrt_malloc(body_len)`, **message-sized, per
  publish**. It copies `data + 4` to strip the CDR encapsulation header, and it
  looks droppable: `dds_istream_init` takes a `const void *`, so the stream could
  point into the caller's buffer, which is exactly the shape issue 0782 used to
  delete XRCE's. **It is not droppable, and the reason is alignment.**
  `dds_cdr_alignto` rounds the stream INDEX and reads at `m_buffer + m_index`,
  so an 8-aligned index only produces an 8-aligned address when the BASE is
  8-aligned. `ddsrt_malloc` guarantees that; `data + 4` is 4-aligned at best, so
  any message carrying an `int64`/`double` would take an unaligned 64-bit read.
  The copy is load-bearing. Recorded because the removal is attractive, wrong,
  and would have passed every test on x86.
- `publisher_publish_raw:295` and `subscription_take:149` —
  `ddsrt_calloc(1, desc->m_size)`, the typed sample. **Fixed size, known at
  create time.** These are the two this issue named, and they are removable:
  hold one per publisher and per subscription, and per operation call
  `dds_stream_free_sample` + re-zero rather than free. That does not remove what
  `dds_stream_read_sample` allocates internally for variable-length fields, so
  it is a full fix only for fixed-size message types — which is most of what an
  embedded image publishes.
- `write_typed:638`, `:652` and the `write_fibonacci_get_result_response:480`
  nested inside it — the request/reply analogue of the publish path, so per
  request and per reply.

Not measured, and not measurable here: allocations inside the middleware
libraries. Cyclone allocates below `dds_write`/`dds_take` and zenoh-pico
allocates per message in its own C. An image on either calls a general allocator
per message whatever this tool reports — the split is about which allocations
are OURS to remove, not about how many happen.

One correction to the Problem table: the cffi shim's per-loan
`alloc::vec![0u8; len]` (now `lib.rs:2211`, in `SlotLending::try_lend_slot`) is
not a fallback that sometimes fires. `borrow_loaned_message` is NULL in every
backend vtable (issue 0800), so it is the path, always.

## Direction item 3, decided 2026-08-26

**No backend is `core`-only, and two are allocation-free on the data plane.**
Stated that way because "is a `no_std` image claimed?" turns out to be two
questions with different answers, and conflating them is how the original clause
got written:

- **Crate flavour.** ARCHITECTURE §2's terminal state for the core crates is
  `core` and `core+alloc`. Every backend allocates at entity creation, so every
  backend needs `alloc`. All four are `core+alloc` backends. Nothing here claims
  or needs `core`-only, and no future work should try to make cyclonedds or
  zenoh reach it.
- **Steady-state behaviour, which is the one that decides real-time
  reasoning.** XRCE and uORB do not allocate after setup: an image on either may
  reason about worst-case latency and heap exhaustion as if the heap were frozen
  once the graph is up. **cyclonedds and zenoh may not**, and neither may any
  image using the cffi lending fallback. Their data plane calls a general
  allocator per message, so their worst case includes whatever the allocator's
  is, and a long-running image on a fragmenting heap can fail on a publish that
  succeeded a million times.

That distinction is now what the seven declarations rest on, and it is checked:
`--check` fails when a backend gains a steady-state allocation, which is the
event that would make this paragraph false.
