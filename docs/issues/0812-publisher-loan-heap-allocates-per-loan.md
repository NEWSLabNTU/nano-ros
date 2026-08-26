---
id: 812
title: "`nros_publisher_loan` heap-allocates a Box per loan, putting a malloc on
  the zero-copy path"
status: open
type: bug
area: api
related: [issue-0813, issue-0814, phase-391]
---

## Problem

The loan API exists to avoid a copy. Its C entry point allocates:

```rust
// packages/api/nros-c/src/publisher.rs:582
let boxed = alloc::boxed::Box::new(slot);
*out_token = alloc::boxed::Box::into_raw(boxed) as *mut core::ffi::c_void;
```

Every `nros_publisher_loan` call does one heap allocation to manufacture a
stable FFI token, and every commit/discard does the matching free. The backend
slot itself is fine — `ZenohPublisher`'s `LendArena` is a static
`[u8; ZENOH_TX_BUF]` guarded by an `AtomicBool`, no allocation. The malloc is
purely the token.

## Why it matters

Two reasons, in order:

1. **It is on the real-time path.** Zero-copy publish exists for RAM-tight,
   copy-count-sensitive targets. Handing that path an allocation with the
   allocator's worst-case latency is the one thing it was supposed to avoid.
2. **A heap-free tier cannot use the loan API at all.** The tier model in
   [phase 391](../roadmap/phase-391-allocation-unification-and-tier-model.md)
   denies every allocation symbol in the image. As written, `lending` and
   `heap-free` are mutually exclusive — for no reason inherent to lending.

## Shape of the fix

The token does not need to be a pointer. The backend arena is already
single-slot per publisher and indexed; a small integer handle (slot index +
generation counter) carries the same information, cannot be forged into a wild
pointer, and needs no allocation. `NROS_RET_TRY_AGAIN` already exists for
"no slot available", so the failure mode is unchanged.

Note the ABI: `out_token` is `void**`. Encoding an index in it is a
representation change behind an unchanged signature, not an ABI break.
