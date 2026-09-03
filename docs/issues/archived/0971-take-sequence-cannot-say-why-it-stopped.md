---
id: 971
title: "`take_sequence` cannot say why a drain stopped, and its two implementations disagree about it"
status: resolved
area: [rmw, api]
severity: high
related: [0969, 0773, phase-124, phase-376, phase-384]
---

# A partial drain and an empty reader are the same answer

## The contract

`rmw_vtable.h:681` and the batch-take note above it:

> Returns:
>   * `>= 0` — count of messages taken (0..=max_msgs).
>   * `< 0` — `rmw_ret_t` error code; **partial drains MUST use the count
>     form, not error-out**.
>
> […] `*taken` is written only on `NROS_RMW_RET_OK`; **a partial drain reports
> what it got rather than erroring**.

And, of the runtime's loop fallback for backends with no native batch:

> The fallback gives **identical observable behaviour** (each call still costs N
> vtable hops) but lets user code commit to the batched API.

Three claims. The first two are honoured. The third is not, and the first two
are not enough.

## What actually happens on an oversized message

`subscription_take_sequence_count`
(`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/subscriber.cpp`):

```cpp
const uint32_t total = ddsi_serdata_size(ds[i]);
if (per_msg_cap < total) {
    break;
}
```

then, at the bottom, every serdata taken this call is unref'd and `produced` is
returned as a count. So:

* the batch ends,
* the samples already written are reported,
* **and the oversized message is gone.** `dds_takecdr` removed it from the
  reader cache before its size could be known, and the unref is the last
  reference. It is not deferred to the next call; there is no next chance.

The caller receives a number, and that number is the same shape it receives when
the reader is simply empty. It cannot distinguish "you have everything" from "a
message was too big for your slots and has been destroyed".

## And the fallback answers differently

`CffiSubscriber::take_sequence`'s loop (`packages/rmw/cffi/src/lib.rs`):

```rust
match self.take_serialized(slot)? {
    Some(len) => { out_lens[i] = len; count += 1; }
    None => break,
}
```

`take_serialized` returns `BUFFER_TOO_SMALL` for the same message, and `?`
propagates it. The fallback therefore returns `Err(TransportError::BufferTooSmall)`
where the native path returns `Ok(partial_count)`.

The messages already written into `buf` are still there in both cases, but the
fallback discards the count on its way out, so a caller has no way to know how
many slots are valid. Same input, two different answers, on precisely the
condition the doc says is indistinguishable. A consumer that develops against
zenoh-pico (fallback) and deploys on Cyclone (native) changes behaviour without
changing a line.

## The error branch that was already dead

This is not a regression from **#0969**, and
the shape predates it. The pre-0969 body read:

```cpp
if (per_msg_cap < total) {
    dds_ostream_fini(&os);
    err = NROS_RMW_RET_BUFFER_TOO_SMALL;
    break;
}
...
if (err < 0) {
    return err;
}
```

`err < 0` is never true. Every `nros_rmw_ret_t` is non-negative (`rmw_ret.h`),
which is exactly why this function's other returns negate
(`-static_cast<int32_t>(...)`, issue 0773). So the error was unreachable from
the day it was written, and every caller has only ever seen the partial count.
#0969 rewrote the body onto `dds_takecdr` and kept the behaviour deliberately,
with a comment saying so, rather than change semantics inside a data-path
rewrite. This issue is that comment's promised follow-up.

Note that the dead branch was ALSO the wrong fix: returning
`BUFFER_TOO_SMALL` from a partial drain is what the contract forbids. Making it
reachable would have traded a silent drop for a contract violation.

## What `subscription_take` (single) does is CORRECT — a correction to this issue

This section originally claimed the single take had "the same root, one level
down", on the grounds that it too destroys the message:

```cpp
const uint32_t total = ddsi_serdata_size(d);
if (buf_len < total) {
    ddsi_serdata_unref(d);
    return NROS_RMW_RET_BUFFER_TOO_SMALL;
}
```

That was wrong, and the tree says so twice.

**Live zenoh does the identical thing, deliberately**
(`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs:1019`):

```rust
// Oversized for the caller's buffer — drop the slot so the
// subscription isn't permanently stuck.
buffer.consume_head();
return Err(TransportError::BufferTooSmall);
```

**And it is formally verified.** `nros-verification/src/e2e.rs`'s
`take_post_fix` models the take path after the 31.6 fix, and its listed
difference from the pre-fix version is exactly this:

> 2. On `BufferTooSmall` (stored_len > rx_buf_len) → clears `has_data`
>    (**FIXED: no longer stuck**)

with proof `no_silent_truncation` establishing that the consumer "**never**
receives truncated data" — it gets the complete message or an explicit error.

So the invariant is **no stuck subscription and no silent truncation**, not
"the message survives". Consume-then-refuse is the design, Cyclone's single take
already implements it, and it needs no change. The defect in this issue is
narrower than first written: it is the SEQUENCE path only, and only because that
is the one place the explicit error has nowhere to go.

## The fix

Where the error cannot be returned inline, carry it to the next call. That is
the same shape `take_post_fix` already has — check a flag first, clear it,
return the error, take nothing — applied one call later.

**Cyclone.** `SubState` gains `pending_too_small`:

* `subscription_take_sequence_count`, at the top: flag set → clear it, return
  the negated `BUFFER_TOO_SMALL`, take nothing.
