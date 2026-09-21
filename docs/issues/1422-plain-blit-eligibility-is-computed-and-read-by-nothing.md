---
id: 1422
title: "The plain (POD-blit, zero-copy eligible) flag is computed on every size
  bound and read by a test and a const - no dispatch path and no gate consumes
  it"
status: open
type: tech-debt
area: [codegen, memory, sizing]
severity: low
found: 2026-09-21
related: [issue-1368, issue-1400, issue-0814, phase-460, rfc-0033]
---

## What exists (verified at 783cdfa14)

* `packages/cli/rosidl-lower/src/lowered.rs:211` - `LoweredType::plain`,
  documented as "POD-blit eligible: every field plain AND all fields share
  one alignment", the property "a blit fast path would ask".
* `packages/core/nros-serdes/src/size.rs:59` - `SizeBound.plain`, computed by
  `size_bound` for every type at every encoding version, with the fixed-array
  and nested rules at `:132-143`.

## Who reads it

`grep -rn '\.plain\b' packages` outside the two producers finds
`packages/core/nros-serdes/src/schema.rs:171` (a per-type const) and
`packages/testing/nros-tests/tests/serialized_size_bound.rs:207` (a test
asserting the bound is version-independent when plain). No arena dispatch, no
inventory fact, no cmake carrier, no gate. The island's deployment report
(section 4 and its section 10 gap list) records the same: "computed on every
bound and consumed by nothing".

## Why file it

A computed fact with no reader is the shape issue 0196 and issue 1233 keep
finding one layer down: it costs nothing today and it is the first thing a
later change will silently break, because no test of a consumer will go red.
It also reads, in the size-bound docs, as though a fast path existed. Issue
1400 already found the loan API's value claim false on three backends; a
"zero-copy eligible" flag beside it invites the same misreading.

## What would fix it

phase-460 W3, second half: give it one consumer or delete it.

1. The inventory emits `NROS_ENTITY_PLAIN_TYPES`, the subscribed types that
   are blit-eligible.
2. The arena's borrowed-view dispatch (`packages/core/nros-node/src/executor/arena.rs`,
   zero-copy tests from line 3645) selects per type from that list rather
   than by a runtime probe.
3. Measure on one in-tree image. If no dispatch differs, delete the flag from
   both producers and the const, and say so in the wave.

## Acceptance

Either a dispatch test that changes behaviour with the flag flipped, or the
flag gone. Not the status quo.
