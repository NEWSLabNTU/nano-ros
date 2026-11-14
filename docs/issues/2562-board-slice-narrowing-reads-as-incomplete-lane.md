---
id: 2562
title: "Every nightly board job whose module has a second board is red by construction: skipping the sibling board makes the lane INCOMPLETE (exit 78), and `test` stops on its `build-fixtures` dependency"
status: open
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

## Directions (not decided)

- A. A platform skipped inside a module because the run selects one of the
  module's OTHER platforms is the run's scope, not a skip: print it, and do
  not ledger it. SKIPPED (78) is kept for a module the run selects nothing
  of. This is one helper beside `nros_lane_out_of_scope_note`, used at the
  seven sibling sites (`just/freertos.just` ×3, `just/nuttx.just` ×4).
- B. Keep the ledger, and teach the nightly's Build and Test steps, and
  `test`'s `build-fixtures` dependency, to accept 78. This takes three
  consumers, and it leaves the untrue "fixtures are NOT built" line in place.
