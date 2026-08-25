---
id: 781
title: "One in-place-dispatch capability, five vtable slots: three dead upstream
  loan slots, one live RTOS pair, and a probe the ABI already answers by nullity"
status: resolved
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

## Resolution, 2026-08-26 — item 1 is REJECTED, with the counterexample

The proposal to delete `subscription_supports_in_place` and derive the
capability from `process_raw_in_place != NULL` would have broken a backend in
the tree. The survey above found two producers, both returning a constant, and
concluded nothing varies. It missed the third shape:

```rust
// packages/rmw/cffi/src/rust_adapter.rs — RustBackendAdapter::<R>::VTABLE
pub const VTABLE: NrosRmwVtable = NrosRmwVtable {
    ...
    subscription_supports_in_place: Some(subscription_supports_in_place_trampoline::<R>),
    process_raw_in_place: Some(process_raw_in_place_trampoline::<R>),
```

`VTABLE` is a `const`, so **every** `R: RustBackend` gets a non-NULL
`process_raw_in_place` whatever that backend can actually do. The answer is a
runtime `&self` method (`Subscription::supports_process_in_place`) and cannot
become an associated const the initializer branches on, because
`CffiSubscription` — which implements the same trait — multiplexes over whichever
backend registered, so its answer is not known until registration.

Two Rust backends sit behind that identical vtable today:

| backend | `process_raw_in_place` slot | capability |
| --- | --- | --- |
| `nros-rmw-zenoh` | non-NULL | true (overrides the trait method) |
| `nros-rmw-metadata` | non-NULL | **false** (takes the trait default) |

Same nullity, different capability. Deriving from nullity routes
`nros-rmw-metadata`'s subscriptions into in-place arena dispatch, where
`Subscription::process_raw_in_place`'s default body returns
`Err(TransportError::MessageTooLarge)` — every take fails, on a path chosen at
subscription creation and never revisited.

So the probe stays, and its declared reason now says why nullity cannot replace
it rather than restating what it queries.

### What was actually wrong: the mechanisms contradicted each other

`process_raw_in_place`'s doc said "NULL function pointer = unsupported", and no
code read it that way — `create_subscription` consulted the probe alone. A
backend answering the probe `true` over a NULL slot therefore selected in-place
dispatch and then failed every take. RFC-0038 had specified the CONJUNCTION from
the start ("until `CffiSubscriber` forwards `process_raw_in_place` to a non-NULL
vtable slot **and** returns `true` from `supports_process_in_place`"); the
implementation landed half of it. The capability is now that conjunction, and
both arms have a test (`in_place_unsupported_when_probe_says_no_behind_a_live_slot`
is the metadata shape; `..._when_probe_says_yes_over_a_null_slot` is the arm that
was live and wrong — it fails against the pre-fix code).

The issue was right that one capability had two answers. It picked the wrong one
to delete, because it inferred the ABI's behaviour from the two backends whose
source spells the capability out and not from the adapter that installs slots for
everyone. A survey that reads implementations finds what implementations say; the
`const` that writes a vtable for every future backend says something none of them
do.

### Item 2 — the subscription loan pair is KEPT

Confirmed dead on the producer side: `take_loaned_message`,
`take_loaned_message_with_info` and `return_loaned_message_from_subscription` are
NULL in cyclonedds, NULL in XRCE, and left at `EMPTY_VTABLE` by the Rust adapter.
Every `try_borrow` across the C ABI takes the copy fallback today.

Kept anyway, for a reason `process_raw_in_place` cannot cover: the scoped
callback ends the borrow when it returns, so it cannot hand a view to a caller
that outlives the call — which is exactly what `nros-c` / `nros-cpp`
`try_borrow` do (`packages/api/nros-c/src/subscription.rs:746`). The loan pair is
the only shape in this ABI that can serve them, it is upstream-named, and its
consumer degrades correctly to a copy when the slot is NULL. Deleting it would
trade real parity and a designed extension point for three NULL pointers.

What was wrong was the silence: the slot's existence read as the capability
working. The header now records "no backend fills this slot today", and the
parity map's detail says the same, so the row is still `vtable` (a slot IS where
we answer it) without implying a producer.

### Item 3 — `process_raw_in_place`'s reason is rewritten

Now the scoped borrow: it ends with the callback, so there is no release token to
leak, whereas a caller who forgets upstream's return retires one entry of a
fixed-depth receive ring on a target with no reclaim. "No copy" is dropped — it is
a property both shapes share and upstream has a name for it.

### Adjacent, not fixed here

Counting the slots no backend fills, the loan trio is 5 of **42 out of 74**.
Several are optional by design with a documented runtime default
(`get_implementation_identifier` answers with the registry name), but others are
plain gaps — `set_log_severity` landed in this same campaign with a slot, a
dispatcher and stub tests, and no backend body, while the parity map counts it
answered. Filed separately: the fix is a declared unfilled-slot table with a
reason each, plus a gate, which is 42 authored reasons and its own piece of work.
