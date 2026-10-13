# Phase 498 -- the last receive buffers that are not sized from their type

**Status (2026-10-14). CLOSED and archived: W1, W2 and W3 landed together.**
W3 (issue 1179, option (a)) puts each generated type's bound on `RosMessage`,
so default receive buffers are sized per type on every backend. W1 (issue
1860) moves the four inline-buffer entries onto the trailing region. W2
measured it: a native zenoh `Int32` safety listener claimed 1,736 B of arena
before and 744 B after. Details are in the two issues.

**Status (2026-10-13). Opened; nothing landed.** Carved out of
[phase-403](phase-403-type-bound-rx-sizing.md) when it was archived. 403's
goal (every receive buffer sized from its message type) is met on the buffered
Rust path and on the C path. What is left is the residue below, plus the
measurement 403 never took.

## Why a phase and not only an issue

Phase-403 archived with one open issue homed in it (1179), and its residue
became a second (1860). A finished phase cannot own open work, and no open
phase covers receive-buffer sizing: phase-394 coordinates the memory campaign
but owns no design, and phase-495 is the scheduler's mapper input. So the
two issues need a home of their own.

## Work items

### W1 -- the four inline-buffer entries move onto the trailing region (issue 1860)

`SubInfoEntry`, `SubSafetyEntry`, `SubBufferedRawInfoEntry` and
`SubBufferedRawSafetyEntry` (`packages/core/nros-node/src/executor/arena.rs`)
still hold `[u8; RX_BUF]` inline. Each reserves the image's closure maximum
whatever its own type is. Give each a `TrailingBuf` sized at registration,
as `SubBufferedRawInfoCEntry` already has (phase-408 W5b), derived from `M`
when the caller states no size, as the buffered path does (phase-392 W3c).
The standalone `Subscription<M, RX_BUF>` is out of scope: it owns its buffer
outside any arena, and `nros::rx_buffer_for!(M)` at the call site is its
per-type form.

*Acceptance:* the four entries hold no inline buffer, and a test charges a
small-type subscription its own bound in an image whose closure maximum is
larger.

### W2 -- the measurement phase-403 owed

A `just mem-report --baseline` before/after on an image that runs the changed
code, naming the image. 403's rule was that no wave is done without one, and
phase-408's archive left the same number owed. Take it before W1 and after,
so W1 gets its own number too.

*Acceptance:* the two reports, and the image's name, recorded in this document.

### W3 -- derived defaults on the backends that carry no schema (issue 1179)

The buffered path derives a slot from `M` only where `MessageForRmw` carries
a schema. That is only on backends that declare `type-descriptors`, so on
zenoh and XRCE an unset size still falls back to `RX_BUF`. Issue 1179 has
the evidence and the options.

*Acceptance:* issue 1179's own.

## Issues homed here

| issue | why it belongs here |
| --- | --- |
| [#1860](../../issues/archived/1860-rx-buffer-inline-entries.md) | W1 and W2: the inline entries and the owed measurement |
| [#1179](../../issues/archived/1179-derived-rx-default-unreachable-without-schema.md) | W3: derived-by-default sizing reaches only backends that carry a schema. Re-homed from phase-403 |
