---
id: 2212
title: "nros-node hazard ladder tests shared one static output row, so a parallel test's publish answered another's top rung"
status: resolved
type: bug
area: [testing]
severity: medium
found: 2026-11-02
resolved: 2026-11-02
related: [2173, 2210]
---

## Measured

`executor::monitor::hazard_tests::a_fault_at_the_top_is_judged_against_the_top_rung`
failed in a local `just ci gate` (step 7, `check::build` → `node-std-tests`)
on a branch that touched no Rust. It passed when `just check node-std-tests`
was re-run alone.

The ladder tests run 2000 times each with `--test-threads=16`
(`hazard_tests` filter, lib test binary,
`alloc,log,sim-time,param-services,lifecycle-services`):

| tree | failed |
| --- | --- |
| `main` at ddedecc828 | 22 / 2000 |
| with this fix | 0 / 2000 |

## Cause

The `ladder` fixture added by issue 2173 (#2173's route-per-rung work)
declared its three output rows as ONE module-level `static OUT`, shared by all
five ladder tests. Each test had its own `HazardCell`, but `cargo test` runs
tests on parallel threads, so a `publish(0)` in
`a_fault_after_a_fall_is_judged_against_the_lower_rung` could land on the
top rung's output row while `a_fault_at_the_top_…` waited for that row to stay
silent. The top rung then counted as answered, and no
`hazard-route-runtime` verdict was reported.

The same sweep found no sibling. Every other `static` cell in
`executor/monitor.rs`'s test modules is written by one test only, and
`PUB_CELL` and `SINK_CELL` in the shared `RATE_ROWS` are never read as an
output or sink by any test but the one that writes them.

## Fix

`ladder::Ladder::new()` builds one test's output cells, its rows and its
`HazardCell`, leaked so they are `'static` like the generated tables they
stand in for. `tick` and `publish` are methods on it. The rung and guard
tables stay shared, because they are immutable.
