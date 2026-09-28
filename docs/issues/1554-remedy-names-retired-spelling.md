---
id: 1554
title: "Three cmake remedies told the reader to write `nano_ros_node_register(... ENTITIES ...)`, which had raised FATAL_ERROR for 24 days — no gate covers a remedy that names a retired spelling"
status: open
type: tech-debt
area: [build, tooling]
severity: low
found: 2026-09-29
related: [1226, 0196, phase-412, phase-454]
---

## What

phase-412 retired the `ENTITIES` argument of `nano_ros_node_register` on
2026-09-05 and made it raise `FATAL_ERROR`. Three user-facing `message()`
strings kept telling the reader to write it, and were still doing so on
2026-09-29 when the keyword left the grammar:

| site | what it said |
| --- | --- |
| `cmake/NanoRosEntityInventory.cmake` (the broken-declaration `FATAL_ERROR`) | "Fix the `ENTITIES` argument of the named nano_ros_node_register()." |
| `cmake/NanoRosMessageBounds.cmake` (the upper-bound `message(STATUS)`) | "Declare `nano_ros_node_register(... ENTITIES sub:<pkg>/msg/<Name> ...)` to narrow them" |
| `cmake/NanoRosMessageBounds.cmake` (the same advice, written into the generated knobs file as a comment) | "... on every component to narrow them." |

Following any of them lands on the tombstone. The first is the worse one: it is
the remedy attached to a FATAL_ERROR, so it is read exactly when the reader is
already stuck.

All three are fixed in the wave that filed this. **The gap is that nothing
caught them**, and the next retirement will land the same way.

## Why no gate caught it

`scripts/check/check-knob-single-reader.py` does carry an ENTITIES retirement,
and its reach is CORRECT for what it claims: it forbids the PRODUCER
(`_entities_field`, a `"entities":` key in emitted JSON), not the spelling. A
gate that forbade the literal `ENTITIES` in cmake would fire on the tombstone
itself, which is the one place that must name it.

So this is not issue 0196's narrow-reach shape. It is an unchecked class: a
retirement ledger knows the retired spelling and knows what resolves it now, and
nothing joins that to the strings the build prints at a user.

## Fix direction (not decided)

The ledger entry already has `what` and `resolves_now`. Give it one more field —
the single function allowed to name the retired spelling in a user-facing
string, i.e. its tombstone — and check that no other `message(...)` argument in
cmake names it. `#` comments are out of scope (`strip_comments_hash` already
drops them, and prose explaining a removal legitimately names it); the subject
is the STRING ARGUMENTS of `message()`, which is what a reader is shown.

Two things to get right, neither measured yet:

- the exemption must be the tombstone FUNCTION, not the tombstone FILE. The
  existing entry's comment records why: an earlier draft exempted
  `cmake/NanoRosNodeRegister.cmake` wholesale and put a blind spot in the one
  file the producer lived in, so re-adding the real thing there would have
  passed.
- a generated-file comment (the `NanoRosMessageBounds` knobs-file case) is a
  `#` comment in its OUTPUT but a `message`/`string(APPEND)` string argument in
  its SOURCE. It is user-facing and must be in scope, so "skip `#` comments"
  has to mean the cmake file's own comments, not a `#` inside a string literal.

## Acceptance

Planting `ENTITIES` into any `message()` string outside the tombstone function
fails the gate; the tombstone itself, and every `#` comment explaining the
retirement, pass.
