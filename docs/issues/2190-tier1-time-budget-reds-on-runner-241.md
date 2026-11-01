---
id: 2190
title: "Tier 1 on newslab-server-241: all five real failures are time budgets"
status: open
type: flake
severity: low
area: [ci, testing]
related: [0854, 1690, 2180]
found: 2026-11-01
---

## Symptom

The manually dispatched run-matrix 41205983383 (2026-10-31, on `37d6466751`)
included tier-1 job 124572774243, `tier 1 (cells)`. It ran on
`nano-ros-runner-newslab-server-241` from 15:08 to 19:23 UTC. This was the
first tier-1 run since issue 2180 to get past `just build tier1` and run its
cells. The junit rewrite reported `Real failures: 5 / 5 total failures`, and
every one of the five is a timing budget:

| Test | How it failed |
| --- | --- |
| `nros-tests fixtures::cache_key::tests::a_resolver_row_observation_reads_the_real_dep_file` | `TIMEOUT [60.005s]` |
| `nros-tests::action_raw_goal_e2e action_raw_goal_ships_one_cdr_header` | `TIMEOUT [60.004s]` |
| `nros-tests::contract_monitor_parity contract_monitor_cpp_twin_reports_rate_violation` | `TIMEOUT [61.067s]` |
| `nros-tests::contract_monitor_parity contract_monitor_cpp_uncontracted_twin_carries_zero_rows` | `TIMEOUT [61.051s]` |
| `nros-tests::rmw_coordinate_truth a_rows_rmw_is_the_backend_its_artifact_linked` | `26 row(s) got no nm verdict within budget (located 110 binaries in 4.8s, nm over them in 27.3s …)`; per row: `not read — the sweep had spent its 25s` |

The four timeouts are consecutive in the run (2782–2789 of 3394), so they
overlapped one stretch of time.

## What it is NOT

- It is not out of disk. The log has no `No space left`, even though the
  tier-2 job of the same run did run out of disk on newslab-118 (issue 1353
  shape).
- It is not issue 2180: that gate passed here.
- There is no assertion about behaviour among the five; each test ran out of
  time.

## Prior art

- 0854 (`action_raw_goal` flaky under load) and 1690 (`rmw_coordinate_truth`
  entry registration times out) were both archived. This is the first
  sighting of either since.
- `contract_monitor_parity` and the `cache_key` resolver test have no prior
  issue for a timeout.

## What would close it

Either:

- the next tier-1 run on server-241 passes all five tests without a code
  change, which would mark it as runner load; or
- `rmw_coordinate_truth`'s 25 s `nm` budget is scaled to the runner, as 1690
  did for registration, and the four 60 s timeouts get a test-group or a
  timeout override that matches what they do.

Re-run the five tests SOLO on server-241 before changing any budget.
