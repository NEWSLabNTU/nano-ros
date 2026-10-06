---
id: 1690
title: "`a_rows_entry_registers_its_nodes_at_runtime` runs every workspace entry
  serially inside one test and outlives nextest's 60 s terminate, solo as well
  as under load"
status: resolved
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

## Resolution

Fixed 2026-10-06 (branch `fix/1690-entry-registration-timeout`).

**Root cause, measured.** Both tests visited every `workspace_fixture` row one
after another, so their wall time was the SUM over rows. Fixtures built from
this tree (`just native build-workspace-fixtures`; 73 rows located), solo
(`cargo nextest run -p nros-tests --test rmw_coordinate_truth -j1`):

```
TIMEOUT [  60.002s] a_rows_entry_registers_its_nodes_at_runtime
   PASS [  22.459s] a_rows_rmw_is_the_backend_its_artifact_linked
```

The per-row cost is small (no router on this host: 2.8-3.8 s per row, the
slowest the xrce rows failing their Agent lookup); 73 of them do not fit 60 s.
A single hung row also could not have been named: each row's budget was 45 s,
most of the test's ceiling.

**Fix.** One `par_map` in the test file, a bounded pool (host parallelism,
capped at 8) returning results in input order, used by BOTH tests -- the entry
runs and the `nm` passes. Each run is timed; the test prints its wall time and
the five slowest rows, and a row that outlives its budget is reported as
`HUNG: no exit within the 20s budget (killed after …): <binary>` with its row id
instead of a suite-level TIMEOUT. `RUN_BUDGET` 45 s -> 20 s, five times the
slowest measured row, so one hang plus the rest of the pool stays well inside
the terminate. A per-row nextest cell (RFC-0051) was not chosen: the rows are
derived from the manifest at run time and the test's job is the sweep.

**After**, same fixtures, solo:

```
rmw-coordinate-truth: ran 73 entries in 25.6s on a pool of 8; slowest: workspace-c-native-xrce 3.1s, …
   PASS [  28.049s] a_rows_entry_registers_its_nodes_at_runtime
   PASS [   2.684s] a_rows_rmw_is_the_backend_its_artifact_linked
   PASS [   0.303s] the_row_pool_runs_rows_concurrently_and_keeps_their_order
```

Margins: 32 s and 57 s under the 60 s terminate. The new
`the_row_pool_runs_rows_concurrently_and_keeps_their_order` is the negative
control: it refuses a serial map (8 x 300 ms must finish under 2.1 s) and
checks input order.

**Sweep.** `grep -ln "manifest_rows()" packages/testing/nros-tests/tests/*.rs |
xargs grep -ln "Command::new\|spawn"` -- the other three hits run `python3`
once, not per row.

**Not measured.** A run WITH a router up (CI's shape): this host ran 1 entry to
completion and 72 reported "no peer", so the per-row cost with a live session
was not measured here. A real hang (the `HUNG` line) was not provoked.