* in the loop, on an oversized sample: set the flag, `break`, return the count.
  Partial drains still use the count form, as the contract requires.
* `subscription_take`, at the top: the same check, so a caller that mixes the
  two entry points still hears about it. Its own oversize path keeps returning
  the status inline, because it can.

**Runtime fallback.** `CffiSubscriber` gains `pending: Option<TransportError>`:

* loop hits `BufferTooSmall` with `count > 0` → stash it, return `Ok(count)`.
* with `count == 0` → return the error directly, as it does now.
* at the top, `pending.take()` → return it.

Both paths then emit the identical *sequence* of answers, which is what makes
`rmw_vtable.h`'s "identical observable behaviour" claim true rather than
something to delete. No signature change, so no `abi_version` bump.

The error arrives one call late. That is the price of a contract that forbids
erroring out of a partial drain, and it is strictly better than never.

## Rejected, and why

1. **Return negated `BUFFER_TOO_SMALL` from the batch.** Contract-forbidden, and
   worse in practice — the caller loses the count for messages that were
   delivered fine.
2. **Skip the oversized sample and keep draining.** Delivers more, still
   destroys the message with no signal. Violates `no_silent_truncation`.
3. **An overflow out-parameter plus an `abi_version` bump.** Works, and is
   unnecessary: the pending flag carries the same fact with no ABI change.
4. **`dds_readcdr` to size before consuming, leaving the sample in the cache so
   a caller with a bigger buffer can retry.** This was offered here as the
   option that "makes the message survivable". It is the bug that was already
   fixed: a sample left in the cache that no caller can ever take is precisely
   the stuck subscription `take_post_fix` exists to rule out, and a caller
   whose buffer is sized from the type bound will not come back with a bigger
   one anyway.

## What a fix must satisfy

* A caller can distinguish "reader drained" from "a message did not fit", on
  BOTH the native and the fallback path, with the same answer.
* The doc's claim that the fallback is observationally identical is made true.
* Messages already written are never lost to the reporting of a later failure.
* No subscription is left stuck, and nothing is truncated silently — the
  invariants `no_silent_truncation` already fixes in place.
* The behaviour is stated where the slot is declared, not inferred from two
  implementations that currently disagree.

## Not to be confused with

**#0969** (`0969-cyclone-take-cdr-round-trip.md`, filed in PR #154 — not linked
here because it has not landed on `main` yet, and `check-doc-refs` is right to
say so) — the CDR round trip on the take
path. It touched this function, kept this behaviour unchanged on purpose, and
named this issue as the place the question belongs.

[#0964](0964-two-different-sizes-for-the-same-type.md) — how big a receive
buffer should be. That is about picking `per_msg_cap`; this is about what
happens when the pick is wrong.

## Resolution — implemented as specified, on both paths, 2026-09-03

Verified against the code rather than the text; the fix had landed and only this
file was open.

**Cyclone** (`subscriber.cpp`). `SubState::pending_too_small`, set in the drain
loop when a sample does not fit and delivered at the top of the NEXT call:

* `subscription_take_sequence_count` returns
  `-static_cast<int32_t>(NROS_RMW_RET_BUFFER_TOO_SMALL)` — negated, per issue
  0773, because this entry point's success value is a count;
* `subscription_take` returns it unnegated, because it can return a status
  inline. Both check, so a caller that mixes the two entry points hears it
  either way — which is the case the fix would have missed if only the sequence
  path had the check.

**Runtime fallback** (`CffiSubscriber`, `cffi/src/lib.rs`).
`pending_status: Option<TransportError>`, with the rule the issue asked for:

```rust
Err(e) => {
    if count == 0 { return Err(e); }   // nothing delivered — no count to protect
    self.pending_status = Some(e);     // park it; the slots already written are real
    break;
}
```

and `pending_status.take()` before any new take. So both paths now emit the same
SEQUENCE of answers, which is what makes `rmw_vtable.h`'s "identical observable
behaviour" claim true instead of something to delete.

## Tests, and the mutation that proves they measure

* `nros-rmw-cyclonedds/tests/take_sequence_pending_status.cpp`
* `nros-rmw-cffi/tests/try_recv_sequence.rs` — six cases, four of them 0971's:
  two messages then an oversized sample then empty; the fallback loop with a
  backend whose third take does not fit; the parked status delivered next call;
  and the `count == 0` arm that returns immediately.

Reverting the fallback's `Err` arm to the pre-0971 `return Err(e)` — the shape
that discarded the count — fails
`try_recv_sequence_fallback_parks_the_status` and nothing else. The tests
distinguish the fix from its absence rather than merely compiling against it.

## What this issue is worth remembering for

Not the flag, which is small, but the two corrections it made to itself while
open:

1. It first claimed `subscription_take` (single) had the same defect. It does
   not. Consume-then-refuse is the DESIGN — live zenoh does the identical thing
   deliberately, and `nros-verification`'s `no_silent_truncation` proves the
   invariant is "no stuck subscription and no silent truncation", not "the
   message survives".
2. The dead `err < 0` branch it found was ALSO the wrong fix. Making it
   reachable would have traded a silent drop for a contract violation, since
   erroring out of a partial drain is exactly what `rmw_vtable.h` forbids.

Both corrections narrowed the issue before anyone implemented it. That is the
issue working as intended.
