---
id: 755
title: "`NanoRosBoardFacts.cmake` never forwards the entry's DEPLOY — a
  multi-deploy system.toml can turn board facts into a silent skip"
status: resolved
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

## Resolved (2026-08-25)

Threaded exactly as the Direction said, plus one thing that section did not
anticipate: the memo.

`nros_resolve_board_facts` gains `DEPLOY` (defaulting to the directory-scope
`NROS_DEPLOY` that `find_package(nano_ros)` parses from the package.xml
export tuple) and forwards `--deploy` beside `--board`;
`nano_ros_add_executable` propagates its own resolved deploy — explicit
`DEPLOY` beats the tuple — through cmake's dynamic scoping rather than a
parameter threaded across every intermediate frame.

The part that needed care: this file cached ONE answer per configure, on the
stated reasoning that exactly one board is active (the toolchain file fixes it
before `project()`). That holds for BOARD and does NOT hold for DEPLOY — one
configure can carry several entry leaves, each naming its own. The memo is now
keyed per (board, deploy) and the result is returned in the caller's scope, so
a second leaf cannot inherit the first's facts.

Regression test on the verb side proves the pair the wrapper depends on: the
same multi-deploy `system.toml` that is REFUSED by board alone RESOLVES when
the deploy is named, and yields that deploy's own SDK root rather than the
other's — i.e. the ambiguity is real, and naming the deploy is the way out of
it.
