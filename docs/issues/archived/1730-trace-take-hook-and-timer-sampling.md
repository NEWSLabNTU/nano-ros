---
id: 1730
title: "The callback trace had no per-sample take event and no way to thin a
  timer's ticks, so the safety island hand-placed both (phase-474 I3, F4)"
status: resolved
type: enhancement
area: [core, tracing]
severity: low
found: 2026-10-06
related: [phase-474, phase-8]
resolved_in: "phase-474 I3 (feat/474-rest)"
---

## What was missing

The island's trace kept no take marker for `kinematic_state`,
`operation_mode_state` and `control_cmd`, so the link hop of those inputs
was not measured, and its emergency operator's 30 Hz ticks were kept one in
ten by hand to fit a 32 KiB RAM trace window, so their jitter was not
measured (island `docs/takeover-trace.md` section 10, F4). nano-ros's
callback trace (`executor/callback_trace.rs`) emitted register/name and a
start/end pair per dispatch and nothing per sample.

## Resolution

Marker ids, appended to the block (16-24 unchanged, stable):

| id | event | arg |
| --- | --- | --- |
| 25 | take | `handle << 24 \| take seq` (image-wide, low 24 bits) |
| 26 | take stamp sec | the sample's `stamp.sec` |
| 27 | take stamp nanosec | the sample's `stamp.nanosec` |

- A take is emitted for every sample a subscription dispatches, before the
  callback's start (18), at all 19 subscription dispatch sites in `arena.rs`
  (typed, raw, C, in-place, info, safety, LET). Opt-in: per slot with
  `callback_trace::set_take_trace(handle, on, stamp_offset)` /
  `nros_trace_set_take(handle, on, stamp_offset)` (C), or image-wide with
  `CONFIG_NROS_TRACE_TAKES` / `NROS_TRACE_TAKES`.
- 26/27 follow when the stamp's place is known: the type's own
  `RosMessage::STAMP_OFFSET` on the typed Rust paths, or the offset the slot
  setting names (the C and C++ paths carry no type; 4 for a type that starts
  with a `Header` or a `Time stamp`, which covers the island's Odometry,
  OperationModeState and Control).
- A timer's start/end pair is kept for one tick in N: per slot with
  `set_timer_trace_every(handle, n)` / `nros_trace_set_timer_every` (C), or
  image-wide with `CONFIG_NROS_TRACE_TIMER_EVERY` (default 1 = every tick,
  0 = none). Other dispatch kinds are always traced.

The per-slot settings live in one image-wide table keyed by the slot index
the events carry (256 x 3 small atomics, only with `trace-callbacks`).

Tests: `i3_an_opted_in_subscription_traces_each_take_and_its_stamp`,
`i3_a_thinned_timer_traces_one_tick_in_n` (nros-node, `trace-callbacks`).
Not run on the board.
