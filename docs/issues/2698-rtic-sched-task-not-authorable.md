---
id: 2698
title: "An RTIC image's levels can only be derived: `system.toml` has no `[tasks.<name>.rtic]` sub-table to author one"
status: open
type: limitation
severity: low
area: [executor, boards]
found: 2026-11-18
related: [phase-562, rfc-0112, rfc-0079]
---

**Home: [phase-562](../roadmap/phase-562-image-framework-field.md) W10**

## What

Since phase-562 W10 an RTIC image runs its derived sched tasks as levels, one
RTIC software task each. Their sched-task key is `rtic`, whatever RTOS family
the board has: `nros::main!` sets it for `framework = "rtic"`, and
`nros-orchestration-ir`'s `SchedTaskDef` has an `rtic` sub-table that only the
derivation fills (`install_placement`).

The launch model has no `rtic` sub-table. `ros_launch_manifest_sched::TaskDef`
carries `freertos`, `zephyr`, `threadx`, `nuttx` and `posix`, and it is
`deny_unknown_fields`, so `system.toml` cannot write `[tasks.ctrl.rtic]
priority = 3`. `sched_task_from_model` sets `rtic: None`.

## What that means

- An RTIC image's level placement is always DERIVED from the contract. An
  author cannot pin a sched task to an RTIC priority, the way RFC-0079 rule 1
  lets them on every other target.
- A sched task that authors a placement for another target only (for example
  `[tasks.ctrl.freertos]`) reaches the RTIC image as rule 3, "placed for
  another board", and resolution refuses it, naming `[tasks.ctrl.rtic]`,
  which the author cannot write.

## Possible shape

An `rtic` key in `ros-launch-manifest`'s `TaskDef` (the play_launch
repository), read by `sched_task_from_model` like the other five.
`rtic_level_plan` already refuses a table the board cannot run (tied
priorities, more levels than dispatchers, too few NVIC priorities), so an
authored priority needs no new check on the nano-ros side.
