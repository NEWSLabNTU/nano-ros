---
id: 1827
title: "A workspace with two bringups gets no board facts, so its FreeRTOS images build without `FREERTOS_PORT`"
status: resolved
type: bug
area: [cmake, cli, boards]
severity: high
found: 2026-10-12
related: [1263, phase-484]
---

## Measured

`just freertos build-examples` with no SDK variables exported (phase-484 W4b-2
rebuild) failed in `examples/workspaces/realtime-c`:

```
thread 'main' panicked at packages/rmw/zenoh/nros-zpico-build/src/runner.rs:3012:13:
FREERTOS_PORT not set. FreeRTOS portable layer.
```

The configure had said why, as a `STATUS` line:

```
-- nano-ros: board facts NOT delivered from …/examples/workspaces/realtime-c —
   Error: …/realtime-c: 2 bringups carry a system.toml; point --path at one
```

`examples/workspaces/c` (one bringup) delivered 8 values the same run.

## Cause

`nros_resolve_board_facts` asked `nros ws board-facts` about the WORKSPACE
(`NROS_WORKSPACE_DIR`). The verb answers that only while the workspace has one
bringup; with two (`demo_bringup`, `smp_bringup`) it refuses as ambiguous, and
the cmake side soft-skips every refusal it cannot name as a configuration
error. Since phase-484 W2b moved `FREERTOS_PORT` off the repo-wide export and
onto board facts, that skip means no port. The same run asked about the
bringup dir directly and got the full answer.

## Fix

`nros build` already knows the bringup it is building. It passes
`-DNROS_BRINGUP_DIR=<dir>` to the configure, and the resolver asks that dir
first, before the workspace. Lanes that do not go through `nros build` set
nothing and keep the old order.
