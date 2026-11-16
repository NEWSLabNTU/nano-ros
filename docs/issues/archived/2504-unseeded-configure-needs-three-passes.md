---
id: 2504
title: "An unseeded configure on a fresh build dir needs three passes, not the two its remedy text promises"
status: resolved
resolved: 2026-11-17
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

## Resolution (2026-11-17)

Fixed by ordering the two compositions. Codegen records where it wrote the
message-bound sizes and which entity fragment it composed them over; both
entity composers (`nano_ros_entry()` and the deferred
`nano_ros_node_register()` composer) call
`nros_entity_inventory_recompose_bounds` right after writing the inventory and
before their promise check, which re-derives the bounds over the file just
written. One configure therefore leaves both fragments at its final answer.
The end-of-configure promise check, which compares each registered fragment
with what its readers read, still holds the re-derived bounds to the seed on a
seeded configure.

Measured on `examples/zephyr/c/talker` image `nucleo_serial`, a fresh west
build dir, with issue 2503's leftover bare-key stamp planted so the configure
reads no seed:

- main `a500ed909b`: pass 1 refused (the two halves disagree), pass 2
  refused ("message-bound sizes changed after this configure's readers
  ran"), pass 3 green;
- with the fix: pass 1 refused, pass 2 green.

Test: `tests/cmake-entity-inventory-tests.sh` case K (the helper re-derives
over the inventory just written and only then; both composers order derive,
re-derive, promise check). Deleting the node-register call fails it.
