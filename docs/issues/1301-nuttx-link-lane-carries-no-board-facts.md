---
id: 1301
title: "The NuttX link lane builds its own cargo command and delivers no BOARD
  facts — and `check-board-facts-delivery` cannot see it, because the gate reads
  two directories and the lane is in a third"
status: open
type: tech-debt
area: [build, boards]
related: [1142, 0529, 0196]
---

## What

`packages/api/nros-c/cmake/nros-nuttx.cmake` links a NuttX image with its own
`cargo build` wrapped in a `cmake -E env` command — the lane
`check-board-facts-delivery` calls "a lane that builds its own cargo command",
whose rule is:

> a lane that builds its own cargo command calls `nros_resolve_board_facts()`
> and puts the result on that command.

It does not. `nros_resolve_board_facts` and `NROS_BOARD_FACTS_ENV` appear
nowhere in that file (checked 2026-09-11), so every NuttX image's cargo
invocation carries no board rung and no site config, and every RFC-0049 board
knob DEFAULTS — silently, which is issue 0529's shape, the one the gate exists
to make impossible.

## Why the gate is green anyway

`check-board-facts-delivery` walks exactly two roots:

```python
CMAKE_DIRS = (os.path.join(ROOT, "cmake"), os.path.join(ROOT, "zephyr", "cmake"))
```

The NuttX lane lives in `packages/api/nros-c/cmake/`, so the file is never
opened. The gate's REACH is narrower than the rule it enforces — the 2026-07-28
audit's shape (issue 0196), and the same shape the gate's own docstring
describes catching one lane earlier ("checking only the first is how the ZEPHYR
arm shipped inert").

Note also that the detection is by SOURCE TEXT and counts COMMENTS: a file that
merely *describes* such a command reads as one (this was measured while landing
issue 1142 — a doc comment naming the NuttX command made
`cmake/NanoRosEntityFacts.cmake` an offender).

## How it was found

Issue 1142 measured the ENTITY half of the same gap: `nros_entity_facts_env`
delivers through `corrosion_set_env_vars`, which reaches nothing on a lane that
uses no Corrosion, so a NuttX image sized `ZPICO_MAX_QUERYABLES` from the
backend's literal whatever it declared. Fixing that (an `ENV_OUT` mode on the
same composition, spliced into this lane's own env list) put the board facts'
identical absence one line away.

## What to do

1. Deliver: `nros_resolve_board_facts()` beside the entity facts in
   `nros_nuttx_build_example`, its `NROS_BOARD_FACTS_ENV` spliced into the same
   `cmake -E env` list issue 1142's `_nnbe_entity_env` now goes into.
2. Widen the gate to every tracked `**/cmake/*.cmake`, not two roots — and
   check what ELSE that surfaces before assuming NuttX is the only one.
3. Verify by READING THE VALUE out of `build.ninja`, never the exit code
   (phase-412 acceptance 2): a board knob that arrives and a board knob that
   defaults to the same number are indistinguishable by rc.

## Related

* **1142** — the same delivery gap for the ENTITY facts, on the same lane, fixed.
* **0529** — "no value, defaulted, with no diagnostic": what this costs.
* **0196** — a gate whose coverage is narrower than the rule it enforces.
