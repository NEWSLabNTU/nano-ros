---
id: 1517
title: "`[image.fvp]` declares a board its own application contradicts — and the
  comment justifying `entry =` is manufactured by that wrong value"
status: open
type: bug
area: examples, cli
severity: medium
found: 2026-09-27
related: [rfc-0098, issue-1288, issue-1253]
---

## What this is

`examples/workspaces/realtime-cpp/src/demo_bringup/system.toml`:

```toml
[image.fvp]
board = "native_sim/native/64"
# Two entry packages target this board (`zephyr_entry`, `fvp_entry`) and both
# declare `DEPLOY zephyr`, so the application cannot be derived — see
# `[image.zephyr]` below, which is the other one.
entry = "fvp_entry"
conf = ["prj-cyclonedds.conf"]

…

[image.zephyr]
board = "native_sim/native/64"
entry = "zephyr_entry"
conf = ["prj-zenoh.conf"]
```

The application it names hard-codes a different board:

```cmake
# examples/workspaces/realtime-cpp/src/fvp_entry/CMakeLists.txt
include($ENV{NROS_REPO_DIR}/zephyr/cmake/nano_ros_use_board.cmake)
nano_ros_use_board(fvp-aemv8r-smp)
…
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
```

`fvp_entry` is built by `west build` with **no `-b`** — the board comes from
`nano_ros_use_board` (phase-215.B), which is why the row's own value has never
had to be right.

## Why the comment is the interesting half

The comment explains why `entry =` is needed: two entries target this board, so
the application cannot be derived from it. **That ambiguity is manufactured by
the wrong value.** The two entries target *different* boards —
`native_sim/native/64` for `zephyr_entry`, `fvp-aemv8r-smp` for `fvp_entry` — so
with `board` stating the truth there would be nothing to disambiguate and the
derivation the comment says is impossible would work.

So this is not a stale comment beside a wrong field. It is a wrong field that
grew a justification, and the justification is what keeps anyone from
questioning it.

## Why nothing catches it

Nothing builds this row through `nros build`. `fvp_entry` is reached only by
`just zephyr build-fvp-ws-entry`, which calls `west build` on the package
directly, so the declared board is never compared against the built one. FVP is
additionally licence-gated, so the lane that would notice is not one anyone runs
by reflex.

## Why it belongs with issue 1288

1288 is "the hand-written workspace entries are all Zephyr, and `nros build` has
no generator for a west application" — its whole argument is that a board hidden
inside an application should be a declaration on the image row instead. Here the
declaration already exists, is already the right shape, and is already
measurably **false**. Generating the entry from the row without fixing this
would generate an application for the wrong board.

## Acceptance

- `[image.fvp] board = "fvp-aemv8r-smp"` (or whatever spelling the board
  catalog uses — check `nros ws model-dims` / the board catalog, do not copy the
  string from this issue).
- The `entry =` comment rewritten to say what is actually true, or `entry =`
  dropped if the row's board makes it derivable. **Decide by measurement**, not
  by the reasoning above: run the derivation and see.
- A check that an image row's board agrees with what its application builds, at
  least for the rows that declare an `entry =`. The general form is what 1288's
  generator makes unnecessary — once the entry is generated FROM the row, the
  two cannot disagree — so a narrow gate here is worth only as much as the gap
  before 1288 lands.
