---
id: 1690
title: "`a_rows_entry_registers_its_nodes_at_runtime` runs every workspace entry
  serially inside one test and outlives nextest's 60 s terminate, solo as well
  as under load"
status: open
type: bug
area: [testing]
severity: medium
found: 2026-10-05
related: [1651, 1684]
---

## What fails

`nros-tests::rmw_coordinate_truth a_rows_entry_registers_its_nodes_at_runtime`
is killed at `TIMEOUT [60.014s]` with no assertion message. It loops over
every `workspace_fixture` row in `examples/fixtures.toml` and runs each entry
one after another (`run_entry`), so its wall time is the SUM over rows.

- CI: workflow_dispatch run 37252649866 — TIMEOUT in the full `test-all`; its
  sibling `a_rows_rmw_is_the_backend_its_artifact_linked` passed at 54.8 s,
  six seconds under the same ceiling.
- Local, SOLO (`-j1`), fixtures built from the same tree by
  `just build-test-fixtures lane=tier1`: TIMEOUT at 60.003 s again. So this is
  not load; the test's own budget is wrong for the row count.

No per-test override in `.config/nextest.toml` names this binary.

## Options

Give the test a `slow-timeout` override sized from a measured run, bound each
row's `run_entry` and run rows in parallel, or split it into a matrix cell per
row (RFC-0051) so one hung entry names itself instead of timing out the lot.
The sibling at 54.8 s will cross 60 s the next time a row is added.

## Acceptance

Both `rmw_coordinate_truth` runtime tests finish under their timeout with a
measured margin, and a hung row is reported by name.
