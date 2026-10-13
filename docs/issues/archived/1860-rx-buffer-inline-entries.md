---
id: 1860
title: "Four Rust subscription entries and the standalone `Subscription` still hold an inline `[u8; RX_BUF]`, sized to the image's largest type rather than their own"
status: resolved
type: enhancement
area: core
related: [phase-403, phase-408, phase-392, phase-498, issue-1179, issue-1595]
---

**Home: [phase-498](../../roadmap/archived/phase-498-rx-sizing-residue.md)**, carved out of
phase-403 when it was archived on 2026-10-13.

## What is still inline

Phase-403 set out to size every receive buffer from its message type. Two of
its three roads got there, by a different design from the one it wrote down:

- **Rust, buffered executor path.** `register_subscription_buffered_on`
  (`packages/core/nros-node/src/executor/spin.rs`) takes the slot size as a
  runtime `usize`. Leaving it unset derives it from `M` (phase-392 W3c). A
  compile-time size from `M` (`Sub<{ M::BOUND }>`) does not compile on
  stable Rust, so this was the only way the per-type size could be
  expressed at all.
- **C.** `rclc_executor_add_subscription_with_context` passes the type
  support's `serialized_size_max` as the slot size, and the payload lives in
  the arena's trailing region (`SubBufferedRawInfoCEntry`, phase-408 W5b). The
  caller owns the message storage, as in rclc. Phase-403 W3's
  caller-supplied-buffer design, with its lifetime crossing the C boundary,
  was never needed.

These entries still carry `buffer: [u8; RX_BUF]` inline:

| entry | where |
| --- | --- |
| `SubInfoEntry<M, F, RX_BUF>` | `executor/arena.rs` |
| `SubSafetyEntry<M, F, RX_BUF>` | `executor/arena.rs` |
| `SubBufferedRawInfoEntry<F, RX_BUF>` | `executor/arena.rs` |
| `SubBufferedRawSafetyEntry<F, RX_BUF>` | `executor/arena.rs` |
| `Subscription<M, RX_BUF>` (standalone, not executor-owned) | `executor/handles.rs` |

`RX_BUF` defaults to `DEFAULT_RX_BUF_SIZE`. Since issue 1595 that is the
image's closure maximum, the largest declared message type, taken from the
sizing descriptor's `[types] max_wire_bound_bytes`. It falls back to 1,024
only when some type in the closure states no bound. So each of these entries
reserves the largest type's bytes, whatever its own type is. That is
bounded, not unbounded, but it is the over-reservation phase-403 existed to
remove.

## Fix shape

Move the four arena entries onto the trailing region, as
`SubBufferedRawInfoCEntry` already is: a `TrailingBuf` sized at registration,
from `M` when the caller names no size. Nothing here needs a design decision.
The C entry is the pattern, and the runtime-`usize` route is the one stable
Rust allows. The standalone `Subscription<M, RX_BUF>` owns its buffer outside
any arena. It keeps the const generic, and `nros::rx_buffer_for!(M)` at the
call site remains its per-type form.

## Acceptance

- The four entries hold no `[u8; RX_BUF]`. A test registers a subscription of
  a small type in an image whose closure maximum is large, and checks the
  slot it was charged.
- Phase-403's owed measurement: a `just mem-report --baseline` before/after on
  an image that runs the changed path, naming the image. Phase-403's rule was
  that no wave is done without one, and phase-408's archive left the same
  number owed.

## Resolution (2026-10-14, phase-498 W1 and W2)

The four entries hold a `TrailingBuf` in the arena's trailing region, with
no inline `[u8; RX_BUF]`, and they and their dispatch functions dropped the
`RX_BUF` const generic.

- **Typed (`.message_info()`, `.safety()`):** the slot is sized by
  `rmw_type_registry::subscription_slot_bytes`, the one rule the buffered
  path also uses now. A size named with `.rx_buffer::<N>()` is used
  verbatim, else the type's bound, else `RX_BUF`. Both builders carry
  `rx_explicit`, as the buffered builder does.
- **Raw info and raw safety:** they have no type, so they still claim
  `RX_BUF`, now in the trailing region.
- **Test:** `a_message_info_subscription_is_sized_from_its_type`.

Measured on native zenoh: arena claimed after registration, a temporary
`arena_used()` print that was not committed.

| image | before | after |
| --- | --- | --- |
| `bins/safety-chatter-listener` (`.typed::<Int32>().safety()`, no `system.toml`, so `RX_BUF` = 1,024) | 1,736 B | 744 B |
| `bins/int32-sink` (plain typed, closure only `Int32`) | 712 B | 712 B |

`int32-sink` does not move because its RX_BUF was already the closure
maximum, which is `Int32` (issue 1595). The saving lands where a
subscription's type is smaller than the image's largest. Static RAM
(`mem-report`) is unchanged by construction, because the executor backing
is sized at build time.
