---
id: 405
title: "tier2 lane gate demands nuttx-riscv workspace fixtures the lane build never produces"
status: open
type: bug
area: build
related: [issue-0393, issue-0196, phase-318, phase-331]
---

## Symptom

`just build-test-fixtures lane=tier2` completes green, then `just ci-matrix`
fails its `_lane-gate`:

```
ERROR: 1 workspace fixture(s) are missing or stale:
  workspace-c-nuttx-riscv-realtime (missing examples/workspaces/realtime-c/
    build-workspace-fixtures-nuttx-riscv/.nros-workspace-fixture.….inputsig)
```

Observed 2026-08-04 during the phase-330 validation rounds. Nothing the tier2
*builder* runs can ever create that file.

## Cause — the two 0393 layers stop agreeing below the module level

Issue 0393 made lanes narrow in two layers that "cannot select different
sets": `lane-coords --modules` picks which `just <mod> build-fixtures` run,
and `NROS_FIXTURE_COORDS` narrows the rows inside each. tier2's 1-wise cover
includes the coordinate `nuttx-riscv,c,zenoh`, and `lane-coords` maps it to
the `nuttx` module ("`nuttx` owns both arm and riscv").

But `just nuttx build-fixtures` builds only the **arm** examples/workspaces.
The riscv side lives in separate `full-matrix`-group recipes
(`build-riscv-c`, `build-riscv-c-workspaces`, `build-riscv-rust`) because the
shared NuttX kernel tree holds one board config at a time, so arm and riscv
need a kernel reconfigure between them. The coords layer selects the row; the
module layer has no recipe that builds it. The gate (correctly, per the
issue-0196 rule) checks the run's full lane coverage — so tier2 can never go
green from its own builder.

This was masked until phase-331 W6 renamed `ws-realtime-c` → `realtime-c`:
the rename orphaned any previously built
`build-workspace-fixtures-nuttx-riscv` tree, so the stale-but-present
artifact that used to satisfy the gate disappeared.

## Workaround

Run the full-matrix recipe by hand between the lane build and the gate:

```
just build-test-fixtures lane=tier2
just nuttx build-riscv-c-workspaces
just ci-matrix
```

Note the riscv kernel reconfigure rewrites `third-party/nuttx/nuttx/staging`
with the riscv lib set; the arm board build scripts re-scan it on their next
build (rerun-if-changed on the staging dir, added 2026-08-04) rather than
linking the stale list.

## Fix directions

1. **Teach the nuttx module stage to honor its coords** (preferred): when the
   run's `NROS_FIXTURE_COORDS` contains `nuttx-riscv,*` rows, the nuttx stage
   in `build-test-fixtures-leaves` (or `just nuttx build-fixtures` itself)
   appends the riscv recipes after the arm ones — serial, same stage, since
   they share the kernel tree.
2. Alternatively, make `lane-coords --modules` emit a distinct `nuttx-riscv`
   module wired to the riscv recipes, so the module list is honest about what
   each module can build.

Either way, add the check the issue-0196 rule asks for: the set of coords the
gate demands must be producible by the recipes the lane actually runs.
