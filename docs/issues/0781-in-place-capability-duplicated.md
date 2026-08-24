---
id: 781
title: "One in-place-dispatch capability, five vtable slots: three dead upstream
  loan slots, one live RTOS pair, and a probe the ABI already answers by nullity"
status: open
type: tech-debt
area: rmw
related: [phase-376, issue-0779]
---

## Problem

Handing a subscriber the transport's own buffer instead of a copy is one
capability. The vtable spends five slots on it.

**`subscription_supports_in_place` is redundant.** Its reason reads "probe: can
this BACKEND hand out its receive buffer directly" — and the code agrees, which
is the problem. Both implementations ignore their subscription argument and
return a compile-time constant:

- `xrce/src/subscriber.c` — `(void)subscriber; … *out_supports = true;`
- `zenoh/.../subscriber.rs` — `fn supports_process_in_place(&self) -> bool { true }`

Nothing varies per subscription. The runtime already caches it once at creation
(`packages/rmw/cffi/src/lib.rs`) and already maps a missing slot to `false` — so
a NULL `process_raw_in_place` ALREADY means unsupported, exactly as that slot's
own doc states ("NULL function pointer = unsupported (the runtime uses the
buffered path)"). Nullity is this ABI's mechanism for "this backend cannot do
this operation"; the probe re-encodes a fact the vtable already carries.

**The upstream-named loan pair is dead.** `take_loaned_message` and
`return_loaned_message_from_subscription` are NULL in cyclonedds, NULL in XRCE,
and unimplemented by the Rust adapter and zenoh. (The PUBLISHER-side loan trio
is live in zenoh — this is about the subscription side only.)

**And `process_raw_in_place`'s reason describes upstream's job.** "Dispatch from
the transport's own buffer — no copy on a target with no spare RAM" is
word-for-word what `rmw_take_loaned_message` is for, and that is a slot we
already carry. The RTOS argument for the callback shape is a different one: a
scoped borrow needs no release token and cannot be leaked, whereas a caller who
forgets `return_loaned_message_from_subscription` wedges a fixed-depth ring on a
target that will never reclaim it. That is a target constraint; "no copy" is
not, because upstream has a name for it.

## Fix

Decide the pair together rather than carrying both:

1. Delete `subscription_supports_in_place`; derive the capability from
   `process_raw_in_place != NULL`. (If it is kept, its reason must name a
   backend whose answer differs per subscription — there is none today, and no
   test asserts one.)
2. Decide whether the subscription-side loan pair earns its two slots given
   nothing implements them, or whether `process_raw_in_place` is the tree's
   answer and they should go.
3. Rewrite `process_raw_in_place`'s reason around the leak-proof scoped borrow.

Whichever way (2) goes, it is an ABI change touching every backend, so it is
filed rather than folded into the W5 audit that found it.
