---
id: 1421
title: "Removing one node's params: block flips the whole store to refused, and
  the crate defaults 32/64/256 replace the derived 25/35/0 with no build-time
  line"
status: resolved
resolved_in: phase-460 W2
type: bug
area: [cmake, build, sizing]
severity: medium
found: 2026-09-21
related: [issue-1352, phase-446, phase-460]
---

# 1421 -- a refused `params:` declaration was sized around, not heard

Digest; the investigation is in the phase-460 doc (W2) and the fixing commit.

## What was observed

Brief D experiment E6a on the Autoware Safety Island: one node's `params:`
block deleted from the contract. `play_launch check` 0 errors, the model
resolves, and the entity inventory flips the store from `declared`
(25/35/0/0/0) to `NROS_PARAM_DECLARATION_STATUS "refused"` with a reason
naming the node. `codegen-system` rc 0, `codegen entry` rc 0, the configure
continues, the image builds with the parameter store on nros-params' crate
defaults (32/64/256/32/256), and the node whose block is missing drops out of
the declared-params header, losing its `DECLARED_PARAM_MISMATCH` check alone.

## The chain (verified at 783cdfa14)

`_nros_param_store_env` (`cmake/NanoRosEntityFacts.cmake`) did
`if(NOT NROS_PARAM_DECLARATION_STATUS STREQUAL "declared") return()` with no
message, so no `NROS_DECLARED_*` reached the crate and
`nros-params/build.rs`' `knob()` fell to `default`. The reason string was
written to a file nobody read for that purpose.

## What landed (phase-460 W2)

* `_nros_param_store_env` is a configure-time `FATAL_ERROR` on `refused`,
  quoting `NROS_PARAM_DECLARATION_REASON` and the fragment path. `absent`
  (no node declares; the board sizes the store) and `declared` are unchanged,
  and a fragment with no status still carries nothing.
* `nros-params/build.rs` refuses the same way when a road hands it
  `NROS_PARAM_DECLARATION_STATUS=refused`, before any number is read. The
  Zephyr resolver road (`nros_cargo_build.cmake`) forwards the five derived
  numbers and not the status; forwarding it is two lines beside them, owned
  by a later wave.

Acceptance: `tests/cmake-entity-inventory-tests.sh` -- `refused` fails the
configure naming the node; `absent` and `declared` pass unchanged (77
assertions; origin/main's carrier fails the six `refused` assertions and
passes the rest).
