---
id: 361
title: "Grandfathered committed SystemModels: the current nros-launch-resolve requires explicit per-block `nodes=`, so any embedded-workspace re-resolve fails 'node not placed' (and c/mixed embedded entries have no board deploy)"
status: open
type: bug
severity: medium
area: codegen
related: [phase-296, issue-0320, issue-0355, rfc-0060]
---

## UPDATE (2026-07-31) — the real root cause is broader

The symptom below (c/mixed embedded entries codegen-fail "no nodes on board") is
one face of a bigger contract break. Attempting the fix — add `[deploy.freertos]`
/ `[deploy.nuttx]` / `[deploy.threadx-linux]` to `examples/workspaces/c` and
re-run `nros ws sync` — fails at model RESOLUTION, before codegen:

```
Error: system config: node '/listener' is not placed — with multiple [deploy.*]
       blocks every node needs a `nodes = [..]` entry
       (packages/cli/.../ros-launch-resolve/resolve/src/model.rs:222)
```

**And it is not specific to the edited workspace.** Forcing the UNMODIFIED
`examples/workspaces/rust` workspace to re-resolve (any `system.toml` change →
content-addressed staleness → real re-run) produces the identical error, even
though its committed model is checked in and "works". So:

- The committed portable SystemModels (`#320`) are **grandfathered** — generated
  by an older resolver that placed nodes across `native` (default launch) +
  `robot1`/`robot2` (multihost) + embedded blocks WITHOUT explicit `nodes=`.
- The CURRENT `nros-launch-resolve` requires `nodes = [..]` on every deploy block
  once there is more than one. No committed embedded workspace declares them.
- Therefore ANY re-resolve of these workspaces fails — adding an embedded deploy
  target is just the first thing that forces one. `nros sync --check` /
  regeneration is effectively frozen against the committed models.

`kind = "embedded"` blocks are documented (rust `system.toml`) as running EVERY
node and being excluded from placement, so the ambiguity the resolver trips on is
between the `kind = "self"` machines: `native` (default `system.launch.xml` =
talker+listener) vs `robot1`/`robot2` (`multihost.launch.xml`, talker@robot1 /
listener@robot2). Those are ALTERNATIVE deployments of the same nodes; the older
resolver resolved it, the current one demands explicit partitioning.

**Fix reframed** — two real options, both bigger than a config add:
1. **Resolver** (`nros-launch-resolve`, vendored fork): restore placement for the
   `native`(default) + `robot*`(multihost) + `embedded` pattern without demanding
   explicit `nodes=` — treat multihost self-blocks as alternative placements of
   the default nodes, as the grandfathered models were built. Preferred: it
   unfreezes regeneration for every embedded workspace at once.
2. **Model authoring**: add explicit `nodes = [..]` to every deploy block in every
   embedded workspace (c, cpp, mixed, rust, ws-realtime-*). Large, and the
   alternative-deployment semantics (a node on both `native` and a `robot*`) still
   need the resolver to accept "machines are alternatives, not simultaneous".

Then the ORIGINAL c/mixed gap (below) is fixed by adding the embedded deploy
targets, which will finally resolve. Verification needs the embedded toolchains
(a full `just {freertos,nuttx,threadx} build-fixtures`).

---

## Original finding (2026-07-31, surfaced building tier-2 fixtures for #355)

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
