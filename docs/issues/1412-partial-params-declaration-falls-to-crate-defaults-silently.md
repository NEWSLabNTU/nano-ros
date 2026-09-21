---
id: 1412
title: "Removing one node's params: block flips the whole store to refused, and
  the crate defaults 32/64/256 replace the derived 25/35/0 with no build-time
  line"
status: open
type: bug
area: [cmake, build, sizing]
severity: medium
found: 2026-09-21
related: [issue-1352, phase-446, phase-460]
---

## What was observed

Brief D experiment E6a on the Autoware Safety Island: `stop_mode_operator`'s
`params:` block deleted from the contract. `play_launch check` 0 errors; the
model resolves. The inventory flips:

```
< set(NROS_PARAM_DECLARATION_STATUS "declared")
< set(NROS_DERIVED_MAX_PARAMETERS 25)
< set(NROS_DERIVED_MAX_PARAM_NAME_LEN 35)
< set(NROS_DERIVED_MAX_STRING_VALUE_LEN 0)
> set(NROS_PARAM_DECLARATION_STATUS "refused")
> set(NROS_PARAM_DECLARATION_REASON "1 of 4 nodes in this image declare no `params:` ...")
```

`codegen-system` rc 0, `codegen entry` rc 0, the configure continues, the
image builds. The parameter store is sized by
`packages/core/nros-params/build.rs` defaults: 32 slots, 64-byte names,
string 256, array 32, byte-array 256 - up from 25/35/0/0/0. The declared-params
header shrinks from 21 to 18 rows, so `stop_mode_operator`'s
`declare_parameter` calls are no longer checked against anything.

## The chain (verified at 783cdfa14)

* `ws entity-inventory` writes the status and the reason
  (`packages/cli/nros-cli-core/src/entity_inventory.rs:3993`); the
  every-node-or-none rule is phase-446's and is right - sizing from the nodes
  that declared would give the rest no slots.
* `cmake/NanoRosEntityFacts.cmake:622`:
  `if(NOT NROS_PARAM_DECLARATION_STATUS STREQUAL "declared") return()`. No
  message. The `NROS_DECLARED_*` carriers are simply not exported.
* `nros-params/build.rs:169-170`: `stated(name, rung).or(declared).unwrap_or(default)`.
  With nothing stated and nothing declared, the default wins, silently.

The reason string exists and is written to a file nobody reads for that
purpose. The inventory did its job and the next layer treated "refused" as
"nothing to say".

## Why it matters

The parameter STORE moves the image by 0 bytes on the island (measured,
section 8 of its deployment report), so the size is not the damage. The
capacities are: string 0 -> 256 and array 0 -> 32 change the shape of every
`set_parameters` request the parameter SERVICES accept, which is the class
issue 1352 measured (a 2,408-byte request dropped silently by a 1,024-byte
slot). And the declared-params header losing a node's rows removes the
`DECLARED_PARAM_MISMATCH` check for that node only, which is the quietest
possible way to lose a check.

## What would fix it

phase-460 W2. `absent` (no node declares) keeps today's meaning. `refused`
becomes a configure-time `FATAL_ERROR` in `NanoRosEntityFacts.cmake` quoting
`NROS_PARAM_DECLARATION_REASON`, and `nros-params/build.rs` refuses the same
way on the cargo road when the inventory carries the status. Nothing about the
every-node-or-none rule changes; what changes is that breaking it is heard.

## Acceptance

A test inventory with `refused` fails the configure naming the node; `absent`
and `declared` inventories pass byte-identically to today.
