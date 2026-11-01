---
id: 1984
title: "`a_rows_rmw_is_the_backend_its_artifact_linked` hit nextest's 60 s terminate on a slow runner and named no row"
status: resolved
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-19
related: [issue-0831, issue-1690, phase-505]
resolved_in: 2026-10-20
---

**Home: [phase-505](../../roadmap/archived/phase-505-ci-lanes-reach-and-infra-robustness.md)** — listed in that phase's "Issues homed here".

## What failed

The scheduled run-matrix run 39210994277 (2026-10-19, head `c75d1265eb`) went
red in its tier 2 job on exactly one test:

```
TIMEOUT [  60.004s] nros-tests::rmw_coordinate_truth a_rows_rmw_is_the_backend_its_artifact_linked
```

The same test passed in 5.6 s and 6.9 s in the two previous tier 2 runs
(38933437082, 38980380118), and 1690 measured 2.7 s solo on a dev host.
Its sibling `a_rows_entry_registers_its_nodes_at_runtime` passed in the same
run (27.0 s).

The failing job ran on a runner this lane had not used before,
`nano-ros-runner-newslab-server-241`; the two passes ran on
`nano-ros-runner-newslab-118-server`. That host was slow across the board in
this run: `check fast`'s slowest gate (`sizes-header-mirrors`) took 1 354 s
there against 89–104 s on runner 118, and the lane's `check::default` step
took 46 min.

A TIMEOUT prints nothing from the test, so which part ran long cannot be
recovered from the run.

## Cause

The test had two unbounded costs, and neither reported itself:

1. **Locating** asked `current_workspace_fixture_record` once per manifest row
   (126 rows), and each call spawned a `python3` that re-read
   `examples/fixtures.toml`. That is about 5 s serially on an idle host
   (0.04 s × 126), before any `nm` runs, and it grows with load on a host
   whose I/O is the bottleneck.
2. **`nm`** ran with no bound. The pool (issue 1690) bounds the sweep's wall
   time by the slowest row, but one `nm` on a starved disk could still spend
   the whole budget, and the only verdict was nextest's anonymous TIMEOUT.

Not measured: which of the two dominated on runner 241. That host is not
reachable from here, and the timeout left no output.

## Resolution

- `current_workspace_fixture_record` reads the manifest once per process
  (`workspace_fixture_records`, a `OnceLock`). `list-workspaces --id X` renders
  the same record as the unfiltered listing for all 126 rows (compared one by
  one), so no answer changes. Locating 24 built rows now takes 91 ms. Every
  caller benefits, the sibling entry-run test included.
- Each `nm` pass is bounded by `NM_BUDGET` (20 s) and killed past it. No row
  starts after `NM_SWEEP_BUDGET` (25 s). Either case FAILS the test, naming the
  row, the binary and the time spent, with the worst case (25 s + 20 s plus
  locating) inside the 60 s terminate.
- The test prints its locate time, its sweep time and its five slowest rows
  on every run, pass or fail. The next slow runner says where the time went.

Measured (fixtures from `workspace-fixtures-build.sh linux mixed`, 24 rows):

```
rmw-coordinate-truth: located 24 binaries in 91.5ms, `nm` over them in 152.6ms; slowest: …
PASS [   0.248s] a_rows_rmw_is_the_backend_its_artifact_linked
```

| Mutation (restored, cmp clean) | Result |
| --- | --- |
| `NM_BUDGET` = 0 | FAIL: 24 rows, each "`nm` outlived its 0ns budget (killed after …): <binary>" |
| `NM_SWEEP_BUDGET` = 0 | FAIL: 24 rows, each "not read — the sweep had spent its 0ns" |

**Sweep:** `git grep -n 'current_workspace_fixture_record\|Command::new("nm")' -- packages/testing`.
There are 5 callers of the record lookup (4 in `fixtures::binaries`, 1 here), and all of them go through the cache now. The only `nm` call is this one.

**Not done:** the nextest timeout was not raised. If runner 241 is slow enough to
spend 25 s on the sweep, the test now fails and names the rows. It does not
pass on a longer budget.
