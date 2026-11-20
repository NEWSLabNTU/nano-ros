---
id: 2767
title: "A hand-written west entry's census check refused a current model: its bringup was spelled through the entry, and the model rules took that spelling's parents"
status: resolved
type: bug
severity: high
area: [cli, build, zephyr, ci]
found: 2026-11-20
resolved: 2026-11-20
resolved_in: "fix(cli, issue 2767): a bringup path is folded before its parents are taken"
related: [issue-2435, issue-2439, issue-2736, issue-2592]
---

## What happened

`merge-runtime.yml` run 44721090137 (lane `merge`, main at `0d9429d990`) was
dispatched to accept issue 2592 on host 118's rebuilt runner. Its Zephyr shard
stopped in `just build merge`, before any test: 2 of 7 west fixtures,
`west_hand_written_c_entry` and `west_hand_written_c_entry_nros_build`, failed
at configure.

```
nano_ros_entry(...): the census and the contract disagree, or the census no longer describes the code.
Error: the model this census was taken against
Caused by:
    SystemModel `.../west_hand_written_c_entry/build/nros/models/demo_bringup/system_model.yaml`
    is stale: recorded input missing `launch/system.launch.xml`
```

## Why

The model was current. Its recorded input `launch/system.launch.xml` existed,
under the generated launch tree `<ws>/build/nros/launch/demo_bringup/`, with
the recorded sha256.

The entry names its bringup relative to itself (`-DNROS_IMAGE_SYSTEM_DIR=../demo_bringup`).
`_nros_entry_require_fresh_census` normalised that path to find the workspace
(issue 2435), then passed the UNnormalised one as `--system-toml
<ws>/src/zephyr_entry/../demo_bringup/system.toml`. `model_gate::verify` takes
the bringup from that file's parent, and `model_location::generated_launch_root`
derives the workspace as the bringup's grandparent, lexically. For the
unfolded spelling that is `<ws>/src/zephyr_entry`, so the launch input was
looked for under the entry, and read as missing.

Reproduced on a fresh `nros configure` of the fixture, at main: the census
check passes with `--system-toml <ws>/src/demo_bringup/system.toml` and fails
with the entry's spelling. So a fresh tree fails as well. The persistent
fixture tree on the runner was not the cause, though its model (written
2026-11-17) first read as a stale-model suspect.

## Fix

- `model_location::lexical` folds `.` and `..` without touching the
  filesystem, and every public function in `model_location` that derives a
  place from a bringup (`model_search_paths`, `generated_launch_root`,
  `generated_launch_dir`, `launch_dir`, `launch_root`, `resolve_system_toml`,
  `recorded_input_path`, `launch_is_generated`, `is_self_resolved`, and the
  private `model_write_dirs` and `build_scoped_dir`) folds its `bringup_dir`
  first. Every reader of a model's place goes through these, so no caller has
  to normalise.
- `_nros_entry_require_fresh_census` passes the normalised bringup it already
  computes.

Sweep: outside `model_location`, only `nros new`'s scaffolding takes a
bringup's parent, from a path the user gives.

## Measured

- Unit tests: the folding over every component shape, and an entry-relative
  bringup resolving the same generated launch root, recorded input, model
  candidates and scoped dir as the folded one. With `lexical` mutated to
  return its input, both fail.
- The rebuilt CLI's census check on the fixture passes with either spelling.

## Not covered

The tree has five other lexical normalisers (`builder/paths.rs`,
`board_descriptor.rs`, `migrate_bringup.rs`, `nros-build`'s
`board_framework.rs`, `submodule_watch.rs`). Consolidating them is a separate
change.
