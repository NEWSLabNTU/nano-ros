---
id: 2563
title: "Two-tiers lab, Rust tab: the one-sched-task run prints `10 windows, 0 late`, not `10 late`"
status: open
type: bug
severity: medium
area: [realtime, book, probe]
related: [issue-2269, phase-537]
found: 2026-11-15
---

**Home: [phase-537](../roadmap/phase-537-book-user-guide-organization.md).**
The `lab-tiers` probe track (Rust, weekly) fails at its first run step.

## What

The lab ([two-tiers.md](../../book/src/tutorial/two-tiers.md)) puts a 100 Hz
control loop and a 60 ms telemetry job on one sched task, with
`group_tasks = { ctrl = "main" }` (and the telemetry group on `main` too), and
expects the loop to miss its deadlines: `10 windows, 10 late`. Then it moves
the loop to a higher-priority task and expects `10 windows, 0 late`.

In the Rust tab, on mps2-an385-freertos, the first run already prints
`10 windows, 0 late`, so the lab's lesson does not show and the track's
`probe-expect` for that step fails.

## Observed (2026-11-15)

- Reproduced on the book pass branch and on the tree before it, with the
  in-tree CLI on the host (not the docker probe).
- The build log of the one-task run shows a derived schedule with two tasks:
  `derived schedule: sched task derived-control_node priority 3 /
  derived-telemetry_node priority 2`. That reads as if the Rust build derives
  a task per node and does not apply `group_tasks = { … = "main" }`, which
  would put the loop on its own higher-priority task and explain `0 late`.
  This is a reading of the log, not a proven cause.
- The C and C++ tabs were not re-run.

## Code trace (2026-11-19, read only, main at 685656d351)

No Rust-only path skips `group_tasks`, and the lab's config is spelled
right.

- `group_tasks` becomes `execution.bindings["<fqn>/<group>"] = task` in
  ros-launch-manifest (`model/src/system_config.rs`). An unmatched component
  name only produces a diagnostic.
- The Rust road (`nros::main!`, `nros-macros/src/main_macro.rs`) and the
  C/C++ road (`sched_task_resolver.rs`, `codegen/entry/mod.rs`) both merge
  groups through `nros-orchestration-ir/src/entry_groups.rs`
  `node_callback_groups`. A binding moves only a group the code declares
  under exactly that id. A bound id the code does not declare is dropped
  without a word.
- `derived-<node>` tasks (`derive.rs`) appear only for groups still on
  `"default"`. With `ctrl` and `telem` bound to `main`, derivation should be
  declined. Seeing both derived tasks means neither binding reached the
  model the macro read.

Two candidates, both inferred, not proven:

- **(a) A stale model.** `ensure_model` (`model_location.rs`) reuses any
  model a build already produced. A fresh resolve reads the mirror
  `<ws>/build/nros/launch/<bringup>/system.toml`, which only `nros sync`
  writes. A model or mirror from before the lab's `group_tasks` edit has no
  bindings and no `[tasks.main]`, which matches the log.
- **(b) A group-id mismatch.** The sidecar (`callbacks[].group`, written by
  `nros/src/node_metadata.rs`) spells the group differently from `ctrl`, so
  the binding is dropped.

Next: after the lab's one-task build, read `execution.bindings` and
`execution.tasks` in `<ws>/build/nros/models/<bringup>/system_model.yaml`
and in the mirror, and `group` in the Rust sidecars. If (a) is the cause,
the freshness check in `ensure_model` must cover the mirror and the built
model against the bringup's own `system.toml`. If (b), fix the sidecar
writer. Either way, warn when a bound group id is not among the code's
declared groups.

## Re-run (2026-11-19, main at 685656d351, host, in-tree CLI)

The lab's own steps (the `lab-tiers` track's steps 2110 to 2160, extracted
with `PROBE_EXTRACT_ONLY`), then `nros run`, no longer show the bug:

- the build prints `derived schedule: declined - every callback group's
  sched task is authored for freertos`;
- the model has `bindings: /control_node/ctrl -> main,
  /telemetry_node/telem -> main` and one task, `main`;
- the sidecars record the groups as `ctrl` and `telem`;
- the run prints `10 windows, 10 late`, as the lab says.

So neither candidate above holds on this tree; a change since 2026-11-15
fixed it. What is left is plan step 3 on the installed track (the docker
probe), which is what failed weekly.

## Plan

1. Decide which is wrong: the Rust road's handling of `group_tasks`, or the
   lab's assumption about how a Rust image is scheduled.
2. Fix that side; the lab's two runs must print `10 late` then `0 late` in
   every language.
3. Run the `lab-tiers`, `lab-tiers-c` and `lab-tiers-cpp` tracks.
