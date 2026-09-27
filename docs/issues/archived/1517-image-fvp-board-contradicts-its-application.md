---
id: 1517
title: "`[image.fvp]` declares a board its own application contradicts — and the
  comment justifying `entry =` is manufactured by that wrong value"
status: resolved
type: bug
area: [examples, cli]
severity: medium
found: 2026-09-27
resolved: 2026-09-27
related: [rfc-0098, 1288, 1253, 1519, 1520]
---

## What was wrong

`examples/workspaces/realtime-cpp/src/demo_bringup/system.toml` declared

```toml
[image.fvp]
board = "native_sim/native/64"
# Two entry packages target this board (`zephyr_entry`, `fvp_entry`) and both
# declare `DEPLOY zephyr`, so the application cannot be derived — see
# `[image.zephyr]` below, which is the other one.
entry = "fvp_entry"
conf = ["prj-cyclonedds.conf"]
```

while the application it names hard-codes a different board:

```cmake
# examples/workspaces/realtime-cpp/src/fvp_entry/CMakeLists.txt
nano_ros_use_board(fvp-aemv8r-smp)
```

The value dated from the phase-383 W9 migration and had never had to be right,
because `just zephyr build-fvp-ws-entry` passes **no `-b`** (phase-215.B) — the
board comes from `nano_ros_use_board`.

`nros build` is the second driver, and it DOES pass `-b`, so the wrong value was
reaching a real command line. Measured, `nros build fvp --dry-run`:

```
west build -b native_sim/native/64 …/src/fvp_entry -- -DEXTRA_CONF_FILE=…
```

`nano_ros_use_board` does not refuse that — it warns
(`BOARD=… overrides the board crate's ZEPHYR_ID=…`) and proceeds, so the FVP
application would have compiled for native_sim.

## The comment was the interesting half, and the issue's own prediction was wrong

The comment justified `entry =` by claiming two entry packages target this board.
That ambiguity was MANUFACTURED by the wrong value, and the issue drew the
obvious conclusion from it: with the board stating the truth "the derivation the
comment says is impossible would work". **Measured, it does not.** All four
states of the row, through `nros build fvp --dry-run`:

| `board` | `entry` | what the derivation did |
| --- | --- | --- |
| `native_sim/native/64` | `fvp_entry` | built `fvp_entry` for native_sim (the bug) |
| `native_sim/native/64` | absent | refused: **2 candidates**, `fvp_entry` + `zephyr_entry` — the comment's claim, true only because of the wrong value |
| `fvp-aemv8r-smp` | `fvp_entry` | built `fvp_entry`, correct board |
| `fvp-aemv8r-smp` | absent | **0 candidates** → fell back to the bringup directory → `conf fragment prj-cyclonedds.conf not found`, looked for in `src/demo_bringup` |

`west_application_dir` matches a package by the board its `DEPLOY` token
resolves to, and a Zephyr entry's `DEPLOY` names the PLATFORM (`zephyr`) because
that is what `NanoRosEntry.cmake`'s link gate compares against
(`NANO_ROS_PLATFORM`, `cmake/NanoRosEntry.cmake`). The board the application
really targets is chosen by `nano_ros_use_board(...)` inside its own
`CMakeLists.txt`, which the token scan does not read. So correcting the board
removes the ambiguity and does not make the entry derivable — it moves the
failure from "too many" to "none".

Changing `fvp_entry`'s `DEPLOY` to the board id was considered and rejected:
`nano_ros_use_board` sets neither `NANO_ROS_BOARD` nor `NANO_ROS_PLATFORM` to
`fvp-aemv8r-smp`, so `NanoRosEntry.cmake`'s
`if(NANO_ROS_BOARD IN_LIST DEPLOY OR NANO_ROS_PLATFORM IN_LIST DEPLOY …)` would
stop matching and the entry would link no nodes — a change only an FVP build can
verify, and the FVP lane is not runnable here.

## What landed

* `board = "fvp-aemv8r-smp"` — the nano-ros board id, which is what the field
  takes. `ImageBlock::board`'s own doc-comment states the rule: "nano-ros board
  id … **NEVER a framework's own board string**", the Zephyr
  `<board>/<soc>/<variant>` spelling being a resolution RESULT. Source of the
  spelling: `names` in
  `packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/nros-board.toml`,
  confirmed with `nros board info fvp-aemv8r-smp`.
* `entry = "fvp_entry"` KEPT, with the comment rewritten to the measured reason
  above and the manufactured one removed.
* The same manufactured claim removed from its five other homes —
  `ImageBlock::entry` and `west_application_dir`'s doc-comments,
  `book/src/getting-started/{integration-zephyr,workspace-entry-pkg}.md`,
  `book/src/user-guide/component-and-entry-pkg.md`, all of which said "both
  `DEPLOY zephyr`, on the same board", plus a dated correction note on
  `docs/design/0085-zephyr-workspace-and-west-handoff.md`, whose count rested on
  it. Each now describes BOTH arms. What the substitute example should be is
  issue 1520, because the obvious one is not one either (measured there).
* `check-deploy-board-resolves` — which FAILED on the correct value, naming it
  as a board "no descriptor claims". It globbed
  `packages/boards/*/nros-board.toml`, the immediate subdirectories, while the
  authority it speaks for (`BoardCatalog::collect_board_dirs`) descends to find
  one; the FVP descriptor is nested one level deeper, so the gate could not see
  it. 0196's shape, and an active obstacle to this fix: writing the right value
  turned the fast line red. Now an index lookup over `packages/boards` at any
  depth — 14 descriptors instead of 13, 42 aliases.
