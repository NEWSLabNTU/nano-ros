---
id: 1969
title: "The C/C++ metadata probe's executor has the build default's 4 callback slots, so a larger component fails its probe with ExecutorFull"
status: resolved
type: bug
area: [build, codegen]
severity: medium
found: 2026-10-19
resolved: 2026-10-19
resolved_in: "branch fix/metadata-probe-executor"
related: [0288, 1469, 1578, 1593]
---

## What happens

`nros sync` on the Autoware safety island (four `SHAPE rclcpp` components,
one workspace, four bringups):

```
sync: source metadata -- no producer for autoware_mrm_handler::mrm_handler
  (metadata probe run failed for `autoware_mrm_handler::mrm_handler` (exit 250):
[ERROR] nros: NodeError::ExecutorFull
node "mrm_handler": FAILED at create_subscription_in (code=-6)
[nros] FATAL: node "mrm_handler" failed to construct at create_subscription_in (code=-6)
```

The build proceeds without the sidecar, so every consumer of the recorded
inventory (`floor_by_recorded`, the probe-row road) is short of the
component. Before nano-ros 941132a82c the same sync also failed the three other
components at `declare_parameter` (exit 240, `code=-16`, a probe built
without the parameter store); that half is already gone on main.

## Why

The probe is the census: it runs the component's constructor and records what
it registers. Its executor is sized by the build defaults, and
`NROS_EXECUTOR_MAX_CBS` defaults to 4 in `nros-node`'s build script: the
probe project has no entity inventory to derive a number from, because it is
the thing that produces one. `mrm_handler` registers nine callback entities
(six subscriptions, two service clients, one timer) and the fifth
subscription is refused.

## Fix

`metadata_probe_cmake::pin_probe_env`, the environment every probe `cmake`
invocation (configure and build) carries, sets `NROS_EXECUTOR_MAX_CBS` to
`PROBE_EXECUTOR_MAX_CBS` = 64, the executor's ready-set ceiling. The env road
is part of the shared cargo directory's knob key (`nros_knob_key_fields`),
so the probe's runtime build gets a directory of its own and no firmware
build sees the value. The negative marker is keyed on the CLI's source stamp
(`probe_inputs_key`), so an existing workspace's `.unprobeable` marker is
retried on the next sync.

## Verified

- `metadata_probe_cmake::tests::the_probe_executor_is_sized_at_the_ready_set_ceiling`
- the safety island: `just sync` -> `source metadata -- 4 rebuilt, 0 already
  current`, no `ExecutorFull`; `src/autoware_mrm_handler/metadata/mrm_handler.json`
  written.
