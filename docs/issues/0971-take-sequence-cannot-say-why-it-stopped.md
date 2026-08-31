---
id: 971
title: "`take_sequence` cannot say why a drain stopped, its two implementations disagree about it, and the message that stopped it is consumed and lost"
status: open
area: [rmw, api]
severity: high
related: [0969, 0773, phase-124, phase-376]
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

`CffiSubscriber::try_recv_sequence`'s loop (`packages/rmw/cffi/src/lib.rs`):

```rust
match self.try_recv_raw(slot)? {
    Some(len) => { out_lens[i] = len; count += 1; }
    None => break,
}
```

`try_recv_raw` returns `BUFFER_TOO_SMALL` for the same message, and `?`
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

## The same root, one level down

`subscription_take` (single) has the same consume-then-refuse ordering:

```cpp
const uint32_t total = ddsi_serdata_size(d);
if (buf_len < total) {
    ddsi_serdata_unref(d);
    return NROS_RMW_RET_BUFFER_TOO_SMALL;
}
```

The caller at least gets a distinct status here, so this is materially better
than the sequence path — but the message is equally gone, and a caller that
enlarges its buffer and retries receives the *next* message, not the one it was
told about. Also pre-existing: the pre-0969 body freed its typed sample and
returned the same status. Worth fixing with the same change rather than
separately, because the ordering problem is identical: **the size is only
knowable after the take consumes the sample.**

## Why this is not a one-line fix

Every obvious option loses something:

1. **Return negated `BUFFER_TOO_SMALL` from the batch.** Contract-forbidden, and
   worse in practice — the caller loses the count for messages that were
   delivered fine.
2. **Skip the oversized sample and keep draining.** Delivers more, still
   destroys the message silently. Trades one silent loss for a quieter one.
3. **Report the count AND an overflow signal.** Correct, and needs a signature
   change: an out-parameter, or a documented convention that `out_lens[count]`
   carries the length that did not fit. The ABI slot is already
   `(…, size_t *out_lens, size_t *taken)` returning `rmw_ret_t`, so there is
   room for a third out-parameter, at the cost of an `abi_version` bump.
4. **Size before consuming.** `dds_readcdr` peeks without removing, so the
   backend could size the head sample, refuse it while leaving it in the cache,
   and let a caller with a bigger buffer actually retry. Two passes per sample
   on the batch path, and only Cyclone can do it — the fallback cannot peek
   through `try_recv_raw`.

(3) and (4) are complementary rather than alternative: (3) tells the caller what
happened, (4) makes the message survivable. Both change the contract, which is
why this is filed rather than fixed.

## What a fix must satisfy

* A caller can distinguish "reader drained" from "a message did not fit", on
  BOTH the native and the fallback path, with the same answer.
* Whatever the doc claims about the fallback being observationally identical is
  either made true or deleted.
* Messages already written are never lost to the reporting of a later failure.
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
