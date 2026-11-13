---
id: 2504
title: "An unseeded configure on a fresh build dir needs three passes, not the two its remedy text promises"
status: open
type: bug
severity: low
area: [build, cmake, cli]
found: 2026-11-13
related: [2503]
---

## What

Measured on 2026-11-13 (main `8b07c45633`, Zephyr rows
`build-c-talker-cyclonedds` and `build-nucleo-f767zi-c-talker-serial`, fresh
build dirs, a configure left unseeded by issue 2503):

- pass 1: FATAL "entity inventory ... was composed after this configure's
  readers ran" (and the same for the message-bound sizes); the remedy text
  says the fragment now holds this configure's answer, so the next
  configure reads it;
- pass 2: FATAL "message-bound sizes changed after this configure's readers
  ran": pass 1 wrote "No knob is derived", pass 2 wrote basis "subscribed"
  with payload status derived;
- pass 3: green.

## Why

The message-bound composition reads the entity-inventory fragment the
PREVIOUS configure wrote. On pass 1 there is none, so the message-bound
answer is composed against nothing; on pass 2 the inventory exists and the
message-bound answer moves, which trips the promise check once more. The
check's remedy ("the next configure reads it") is true for each fragment
alone and false for the pair.

## Fix direction

Compose the message-bound fragment from this configure's own inventory
answer (order the two compositions), or have the promise check treat a
fragment that moved only because an upstream fragment it reads was absent
as converging. A seeded configure (`nros configure` first) is unaffected.

## Acceptance

An unseeded configure on a fresh build dir converges in at most two passes,
as the remedy text says; a test covers the pair.
