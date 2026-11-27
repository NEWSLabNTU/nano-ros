---
id: 2990
title: "`just ci matrix build` configures no CMake image, so it is not tier 2's build-and-link depth"
status: resolved
resolved: 2026-11-27
resolved_in: "the change that archives this issue"
type: bug
severity: medium
area: [ci, testing]
related: []
found: 2026-11-27
---

## What happened

PR #4928 changes the six GNU-binutils CMake toolchain files (`ar D`). It was
gated on host 175 with the full `just ci gate` plus `just ci matrix build`,
because CLAUDE.md sells `just ci matrix build` as tier 2 "builds+links only"
for a change to `cmake/`. Both exited 0.

175 then read the matrix-build log. It names none of the six toolchain
files: zero hits each for armcm3, armcr52, armcr5f, riscv32imac-nuttx-elf
and riscv64-threadx, while armv7a-nuttx-eabihf appears only as a cargo
target-dir path. So the green said nothing about the change it was run for.

## Why

`just ci matrix build` dispatches to `_matrix-build`, whose only dependency
is `l3`. `l3` runs:

- `rust-rtos-link-check`, the cargo road for the embedded RTOS Rust talkers;
- an ELF check of three Rust images (FreeRTOS mps2, NuttX armv7a, ThreadX
  Linux);
- the L3 ELF and heap-free checks.

It builds no fixture row and configures no CMake image. The C and C++
images, and every image built through `cmake/toolchain/*.cmake`, are
outside it.

Three texts claim otherwise:

- CLAUDE.md (Practices, `just ci gate`): "`just ci matrix` runs, `just ci
  matrix build` builds+links only (27 s)", placed under the tier-2 entry
  that says to run tier 2 "when the diff touches `packages/core`, codegen,
  or `cmake/`".
- `just/ci.just` above `matrix`: "build depth is 46 s against 20+ minutes
  for run depth over the same tree". The two depths do not cover the same
  tree: run depth goes through tier 2's fixture build, which does
  configure CMake images.
- The same comment calls the depth argument a choice between "build +
  link" and "build + execute" of one cover.

## Consequence

A `cmake/` change gated with `just ci matrix build`, which is what the
docs prescribe, has its CMake roads unbuilt. #4928 would have armed on a
green that never configured a CMake image. 175 caught it by reading the
log, not from any verdict.

## Ruling (2026-11-27)

The user, relayed by the manager, chose option 3: **add a CMake smoke.**
`just ci matrix build` also configures and links one cheap CMake-road image
per cross-toolchain file, so a `cmake/` change is linked at tier-2 build
depth.

- The toolchain list is DERIVED from `cmake/toolchain/*.cmake`, never an
  authored table. A new toolchain file with no smoke fails the gate.
- The seed is the set #4928 needed, one row per GNU toolchain file:
  - `freertos-c-zenoh-talker` (armcm3);
  - `workspace-cpp-mps3-an536-freertos` (armcr52);
  - `workspace-c-nuttx` (armv7a-nuttx-eabihf);
  - `workspace-c-nuttx-riscv-realtime` (riscv32imac-nuttx-elf);
  - `workspace-c-threadx-riscv64` (riscv64-threadx);
  - armcr5f, which has no board row: `check r5f-hard-float-abi` and
    `check out-of-tree-board-images`.
- The added minutes are measured and stated, and the "27 s" / "46 s"
  figures are corrected to what the recipe then builds.

Implementation follows #4931.

## Second ruling (2026-11-27): where it runs

With the measured price (host 175, sccache off: 893 s cold, 337 s warm,
+12 GB peak), the user ruled that the smoke runs in the scheduled tier-2
nightly and in local `just ci matrix build`, NOT per merge. The merge queue
keeps its "no fixture build" contract. Full Cortex-R5F coverage stays,
since nightly time is not the constraint.

## Resolution

- `scripts/ci/cmake-toolchain-smoke.py` derives the plan: a toolchain file
  sets `CMAKE_SYSTEM_NAME`; it maps to boards whose `toolchain_file` names
  it, then to images on those boards, then to the first C/C++ fixture row.
  - An include-only file is credited through its include.
  - `NO_ROW` covers cr5f and TI, with reasons, and is refused when stale.
  - A recipe needing a capability no lane claims (TI's `tiarmclang`) is
    reported out of scope, never run and never a pass.
- `just ci cmake-toolchain-smoke` builds it. `just ci matrix build` runs
  `_matrix-build`, then the smoke. `queue.yml` calls `just ci::_matrix-build`
  directly, so the queue never runs the smoke. The tier-2 nightly job runs
  it as its own step.
- `just check cmake-toolchain-smoke-plan` (fast, buildless) refuses a
  toolchain file with no smoke. Its selftest checks:
  - a new file is refused;
  - an alias is credited;
  - an alias with its own flag is refused;
  - a stale NO_ROW entry is refused;
  - an unclaimed capability is reported out of scope.
- CLAUDE.md and the `matrix` recipe comment now say what build depth
  builds.

## Options considered

1. Make build depth really be tier 2's cover at link depth: the
   `build-test-fixtures lane=tier2` build that `_matrix-run` relies on,
   without executing cells. It is slower than 27 s, and the docs' numbers
   move with it.
2. Keep `l3` under an honest name (a cargo-road RTOS link check), and
   correct CLAUDE.md and the recipe comment so neither claims tier-2 build
   depth. A `cmake/` change would then name its own CMake rows.
3. Add a small CMake-road smoke to `_matrix-build`: one C or C++ row per
   cross toolchain file, built and linked. This is the set #4928 needed,
   chosen by toolchain file so a new toolchain file cannot be left out (a
   gate could check that every `cmake/toolchain/*.cmake` has a row).

Whichever is chosen, the "27 s" / "46 s" figures and the "same tree" claim
must be re-measured against what the recipe actually builds.
