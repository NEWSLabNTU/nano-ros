---
id: 755
title: "`NanoRosBoardFacts.cmake` never forwards the entry's DEPLOY — a
  multi-deploy system.toml can turn board facts into a silent skip"
status: open
type: enhancement
area: cmake
related: [issue-0729, phase-351]
---

## Problem

`nros ws board-facts` accepts `--deploy` and refuses only when several
deploy blocks naming the same board resolve DIFFERENTLY — correct verb
behaviour. But the cmake wrapper (`cmake/NanoRosBoardFacts.cmake`) only
ever passes `--board`:

```cmake
set(_args ws board-facts "${_ws}")
if(NOT _board STREQUAL "")
    list(APPEND _args --board "${_board}")
endif()
```

`nano_ros_add_executable(... DEPLOY <name>)` knows exactly which deploy
this build is, and does not forward it. When a bringup's `system.toml`
grows several deploys that genuinely resolve differently (the shape a
multi-target consumer converges on — one bringup, deploys for
posix + fvp + hardware), the verb's ambiguity refusal comes back non-zero,
and the wrapper's deliberately-soft error handling ("everything but a
netstack-domain error means this build has no facts to carry") turns it
into a silent skip: tiers/sizing facts quietly missing from the image.

## Evidence (consumer side)

autoware-safety-island's `controller_bringup/system.toml` now carries
multiple `[deploy.*]` rows (posix, fvp, s32z2 — ASI phase-4 W5.b). Today
each lane happens to disambiguate via `--board` or resolve identically;
the first time two rows on one board diverge (e.g. same board, different
netstack or tier table), facts vanish with a STATUS line at best.

## Direction

Thread the deploy through: `nano_ros_board_facts(... DEPLOY <name>)`,
populated by `nano_ros_add_executable` from its own DEPLOY argument, and
`--deploy` appended like `--board`. The entry-leaf metadata rung
(`[package.metadata.nros.entry] deploy`) already exists for the Zephyr
arm; this is the same fact for the workspace arm, from the caller that
authoritatively knows it.
