---
id: 567
title: "zenoh-pico's unicast read resets its receive buffer every call, so it cannot return with bytes unread — which makes any drain budget lossy"
status: resolved
resolved_in: "zenoh-pico fork 43ddb0ec — reset only when the buffer is empty"
type: enhancement
area: rmw
related: [issue-0506]
---

## The constraint

`_zp_unicast_read` (`zenoh-pico/src/transport/unicast/read.c`) starts
every non-`single_read` pass with:

```c
// Prepare buffer
_z_zbuf_reset(&ztu->_common._zbuf);
```

so anything left in the receive buffer when the function returns is
discarded. That is why the inner loop below it drains *every* complete
frame it can see, with a comment recording the defect that motivated it:
one `recv` can pull several stream frames, and "a frame left here is
silently lost" (declares and interest replies vanished on the polled
multi-executor path).

The loop is therefore unbounded by construction: it cannot stop early
without losing data.

## Why it matters

Issue #0506 needs to bound that loop. Under a sustained inbound flood the
read task holds the CPU for 100-340 ms at a stretch — above every
application tier, since the transport band sits above them by design —
and a budget on the drain is the device-side half of the proposed fix
(the router-side half cannot be enforced by the device itself).

Measured on the FreeRTOS mps2-an385 lane at a 2 kHz flood, capping the
loop at 4 and 16 frames:

| cell | stalls >50 ms | miss >15 ms | inbound rx/s | chain delivered |
|---|---|---|---|---|
| unbounded (today) | 10 | 1.79% | 282 | 13.2% |
| cap = 16 frames | 4 | 0.59% | **10** | **5.7%** |
| cap = 4 frames | 5 | 0.85% | **10** | **5.4%** |

Cadence improves — and it improves *because messages are being thrown
away*. The drain collapsing to 10 msg/s is the reset above discarding
whatever the cap declined to read. A frame cap here is a drop policy, not
a budget.

(Control: a cap of 1 degenerates to the pre-loop single-frame path, where
the outer read task simply re-enters; it matches unbounded on every
column — 12 stalls, 268 rx/s, 13.2% chain.)

## Resolution (2026-08-14)

The reset is now conditional (zenoh-pico fork `43ddb0ec`):

```c
if (_z_zbuf_len(&ztu->_common._zbuf) == 0) {
    _z_zbuf_reset(&ztu->_common._zbuf);
} else {
    _z_zbuf_compact(&ztu->_common._zbuf);
}
```

Smaller than expected, because `_z_unicast_client_read` was already
written to cope with pre-existing buffered bytes — it only `recv()`s when
it holds fewer than the length prefix, and rewinds its read position on
an incomplete frame. The unconditional reset was the only thing
preventing resumption.

**Verified: the budget is no longer lossy.** With a 16-frame cap on the
drain loop, inbound delivery holds at **274 msg/s** where the identical
cap before this change collapsed it to **10**. No regression at normal
load: idle and 1 kHz runs give ctrl 10.000 ms mean, chain at its ~29.5%
sampling ceiling, failover unchanged (94-95 emergency gate ticks).

### What this does NOT establish

That a drain budget actually buys cadence. On the resumable path the
two cells point in opposite directions — a 16-frame cap left stalls
unchanged (14 vs 10 baseline) while preserving delivery, and a 4-frame
cap improved stalls (7) while delivery collapsed again, this time by
TCP backpressure rather than discard (never recv()ing while the buffer
stays non-empty).

Worse, second repetitions of both cells landed in a degraded regime
(rx 10 msg/s, chain 1.4-3.3%) that also appeared in unrelated cells
earlier in the session, so the 2 kHz harness is bimodal and n=1 cells
there are not evidence. Establishing the budget's cadence effect needs a
harness that reliably reproduces the loaded regime — the router-pacing
probe's cells were stable, this one's are not.

So: #0506's device-side half is now *implementable*. Whether it is
*worth implementing* is still open, and router-side pacing remains the
only mechanism measured to fix both harms.

## What would have to change (as filed)

`_zp_unicast_read` needs to be resumable: return with bytes still in
`_zbuf` and continue from them on the next call, instead of resetting.
Sketch of the requirements, not a design:

- Reset only when the buffer is genuinely drained (`_z_zbuf_len <
  _Z_MSG_LEN_ENC_SIZE` after compaction), rather than unconditionally on
  entry.
- Keep `_z_zbuf_compact`'s invariants intact across the early return —
  the read position bookkeeping the partial-frame path already does is
  the model.
- The `single_read` path is unaffected; it processes exactly one message
  and has no inner loop.

This is a change to a vendored third-party submodule, so it also needs a
decision about whether it lands as a local patch or goes upstream to
zenoh-pico first.

## Status

Blocking the device-side half of #0506. The router-side half
(rate+burst pacing at the router) is unaffected and is currently the only
mechanism measured to fix both the cadence and the chain-delivery harm.
