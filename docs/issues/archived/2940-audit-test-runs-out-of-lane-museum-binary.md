---
id: 2940
title: "rmw_coordinate_truth audits every workspace row on disk, so on a persistent fixture tree it ran a row its lane never built: a museum binary from before phase-581 W6"
status: resolved
type: bug
area: [testing, fixtures, ci]
severity: high
found: 2026-11-24
resolved: 2026-11-24
related: [0196, 0442, 0584, 1620, phase-590]
---

## What happened

Release run-matrix 45351389552 (main `0d9fa3e470`, the nros12 cut), tier 2:
`rmw_coordinate_truth::a_rows_entry_registers_its_nodes_at_runtime` failed
on `workspace-features-mixed-params`, whose binary printed

    Raise `max_parameters` under [knobs.params] in system.toml, NROS_MAX_PARAMETERS or CONFIG_NROS_MAX_PARAMETERS.

That wording and spelling exist in no source at `0d9fa3e470`: phase-581 W6
renamed the knob (`a3f582f9e5`) and reworded the line (`9f2e1e8def`). In the
same run, `workspace-features-c-params` from the same workspace printed the
current text.

## Cause

It is not a missing rebuild edge. Measured on the tier-2 runner's persistent
tree (`newslab-118`, `nros-fixture-tree`):

- Tier 2's own fixture build (`tmp/build-test-fixtures-20261124-080025-520497/linux.log`)
  built only the `c` and `rust` features rows. `workspace-features-mixed-params`
  is at coordinate `(linux, cpp, zenoh)`, outside tier 2's 1-wise lane, so the
  run never built it.
- The test walks EVERY `workspace_fixture` row and runs whatever binary sits
  in the row's artifact directory. `built_row_binary` was `root.is_dir()` plus
  a find by name. It had no lane check and no `.inputsig` check, while every
  resolver applies both (`require_prebuilt_workspace_binary`). The symbol
  audit in the same file locates binaries the same way.
- On a persistent tree, an out-of-lane row's directory holds whatever an
  earlier, wider run left there. Here that was a build from before W6.
- The rebuild edge itself works. The tier-2 nightly's build (09:19, a wider
  lane) rebuilt the row, and the binary on disk afterwards (09:54) carries the
  current text and none of the old.

This is the issue 0196 / 0442 class (a museum binary read as current), reached
by a test that never asked the freshness probe at all.

## Fix

- `nros_tests::fixtures::workspace_row_build_is_current(id)` is the resolvers'
  own content check, exposed for audits: the row's `.inputsig` stamp against
  the signature its record derives now.
- `built_row_binary` (both audits in `rmw_coordinate_truth`):
  - skips a row outside the run's lane, because that row was not built by
    this run;
  - audits an in-lane row only if its build is current;
  - fails on a STALE row, naming it;
  - counts a row with no stamp as not located. In a gated run that is still a
    broken promise (issue 0584).

Sweep for other tests that locate a fixture binary by directory instead of
through a resolver: `git grep -n 'artifact_root\|manifest_rows()' --
packages/testing/nros-tests/tests` plus the `read_dir` walkers that spawn. The
remaining hits either read the manifest only (`entry_e2e`,
`matrix_fixture_coverage`, `lane_run_narrowing`, `fixture_group_resolution`) or
resolve their row first and then walk only its live build graph
(`interface_subset_linkage`, `out_of_tree_rmw_*`). `rmw_coordinate_truth` was
the only audit by path.
