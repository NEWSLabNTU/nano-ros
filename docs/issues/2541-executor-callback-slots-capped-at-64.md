---
id: 2541
title: "An executor holds at most 64 callbacks, because its ready bits are one u64"
status: open
type: limitation
severity: low
area: [executor, runtime]
related: [rfc-0084, rfc-0108, phase-555]
found: 2026-11-14
---

## What

`MAX_CALLBACK_SLOTS` is 64 (`packages/core/nros-node/src/executor/spin.rs`).
The executor carries a callback's index in a `u64` active mask
(`1u64 << i`), RFC-0084's pushed arrival bits are one `u64` per executor
(`take_entity_ready_bits`), and both ready sets of RFC-0108 assert
`N <= 64` (`FifoReadySet`, `EdfReadySet` presence bitmap,
`executor/ready_set/mod.rs`). So one executor can register at most 64
callbacks, whatever the per-entry sizing says.

RFC-0084 left this as its open question 3: widening to `BitSet<N>` needs a
lock-free multi-word set-bit (or a per-word atomic array) on the arrival
path, which runs in a backend callback or an ISR.

## Why it is filed now

Phase-555 W3 flipped RFC-0084 to Stable, and the rule for that flip is that
no open question stays only in the RFC. No image in the tree is near 64
today; the cap is a ceiling, not a measured failure.

## Acceptance

Either an executor with more than 64 callbacks registers, dispatches in rank
order and receives pushed arrivals for all of them, measured; or the cap is
recorded as the design (a refusal at registration that names the limit, and
a sentence in RFC-0084) and this issue is closed as wontfix.

## Worse than filed: the cap was not enforced (2026-11-19)

Nothing held a table to 64. `NROS_EXECUTOR_MAX_CBS` had no upper bound,
`ExecutorSizing::cbs` was not checked, and only the levels sizing path
clamped. So a table sized past 64 ACCEPTED a 65th callback, and its dispatch
shifted `1u64 << 64`: a panic in a debug build ("attempt to shift left with
overflow", `spin.rs`), and in a release build the bit of slot 0, i.e. a
wrong dispatch with no diagnostic. Reproduced: 70 timers on a 70-slot
executor.

First step, the refusal (this issue stays open for the second):
- `config::CALLBACK_SLOT_CAPACITY` (64) is the one number;
  `spin::MAX_CALLBACK_SLOTS` reads it.
- An image whose `MAX_CBS`, or whose sched tasks' callbacks together, exceed
  it fails to compile, saying which.
- `storage::callback_slots` caps every table at it, for the layout and the
  carve alike. The 65th registration is therefore the full table's
  `NodeError::ExecutorFull`, which names `NROS_EXECUTOR_MAX_CBS`, and no loop
  over the entries can reach an index the masks cannot hold.
- Test `an_executor_sized_past_64_callbacks_refuses_the_65th`: a 70-slot
  sizing takes 64 timers, refuses the 65th by name, and dispatches all 64.

Second step: a multi-word slot mask sized from the image, which raises the
cap. Its acceptance is this issue's.
