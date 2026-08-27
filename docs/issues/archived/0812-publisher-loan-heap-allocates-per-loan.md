---
id: 812
title: "`nros_publisher_loan` heap-allocates a Box per loan, putting a malloc on
  the zero-copy path"
status: resolved
resolved_in: 5af7e1e44
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

## Correction: it was TWO mallocs, not one, and neither needed a pool

Reading the whole path before fixing it found a second allocation the report
above misses. A C loan against the zenoh backend goes

```
nros_publisher_loan
  -> CffiPublisher::try_lend_slot          (packages/rmw/cffi)
       -> vtable.borrow_loaned_message
            = zenoh_pub_loan               (packages/rmw/zenoh/.../lib.rs)
                 -> ZenohPublisher::try_lend_slot   (the static arena, no alloc)
```

and **both** of the middle two layers boxed a slot to manufacture a token:
`Box<ZenohSlot>` in the zenoh trampoline, then `Box<CffiSlot>` in `nros-c`.
Two `malloc`/`free` pairs per zero-copy publish. Fixing only the reported site
would have moved the cost one frame down, not removed it.

Neither layer needs storage at all, so no slot pool and no generation counter
were introduced — the state each was boxing is redundant with what commit and
discard are *already handed*:

* the publisher comes back as a parameter on both calls;
* whether the loan took the runtime's staging fallback is a property of that
  publisher's **vtable**, identical for every loan on it, not per-loan state;
* the arena behind a `ZenohSlot` is single-slot per publisher and always hands
  out a prefix of the same buffer, so a live loan is fully described by its
  publisher plus its granted length.

So each layer passes through the token the layer below it minted, and the
bottom layer's token is an integer:

* `zenoh_pub_loan` returns the granted **length**, tagged in the top bit —
  non-NULL for the runtime's NULL check, and a token from anywhere else fails
  the decode instead of being read as a length. `ZenohSlot::from_outstanding_loan`
  rebuilds the slot from it at commit / discard.
* `CffiPublisher::{try_lend_raw, commit_raw, discard_raw}` hand the backend's
  own token to the FFI caller and store nothing. They rebuild a `CffiSlot` and
  delegate to `commit_slot` / `Drop`, so the loan lifecycle keeps ONE body and
  cannot drift from the `SlotLending` path.
* `nros_publisher_loan` / `_commit` / `_discard` and their `nros_cpp_*` twins
  just forward. The `alloc` import is gone from both publisher FFI files.

The C signatures are untouched, so the committed cbindgen headers are
unchanged — as the note above predicted, this is a representation change behind
an unchanged ABI.

### What this does NOT yet buy

* **Not `heap-free` yet.** `nros-c`'s `nros_subscription_borrow` /
  `_release` and the `nros_cpp_` twins still box a `RecvView` / `RmwView` per
  borrow — the same class, the receive half, untouched here. And publisher
  *creation* through the cffi adapter still boxes the backend handle once per
  publisher. `lending` + no-`alloc` therefore still does not build; what
  changed is that the publish path no longer allocates *per loan*.
* **Still unexercised.** [issue 0814](0814-lending-never-exercised-on-hardware.md)
  stands: no embedded lane builds with `lending`, so this fix — like the code it
  replaces — has no runtime coverage on a target.
* One contract moved: `set_len`'s clamp of an over-long `actual_len` no longer
  happens in the cffi layer, because that layer no longer remembers the loan's
  capacity. The staging fallback still clamps to the staging buffer, and the
  native path's clamp is the backend's (zenoh's `truncate` no-ops above the
  loaned length, which publishes the full loan). A backend that fills the loan
  vtable slots must clamp `actual_len` itself.
