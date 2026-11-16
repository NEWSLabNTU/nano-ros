---
id: 2643
title: "The stack-floor drift job's failed rows named only a log path on the runner, and the C template row moved with issue 2623's anchor fix"
status: resolved
type: bug
severity: medium
area: [testing, ci, stack-floor]
found: 2026-11-16
resolved: 2026-11-16
related: [2402, 2550, 2623]
---

## Symptom

The nightly of 2026-11-16 (run 43808375964, main `ffff1e331f`) went red in
`stack-floor drift (observed rows)`. Two of the four rows failed before any
walk:

- `qemu_bsp_talker_bringup@mps2-an385-baremetal`: `nros build
  qemu_bsp_talker_bringup:mps2-an385-baremetal` exited 101;
- `talker/c/zenoh@nucleo-f767zi-zephyr`: `nros build` exited 1.

The job printed each build log's path on the runner and nothing from the log
itself. The job runs in a container whose files nobody can read once it ends,
so neither failure could be triaged from the job's output.

## What was measured

**At the nightly's own commit both rows pass locally.** A detached worktree at
`ffff1e331f`, with only the `play_launch` submodule initialised, as the job's
checkout has: the bare-metal row walks 102,804 B (row 103,252), and the C row
walks 99,412 B (row 99,464). Both are within 2 %, so both are notes. Whatever failed on the
runner belongs to that environment, and its log is gone. Exit 101 is cargo's
exit code for a failed build as well as a Rust panic's, so the bare-metal row
need not have been a CLI panic.

**On main, the C row fails for a different, reproducible reason.** Issue
2623's anchor fix (#4031) stopped the C API being kept whole in every image.
The walk of the template's C talker rose from 99,412 B to 107,652 B (+8,188 B,
+8.2 %) with #4031, and fell back to 99,412 B with it reverted, so the
change is #4031's alone. The template stamps its main stack from the row
(117,576 B, from the row's `walk_floor` of 106,884 B plus the 10 % margin). That is now
below the walk's floor of 118,424 B, so the template's own build refuses:

```
nros-stack-floor: ERROR: the OPEN stack is 117576 B, below the measured floor 118424 B. Raise it: CONFIG_MAIN_STACK_SIZE=118424
```

The image did not start using more stack: the row's measured runtime peak is
11,876 B. The static walk is a conservative bound over a cyclic call graph
with bound indirect calls (issue 2550). Both deepest paths run through
indirect sites that cannot follow each other at run time. With fewer
functions in the link, the walk cuts its cycles at different places and
finds a longer chain.

## Fix

- `scripts/stack-floor-drift.py` prints the last 60 lines of a failing
  build's log under the row's `FAIL` line, so the next red names its own
  cause. Self-test cases cover the tail and a missing log.
- The C row is re-measured as the table prescribes: `static = 107652`. Its
  `walk_floor` (106,884 B) is now below `static`, so it holds nothing and
  goes, which is what `--validate-table` requires. The template stamps 118,424 B, and
  `a_rows_recorded_floor_is_the_check_formula` checks it. With a rebuilt CLI
  the drift check reports `the walk is 107652 B, as recorded`, and the
  template builds.

## Why no merge check caught it

#4031 changed every C image's link set, and the merge queue re-walks only one
stack-floor row: the mps2 bare-metal Rust talker, in `check-template-copy-out`
(shard 2). That row has no C in it, and the image moved by -448 B, well
inside its 2 % band. The three nucleo rows, the C one among them, need the
Zephyr SDK and a west workspace. They are re-walked only by the nightly
`stack-floor-drift` job. The Zephyr template copy-outs are deliberately daily
too, by the user's ruling of 2026-11-13 (`check-template-copy-out-zephyr`).
So the first check that could see the C template's walk move ran the night
after the merge, and it was already red for an unrelated runner failure.

## Proposal: a C template row in the merge queue, on the board it already builds

`check-template-copy-out` already builds the `mps2-an385-freertos` C and C++
templates in every merge group, with the board's pinned compiler. The walk is
the only new cost: measured at 20 s for the nucleo C talker's walk on its own,
warm (llc cache present, against a built image). The cold walk was not
measured.

That needs a recorded row for that image, and there is none yet. The FreeRTOS
cmake road does not export `compile_commands.json` or pass
`--compile-commands` to the walker, so its C call sites keep the unbound
fallback and the walk does not fit any stack the tutorial sets. That is
phase-557's open item (a). The order would be:

1. Item (a): the FreeRTOS cmake road exports compile_commands and passes
   `--compile-commands`. Re-walk the tutorial C image, and record a row only
   if the walk fits the stack the tutorial sets.
2. With that row, add `--only <the mps2-freertos C key>` to the queue's
   existing drift step, beside the bare-metal row. That is one more walk on a
   shard that has measured headroom of 3-7 min against the others.

#4031's change is the same class on both boards: it removes functions from
every C image's link. So a FreeRTOS C row would very likely have moved in the
merge group, though it would not have measured the nucleo template itself. Covering the nucleo
row per merge group means reopening the 2026-11-13 ruling, which is the
user's to make, not this issue's.

## Open

The runner-only failures at `ffff1e331f` are unexplained. The next run that
reproduces them will print the cause.
