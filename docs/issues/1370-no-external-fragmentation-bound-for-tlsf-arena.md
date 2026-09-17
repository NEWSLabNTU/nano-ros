---
id: 1370
title: "The unified TLSF arena bounds INTERNAL fragmentation at 6.25% and bounds
  external fragmentation nowhere: the phase-391 argument is that Robson's bound
  is 'cheap to defend', and nothing in the tree defends it"
status: open
type: tech-debt
area: [memory, zephyr, rmw]
severity: medium
found: 2026-09-18
related: [issue-0968, issue-1010, issue-1033, phase-391, phase-392, phase-412, rfc-0034]
---

## The one bound that exists

`packages/rmw/zenoh/zpico-alloc/src/lib.rs:63-65`:

```rust
/// phase-391 W2 - the second-level list length. 16 bounds internal
/// fragmentation at 1/SLLEN = 6.25%.
const SLLEN: usize = 16;
```

That is a real, derived, checkable number, and it is the ONLY fragmentation
figure the allocator states. It bounds the waste inside a block that was
rounded up to a size class. It says nothing about the free memory that exists
but cannot be handed out because it is split across non-adjacent holes.

## The external bound is an adjective

`docs/roadmap/phase-391-allocation-unification-and-tier-model.md:110-118` is
the whole of the external-fragmentation argument:

> The decision that payload buffers **stay static** (they do not move to the
> heap) is what makes TLSF sizeable here. Robson's bound scales with the ratio
> of largest to smallest block; a heap holding both 20-byte key expressions and
> megabyte payloads has a ~2^16 spread and a punishing worst case. A heap that
> holds only *infrastructure* - zenoh-pico's sessions, key expressions and
> strings, and Rust `String`/`Vec` churn - has a narrow spread, and the bound
> becomes cheap to defend.

Every step of that is sound. None of it is a number. "Narrow spread" is not a
ratio; "cheap to defend" is not a defence. The smallest and largest block
classes the infrastructure traffic actually uses are not measured anywhere in
this tree, so Robson's bound cannot be evaluated even by hand.

The tree already knows this. `phase-392-static-memory-space-campaign.md:214-216`,
listing what a shared arena costs:

> **Fragmentation.** Static pools cannot fragment. rlsf's bound is
> `1/SLLEN` internal, but external fragmentation across mixed lifetimes is a
> property of the traffic, not of the allocator. Needs measurement, not
> argument.

phase-391 landed the arena. The measurement phase-392 said it needs has not
been taken.

## The instrumentation that exists cannot supply it

`FreeListHeap` tracks live bytes and a sticky peak (`zpico-alloc/src/lib.rs:488`,
`:496`), both behind `feature = "stats"`. The one accessor that sounds like a
fragmentation answer disclaims itself at `zpico-alloc/src/lib.rs:500-506`:

```rust
    /// Free bytes remaining (approximate - does not account for fragmentation).
    #[cfg(feature = "stats")]
    pub fn free_bytes(&self) -> usize {
        (N + SLAB_REGION_SIZE).saturating_sub(self.used_bytes.load(Ordering::Relaxed))
    }
```

It is a subtraction, not a walk of the free lists. The quantity an external
bound is stated against - the LARGEST CONTIGUOUS free block, against the
largest live request - is not exposed by this crate at all. `peak()` answers
"how many bytes were live at once", which is the input to arena SIZING, not to
a fragmentation argument: an arena can pass a peak check and still fail an
allocation because no single hole is large enough.

## What the failure mode is, when it happens

Exhaustion is not silent any more, and it is also not a fragmentation report.
`packages/platform/nros-platform-zephyr/src/platform.c:212-218`:

```c
        printk("nros: HEAP EXHAUSTED: request %zu bytes, arena %zu bytes, "
               "caller %p\n"
               "      (addr2line -f -e zephyr.elf %p to name it; raise "
               "CONFIG_NROS_ZEPHYR_HEAP_SIZE / NROS_ZEPHYR_HEAP_SIZE only once "
               "you know what asked)\n",
               size, nros_zephyr_heap_capacity(),
               __builtin_return_address(0), __builtin_return_address(0));
```

`nros_platform_alloc` then returns NULL and the caller does whatever it does;
`docs/roadmap/phase-412-derived-counts-and-sizes.md:190` records that "the
process does not fault - but it is no longer mute". An image that outgrows the
arena stops.

Note what the message reports and what it does not. It prints the REQUEST and
the ARENA CAPACITY. It does not print live bytes, and it does not print the
largest free block, so the line cannot distinguish "the arena is genuinely too
small" from "the arena has the bytes and they are in the wrong shape". Every
recorded instance so far has been the first case by a wide margin and was read
that way - `docs/issues/0968-tier2-runtime-failures-unreproduced.md:206`
(request 427,968 against arena 66,048),
`docs/issues/archived/1010-zephyr-xrce-executor-arena-exceeds-heap.md:27`
(329,648 against 66,048),
`docs/issues/archived/1033-xrce-subscriber-slots-budget-eight-for-one.md:444`
(309,696 against 66,048). A request four to six times the whole arena is not a
fragmentation failure. That is exactly why fragmentation has never been
measured: it has never yet been the cause.

## Why this is filed rather than left

A safety argument for this image needs a numeric external-fragmentation bound
and cannot source one from this tree. Right now the honest answer to "what is
the worst-case heap this image can need" is the peak plus an unbounded term.
That is fine for a demo and is not fine for the thing this arena is being built
toward.

Two further facts make the qualitative argument weaker than it reads:

* The narrow-spread premise holds only while payload buffers stay static.
  `docs/design/0002-rt-execution-model.md:459-463` names widening the block-size
  spread as the standing case against heap-backed payloads, and
  phase-392 amendment B has reopened exactly that question. The premise is a
  decision that is under review, not a property of the design.
* `zpico-alloc/src/lib.rs:453-457` records that the used-bytes counter was
  cumulative rather than live until phase-412 - "the island reported 395,132
  used against a 94,720-byte arena". The instrumentation this argument would be
  built on has been wrong before, in the direction of looking worse than
  reality, and nothing yet cross-checks it against the allocator's own free
  lists.

## What would answer it

1. **Expose the shape, not just the total.** rlsf can walk its free lists;
   `largest_free_block()` beside `used()`/`peak()` in `zpico-alloc` costs a
   walk on a diagnostic path and turns the unanswerable question into a
   measurement. Add it to the `HEAP EXHAUSTED` line, so the next occurrence
   says which failure it is.
2. **Measure the spread.** The smallest and largest block sizes zenoh-pico's
   infrastructure traffic actually requests, over one real image's bringup and
   one steady-state window. That ratio is the input Robson's bound takes, and
   it is what turns phase-391's paragraph into an inequality.
3. **Then state the bound or state that there is none.** Either
   phase-391:110-118 gains the arithmetic, or it gains a sentence saying the
   external term is unbounded and this issue is what a consumer should read.
   The current text implies a defence exists and points at nothing.

Step 1 is the small one and is worth doing regardless: it is the difference
between an exhaustion report that is a lead and one that is a number.
