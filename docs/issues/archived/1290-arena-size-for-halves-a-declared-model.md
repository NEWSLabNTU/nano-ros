---
id: 1290
title: "`arena_size_for(cbs)` scales the arena by cbs / MAX_CBS, which is not
  the model once an image declares its entities -- the bench's only executor
  got half of what it needs"
status: resolved
resolved_in: phase-448 W8
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

## Resolution (phase-448 W8)

`arena_size_for` asks the MODEL when the image declares one, and keeps the
ratio only for the undeclared case -- the first of the two options this issue
listed, taken for the reason the second could not be:

```rust
pub const fn arena_size_for(cbs: usize) -> usize {
    if arena_model::REQUIRED > 0 {
        return ARENA_SIZE;
    }
    // ... the cbs / MAX_CBS scaling, unchanged
}
```

`ARENA_SIZE` IS the model on that branch (the per-kind sum, floored), and
`executor::arena_oracle` already refuses at compile time to let a STATED
`NROS_EXECUTOR_ARENA_SIZE` sit below it -- so returning it is "ask the model",
not a second copy of the arithmetic.

**The tiered-boot cost is accepted and written down.** One tier now carries the
whole image's model rather than its own subset. That is over-provision, which
is the safe direction; the alternative needs the ENTRY's per-kind counts where
the macro emits the sizing, and they do not reach it. An entry that wants less
states its own `ExecutorSizing` and holds it with `assert_covers_model`, which
is a compile-time check the ratio never had.

**Nothing undeclared moves.** `REQUIRED` is 0 unless every `NROS_ENTITY_COUNT_*`
arrives, which is the Zephyr resolver road; `nros-board-linux`'s
`arena_size_for(cbs)` per entry is on the ratio exactly as before.

### The bench declares, and it was measured

`packages/testing/nros-bench/large-msg-baremetal/.cargo/config.toml` gained an
`[env]` block stating all five counts as ZERO -- `main()` creates one publisher
and registers no callbacks. Authored by hand because the cargo-LEAF road
carries no entity counts (this issue's "Related" says why), and cargo's `[env]`
is the one channel a cargo-only leaf has to a build script. Verified with
`cargo config get env`, then built for `thumbv7m-none-eabi` and measured with
`nm -S`:

| build | `EXEC_BACKING` |
| --- | ---: |
| undeclared (today's `main`) | 49,280 B |
| declared, OLD `cbs / MAX_CBS` arithmetic | 16,256 B |
| declared, this fix | **20,352 B** |

Two numbers in one table, and they are different facts. The declaration itself
is worth **-28,928 B** (an image that declares nothing budgets four worst-case
action-client slots). This FIX is worth **+4,096 B** against the old
arithmetic under that declaration -- the correction, paid in the direction that
stops the arena being below its own model.

### The positive control

`config::arena_size_for_tests::the_scaling_this_replaced_was_short_of_the_declared_model`
is `#[ignore]`d, because a `cargo test -p nros-node` declares nothing and the
test could only pass vacuously; `just check node-std-tests` runs it with the
island's declared shape in the environment, and its FIRST assertion is that
`REQUIRED > FLOOR` so that run cannot go quiet if the declaration stops
arriving. It restates the replaced arithmetic -- the one place in the tree that
is allowed, because what it asserts is that the arithmetic was wrong.

Measured on 2026-09-11 by deleting the two-line fix: the test reds with

    arena_size_for(3) = 108438 still does not cover the declared model of 144584

and passes with it. The always-on sibling
`a_per_entry_arena_covers_the_declared_model` holds the property for every
`cbs` in both branches.
