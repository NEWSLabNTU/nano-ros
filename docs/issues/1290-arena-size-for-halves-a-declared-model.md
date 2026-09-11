---
id: 1290
title: "`arena_size_for(cbs)` scales the arena by cbs / MAX_CBS, which is not
  the model once an image declares its entities -- the bench's only executor
  got half of what it needs"
status: open
type: bug
area: executor, build
severity: medium
found: 2026-09-11
related: [phase-412, issue-1255, issue-1036, phase-271]
---

## What happens

`nros_node::config::arena_size_for(cbs)` sizes a per-entry executor's arena as
`ARENA_SIZE * cbs / MAX_CBS`, floored at `ARENA_SIZE / MAX_CBS`. Its doc says
why: "the same per-slot arena budget the global default used". That was true
while `ARENA_SIZE` WAS a per-slot budget -- `max_cbs * worst_case_entry + base`.

Since phase-403 step 3 it is not, on an image that declares its entities:
`nros-node/build.rs` then sums the model per KIND (subscriptions, timers,
services, actions, plus a base overhead) and `MAX_CBS` is a separate knob. So
`ARENA_SIZE / MAX_CBS` has no meaning, and scaling it by `cbs` can hand the
image's ONE executor a fraction of what that image's own entities need.

## Measured

`packages/testing/nros-bench/large-msg-baremetal` (mps2-an385, bare metal)
sizes its only executor with `arena_size_for(2)`. Built with one declared
subscription (`NROS_ENTITY_COUNT_SUBSCRIPTION=1`, the others 0):

| | bytes |
| --- | ---: |
| `cargo:arena_size` (the derived default, = the model) | 14,424 |
| `arena_size_for(2)` with `MAX_CBS` = 4 | 7,212 |

Half. Before phase-412 item 4 this image would have built and died at the
subscription's registration with `NodeError::BufferTooSmall`. With it, the
bench's opt-in `EXEC_SIZING.assert_covers_model(..)` refuses the build instead:

    EXEC_SIZING.arena = 7212 B, but the entities this image declares are
    modelled at 14424 B (7212 B short; ...)

The bench declares no entities today, so nothing in the tree is broken yet.
The hosted board is the other caller: `nros-board-linux` opens
`Executor::open_sized` with `arena_size_for(cbs)` for every entry that states
or derives `max_callbacks`, and there `cbs` is a runtime value, so no build
can check it.

## Why it is not fixed with item 4

Two defensible fixes, and they pull in opposite directions:

* floor `arena_size_for` at `arena_model::REQUIRED` -- correct for an image's
  only executor, and wrong for one tier of a tiered boot, which holds a
  subset and would then carry the whole model once per tier;
* retire the scaling and size per-entry executors from a per-ENTRY model --
  which needs the entry's per-kind counts where the macro emits the sizing,
  and those do not reach it today.

Choosing is a design decision about per-entry sizing (phase-271's), not about
the oracle.

## Related

The per-kind counts reach `nros-node/build.rs` on the Zephyr lane only
(`zephyr/cmake/nros_cargo_build.cmake`). The native cmake road carries the
DECLARED facts (`NROS_DECLARED_EXECUTOR_MAX_CBS` and friends) but not
`NROS_ENTITY_COUNT_*`, and the cargo-leaf sidecar carries neither, so on those
roads `arena_model::REQUIRED` is 0 and every arena check is inert.
