---
id: 2562
title: "Every nightly board job whose module has a second board is red by construction: skipping the sibling board makes the lane INCOMPLETE (exit 78), and `test` stops on its `build-fixtures` dependency"
status: resolved
type: bug
severity: high
area: [ci, build, testing, nightly]
found: 2026-11-15
related: [phase-466, phase-558, 1758]
---

## What

In the per-board nightly (phase-558), `FreertosMps2`, `FreertosPosix`,
`NuttxArm` and `NuttxRiscv` fail both nights so far (runs 43383470982 on
2026-11-13 and 43559931051 on 2026-11-14), whatever else happens in them.
The two single-board modules, `ThreadxLinux` and `ThreadxRiscv64`, pass.
`NuttxRiscv` on 2026-11-14 shows the shape with nothing else in the way:

```
nuttx skip: the arm kernel: no nuttx (arm) coordinate in this run — skipping
lane nuttx INCOMPLETE — 1 step(s) skipped, so its fixtures are NOT built:
  - the arm kernel: no nuttx (arm) coordinate in this run — skipping
error: recipe `build-fixtures` failed with exit code 78
```

The Build step fails with that 78. The Test step runs `just test <module>`,
which reaches `just <module> test`. That recipe depends on `build-fixtures`,
gets the same 78, and stops before nextest starts. So the board's own
fixtures are built, its tests never run, and the job is red.

| board job | INCOMPLETE lines (11-13 / 11-14) |
| --- | --- |
| FreertosMps2 | 2 / 2 |
| FreertosPosix | 2 / 2 |
| NuttxArm | 1 / 1 |
| NuttxRiscv | 1 / 1 |

## Why

phase-558 W2 made each module stage that belongs to one board skip when it
runs in the other board's job. It does this with `nros_lane_wants_platform`
and `nros_lane_out_of_scope_note`, as nuttx's arm and riscv halves already
did. That note writes to the lane's skip ledger, and `nros_lane_skip_flush`
turns any non-empty ledger into `INCOMPLETE … fixtures are NOT built` and
exit 78, the SKIPPED verdict from phase-411 W2.

The tier drivers read 78 as SKIPPED. The nightly's per-board steps do not.
And the verdict is untrue here anyway: the module built every fixture its
board's slice selected. The only thing skipped is a sibling board, which
another job builds.

## Fix (2026-11-15)

Direction A, by the manager's ruling. A new helper,
`nros_lane_sibling_out_of_scope <lane> <reason> <sibling>...` in
`scripts/build/lane-skip.sh`, asks whether the run selects one of the
module's other platforms:
- If it does, the skip is the run's scope. It is printed as
  `<lane> scope: … (this run builds <sibling>)` and is not ledgered.
- If it selects none of them, the skip falls through to
  `nros_lane_out_of_scope_note`, and the flush still exits 78 (SKIPPED).

The seven sibling sites use it: `just/freertos.just` ×3 and `just/nuttx.just`
×4.

Direction B (teach the three consumers to accept 78) was not taken. It would
have kept "fixtures are NOT built" on a lane that built every fixture its
slice selected.

Fast gate `lane-sibling-scope` (`tests/lane-sibling-scope-tests.sh`) has four
cases:
- a selected sibling flushes 0;
- both platforms selected flushes 0;
- neither selected exits 78, the negative control;
- the sibling form leaves the ledger empty.

Mutation-tested: make the helper ledger in the sibling case, and two cases
go red.

Sweep: `git grep -n 'nros_lane_out_of_scope_note' -- just` lists no
call site left in a two-platform module.
