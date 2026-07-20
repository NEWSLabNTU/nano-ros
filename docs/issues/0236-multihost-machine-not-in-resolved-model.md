---
id: 236
title: "play_launch resolve drops `<node machine=>` — multi-host workspaces can't migrate to the model path"
status: open
type: enhancement
area: build
related: [phase-296, rfc-0050, rfc-0052]
---

## Summary

`play_launch resolve` does not carry a node's `<node machine="…">` target
host into the resolved SystemModel. The `play_launch_parser` layer captures
it (`record/types.rs` `NodeRecord.machine`, populated in
`record/generator.rs`), but it is **dropped at the `launch_dump` layer**
(play_launch's `src/ros/launch_dump.rs` node record has no `machine` field),
so `model_builder.rs` never sees it and never maps it to
`execution.deploy[fqn].host`. The resolved model therefore has no per-node
host, and a multi-host launch collapses to a single unhosted graph.

This blocks the phase-296 R4 migration of any multi-host workspace — concretely
`examples/workspaces/rust/src/native_entry_robot1` / `native_entry_robot2`,
which bake with `nros::main!(model = "demo_bringup:config/multihost_model.yaml",
host = "robotN")`. The `host` filter needs the per-node machine in the model to
keep only that host's (plus unhosted) nodes; without it the robot entries stay
on the deprecated `launch` arm.

## Repro

```
play_launch resolve examples/workspaces/rust/src/demo_bringup/launch/multihost.launch.xml \
    -o /tmp/mh.yaml
# resolved model: nodes present, but no `deploy.*.host` / node machine field —
# the two robots' partition is indistinguishable.
```

nano-ros's own `nros-launch-parser` DOES carry the machine (its launch arm
bakes the per-host partition via `Plan::for_host` / the `host =` macro arg), so
this is another play_launch/nano-ros parser-fidelity divergence (cf. the
`<group ns=>` fix, archived once landed) — the resolved model must reproduce
what the launch arm produces.

## Fix (spans three layers)

1. **launch_dump** (play_launch `src/ros/launch_dump.rs`): add `machine:
   Option<String>` to the node record so it deserializes from the parser's
   `NodeRecord.machine`.
2. **model_builder** (play_launch `src/ros/model_builder.rs`): map
   `n.machine` → `execution.deploy[fqn].host` (create the deploy entry with
   `host` set when a machine is present; leave `target` for the system-config
   pass to fill / default).
3. **nano-ros model arm** (`nros-macros` + `plan_from_model`): the `host =`
   filter reads `execution.deploy[fqn].host` — verify it keeps `host`-matching
   + unhosted nodes (mirror the launch arm's `Plan::for_host`).

Add a resolve golden test (multihost.launch.xml → two distinct host
partitions) + a nano-ros model-arm host-slice test.

## Workaround

The robot/multi-host entries stay on `nros::main!(launch = …, host = …)` (the
deprecated-but-working arm) until this lands. Single-host entries of the same
workspace migrate normally (phase-296 R4: the monolith's 7 single-host native
entries are already on the model path).

## Cross-track update (play_launch, 2026-07-20)

play_launch is landing steps 1+2 (the play_launch side) now, as **Phase 46.1**
of the unified-SystemModel work — `launch_dump` gains `machine`, `model_builder`
maps it to `execution.deploy[fqn].host`, plus a resolve golden test
(multihost.launch.xml → two host partitions). This ships ahead of the rest of
Phase 46 specifically to unblock the phase-296 R4 multihost migration.

**Step 3 is yours:** verify the nano-ros model arm's `host =` filter reads
`execution.deploy[fqn].host` and keeps host-matching + unhosted nodes (mirror
the launch arm's `Plan::for_host`).

Context — play_launch's broader design this is part of:
**Unified SystemModel** (play_launch `docs/design/unified-system-model.md`,
`docs/roadmap/phase-46-unified_system_model.md`): the two artifacts
(`system_model.yaml` + `record.json`) merge into ONE complete model carrying
all launch info; each consumer derives its own platform specifics. See the
RFC-0050 note below — please confirm you've reviewed it and flag any field the
model still omits for your bake.
