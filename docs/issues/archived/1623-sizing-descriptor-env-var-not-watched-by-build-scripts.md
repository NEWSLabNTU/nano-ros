---
id: 1623
title: "No build script watched NROS_SIZING_DESCRIPTOR itself, so a script that
  first ran with it unset kept its undeclared defaults after a later build set
  it — a declared C++ talker built at the worst-case arena with a descriptor
  beside it stating one timer"
status: resolved
type: bug
area: [build, executor]
severity: high
found: 2026-10-01
related: [issue-0810, issue-1577, issue-0491, rfc-0100]
resolved_in: "branch fix/executor-arena-exact-0810-1340-1370-1036-1496"
---

## What happened

`nros_sizing_descriptor::from_build_env` — the one loader every consumer
build script calls (`nros-node`, `nros-params`, `nros-rmw-cffi`,
`nros-rmw-zenoh`, `nros-rmw-xrce-cffi`) — emitted
`cargo::rerun-if-changed=<path>` for the file the variable names, and nothing
for the VARIABLE. Cargo re-runs a build script on exactly the inputs it
declares, so a script that first ran with `NROS_SIZING_DESCRIPTOR` unset (a
sizes probe or metadata pass sharing the target dir) was never re-run when a
later build of the same unit set it.

## Measured

`examples/native/cpp/talker` with `[[component]] entities = ["publisher:…",
"timer"]` in its `system.toml`, configured and built with `cmake -G Ninja
-DNROS_RMW=zenoh`. The configure wrote `nros/sizing/cpp_talker.toml` stating all
seven entity counts (one timer) and passed it to every `cargo rustc` as
`NROS_SIZING_DESCRIPTOR`. `nros-node`'s build-script `output` named no
descriptor, and the image carried:

| | `ARENA_SIZE` | `REQUIRED` | `NROS_CPP_EXECUTOR_STORAGE_SIZE` |
| --- | --- | --- | --- |
| before (variable not watched) | 74,240 | 0 | 90,624 |
| after (`rerun-if-env-changed` emitted) | 8,192 | 2,112 | 24,576 |

Same build directory, incremental — the fix is an EDGE, not a wipe.

## Fix

`from_build_env` always emits `cargo::rerun-if-env-changed=NROS_SIZING_DESCRIPTOR`
before reading it, set or not. One loader, so all five consumers are covered;
`git grep 'from_build_env\|DESCRIPTOR_ENV' -- 'packages/**/build.rs'` is the
sweep (no consumer reads the variable another way).

Test: `the_descriptor_variable_is_watched_even_when_unset` (unset, empty, and
set — where the variable edge comes before the path edge).
