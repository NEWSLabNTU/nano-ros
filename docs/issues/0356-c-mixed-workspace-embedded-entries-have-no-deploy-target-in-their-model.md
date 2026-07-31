---
id: 356
title: "C/mixed workspace embedded Entry packages (freertos/nuttx) codegen-fail 'no nodes on board': their SystemModel declares only native deploy targets"
status: open
type: bug
severity: medium
area: codegen
related: [phase-296, issue-0320, issue-0355]
---

## Finding (2026-07-31, surfaced building tier-2 fixtures for #355)

`just freertos build-fixtures` (and `nuttx`, `threadx`) fails at CMake
configure with:

```
CMake Error at cmake/NanoRosEntry.cmake:668 (message):
  nano_ros_entry(LAUNCH ""): `nros codegen entry` failed (rc=1).
  ...
  stderr: SystemModel `examples/workspaces/c/src/demo_bringup/config/system_model.yaml`
          places no nodes on board `mps2-an385-freertos` — check execution.deploy targets
```

Reproduces directly against the committed model with a freshly built CLI
(`just setup-cli`):

```
nros codegen entry --lang c --workspace examples/workspaces/c \
  --model examples/workspaces/c/src/demo_bringup/config/system_model.yaml \
  --board mps2-an385-freertos --out /tmp/e.cpp --typed
# Error: SystemModel ... places no nodes on board `mps2-an385-freertos`
# nros-cli-core/src/codegen/entry/mod.rs:456
```

## Root cause

`examples/workspaces/c` contains a `src/freertos_entry/` Entry package (and the
mixed workspace likewise), migrated to the model path in phase-296-R4/M1
("migrate the 15 monolith embedded entries to the model path", 31e051009). But
the workspace's bringup declares **only native deploy targets** —
`examples/workspaces/c/src/demo_bringup/system.toml` has `[deploy.native]`,
`[deploy.robot1]`, `[deploy.robot2]`, all `target = "x86_64-unknown-linux-gnu"`
— and the committed `system_model.yaml` (regenerated portable in #320,
07650d0a1) reflects exactly that: `/talker` + `/listener` on `target: linux`,
nothing on any embedded board.

So `codegen entry --board mps2-an385-freertos` filters the model's deploy
targets by that board, finds none, and fails — correctly, given the model. The
Entry package exists and the tier-2 lane tries to build it (`workspace-mixed-freertos`
is a declared `fixtures.toml` coordinate), but the model has no deploy target for
its board. The **rust** freertos fixture is unaffected because it lives in a
different workspace (`examples/workspaces/rust`, fixtures.toml
`workspace-rust-qemu-freertos`, entry `qemu_freertos_entry`) whose model does
deploy to the board.

Net: the C/mixed embedded Entry packages were migrated to consume the workspace
SystemModel, but the model (and its `system.toml` source) was never given the
embedded-board deploy targets those entries require — an entry that consumes a
model, paired with a model that deploys nothing to the entry's board.

## Impact

- `just {freertos,nuttx,threadx} build-fixtures` cannot build the C/mixed
  embedded workspace fixtures → tier-2/tier-3 `ci-matrix` cannot go green for
  those coordinates on a dev box. Distributed nightly likely hits the same wall.
- Not caught earlier because full tier-2 fixture builds are rarely run locally
  (the per-platform toolchains do not coexist on one runner), so this lane's
  breakage sat latent since 296-R4/#320.
- Unrelated to #355 (that fix is nros-c executor-spin logic); this is the
  workspace-model deploy shape.

## Fix direction

Either:
1. Add the embedded-board deploy target(s) to the C/mixed workspace bringup
   (`system.toml` `[deploy.*]` for `mps2-an385-freertos` and the nuttx board),
   so the regenerated SystemModel places the entry's nodes on that board — the
   shape `ws-realtime-c-mps2` already uses for its freertos tiers is the working
   template; then re-commit the portable model (issue-0320 flow); or
2. If the C/mixed freertos/nuttx Entry packages are vestigial (only the rust
   workspace is a real embedded fixture), remove them and drop the corresponding
   `fixtures.toml` coordinates + matrix cells so nothing tries to build them.

The distinguishing question — are C/mixed embedded workspace fixtures INTENDED
coverage? — decides which. `workspace-mixed-freertos` being a declared
`fixtures.toml` row argues for (1).

## Repro

```
just setup-cli
nros codegen entry --lang c --workspace examples/workspaces/c \
  --model examples/workspaces/c/src/demo_bringup/config/system_model.yaml \
  --board mps2-an385-freertos --out /tmp/e.cpp --typed
```