* A note on `fvp_entry/CMakeLists.txt` that `nros build` is a second driver and
  passes `-b`, so the row and `nano_ros_use_board` have to stay in step until
  1288 generates one from the other.
* `BoardDescriptor::west_build_board`, because the corrected row alone made
  `nros build fvp` fail a NEW way. `nros build` read the OUTER
  `BoardDescriptor::west_board`, which **no in-tree descriptor declares**;
  `[board.zephyr] west_board` is where three of them state it, and
  `BoardZephyr::west_board` calls it "the one irreducible fact". So `-b` fell
  through to the authored id for every board that states it there, and
  `board = "fvp-aemv8r-smp"` reached `-b fvp-aemv8r-smp`, a board west has never
  heard of. One rule now, on the descriptor.

## Verified

Everything below is a measurement on this host; nothing rests on reading alone
except where said so.

* All four rows of the table above, `nros build fvp --dry-run` (safe by
  construction — a `Handoff` performs no I/O until `exec`), after
  `nros sync` in `examples/workspaces/realtime-cpp`.
* End state: `west build -b fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp …/src/fvp_entry`.
  That is the id `nano_ros_use_board(fvp-aemv8r-smp)` projects as
  `NROS_BOARD_ZEPHYR_ID` (`nros board cmake-vars`), so the row and the
  application now agree and the override warning has nothing to fire on.
* `[image.zephyr]` unchanged: still `-b native_sim/native/64 …/src/zephyr_entry`.
* `a_zephyr_boards_west_b_comes_from_its_descriptor` (in
  `packages/cli/nros-cli-core/tests/board_key_table.rs`, so it runs under
  `check-cli-tests`, on the required PR context) asserts the projection over
  every zephyr descriptor in the real catalog, both arms, with floors so neither
  can go vacuous. Mutation control: deleting the `[board.zephyr]` fall-back
  makes it fail.
* `board_key_table` (10 tests) and `build_verb_pipeline` (32 tests) green.
* `just check fast` green, exit 0, 0 of 346 gates failed (14 skipped, all
  environmental) — including `check-deploy-board-resolves`, which is red on this
  row's correct value without the reach fix.
* `just ci gate`: `cli-fresh`, `launch-resolve-fresh` and `check::fast` OK;
  `check::build` red at `scaffold-builds` and `template-copy-out`, both
  re-run alone afterwards and both **green (rc=0, 6/6 variants each)**. The reds
  were one cause and it was the environment, not the change: the parent
  checkout's `nros` was first on `PATH` (no `activate.sh` in this worktree), so
  every one of the 8 sub-failures read `in-tree nros CLI is STALE`. With
  `packages/cli/target/release` prepended they all pass. `check::api-parity`,
  `test-unit` and `test-lane-contracts` were WITHDRAWN by the red and have not
  been run.

**What rests on READING rather than on a run:** what cmake does with a
mismatched `BOARD`. `zephyr/cmake/nano_ros_use_board.cmake` sets `BOARD` only
`if(NOT BOARD)` and otherwise `message(WARNING … Proceeding with the user value;
per-board overlays may not apply.)` — a warning, not a `FATAL_ERROR`. That is
the source, not an observed configure. The `-b` string itself is measured.

**Not verified: the FVP image does not BUILD here.** It needs a Zephyr 3.7 west
workspace, the `aarch64-zephyr-elf` toolchain and the Arm FVP; `just zephyr
build-fvp-ws-entry` skips without them. So "the row's board now agrees with what
its application builds" is established from the `-b` string and the board
crate's projected `NROS_BOARD_ZEPHYR_ID`, not from an image. (For the record the
FVP is NOT licence-gated any more —
`packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/nros-board.toml` records
`tools = ["arm-fvp"]` and a 2026-09-06 measurement of Arm's permalink; what keeps
CI out is cost and x86_64-only hosting.)

## No gate, and why

The acceptance criterion asked for a check that an image row's board agrees with
what its application builds, for rows declaring `entry =`. **Weighed and
declined.** Reach, measured: 10 `entry =` rows across `examples/`, and exactly
**one** application in `examples/` declares its own board
(`nano_ros_use_board`, in `fvp_entry`). The other nine take their board from
`west -b`, i.e. from the row, so there is no second author to disagree with. A
rule with a population of one is a check of this row wearing a rule's clothes,
and issue 1288 removes even that one by generating the entry FROM the row.

What was worth pinning is the half that is load-bearing and has a real
population — the row's board reaching `west -b` — and that is the test above:
4 descriptors, 6 names, mutation-controlled, in a lane that runs.

## Not fixed here — issue 1519

The other half of the `-b` projection. `board = "zephyr"` (23 rows in
`examples/`) reaches `west build -b zephyr`, because the `zephyr` descriptor
states no `[board.zephyr]` at all and carries `native_sim/native/64` as a second
NAME instead — the smuggling `BoardZephyr::west_board` was added to retire. The
10 rows spelling it that way are therefore authoring a framework board string
against `ImageBlock::board`'s own rule, and they work BECAUSE they do. Fixing it
changes `-b` for 23 rows and wants a Zephyr build to confirm, so it is filed
rather than guessed at.
