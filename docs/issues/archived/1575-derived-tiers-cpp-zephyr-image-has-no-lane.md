---
id: 1575
title: "The derived-tiers-cpp Zephyr image has no lane, and without `nros sync` its configure derives no tiers and says nothing"
status: resolved
type: test-gap
area: [testing, zephyr, codegen]
severity: medium
found: 2026-09-29
related: [issue-1551, issue-1426, issue-1288, phase-459]
---

## What is missing

Issue 1551 was measured on `examples/workspaces/derived-tiers-cpp`'s Zephyr
entry (`native_sim/native/64`, C++, zenoh): four tiers are derived from its
SystemModel, and it was the image that ran the platform heap dry. Both of
1551's defects are fixed (#1436, #1432), and a hand-run boot shows all four
tiers ticking. **No lane boots this image**, though. It has no
`examples/fixtures.toml` row and no `matrix::CELLS` cell, so the regression
1551 fixed can come back unseen. 1551's acceptance asked for such a lane; it
closed without one, and this issue carries that item.

## The silent half

The configure names `src/demo_bringup/config/system_model.yaml`. That file is
a BUILD ARTIFACT that only `nros sync` writes (phase-330 W4.a). When it is
absent, the tier derivation returns **nothing, silently**, and the image boots
as a single executor. Its one executor's storage is already static, so a
single-executor boot proves nothing about tiers. A report that "the image
boots" on an unsynced tree is therefore evidence of nothing, and 1551's own
investigation was misled by this once (see its "Sync is load-bearing"
paragraph).

## Acceptance

- A `fixtures.toml` row for the west entry
  `examples/workspaces/derived-tiers-cpp/src/zephyr_entry`: board
  `native_sim/native/64`, `lang cpp`, `rmw zenoh`, confs
  `prj.conf;prj-zenoh.conf` plus the NSOS fragment. Its build runs `nros sync`
  on the workspace BEFORE configure.
- A `matrix::CELLS` cell, with the `lane_scope` registries kept in step. Under
  a router, it asserts that all four nodes tick (or appear in
  `ros2 node list`).
- A configure that names a SystemModel which does not exist fails and names
  `nros sync`, instead of deriving zero tiers. If some caller legitimately
  wants "no model ⇒ no tiers", that must be an explicit opt-out, not the
  fallback.

## Resolution

All three acceptance items landed.

**The silent half — a configure that names a missing model now refuses.**
Two sites could turn "no model" into "no tiers", and both are fixed:

- `nano_ros_entry` (`cmake/NanoRosEntry.cmake`) FATALs when the path
  `nros model-path` returns does not exist, naming `nros sync`. Before `sync`
  that path is the source-tree rung no producer writes, and the entity facts,
  the census and the codegen all read its absence as "nothing to do".
  `-DNROS_ALLOW_UNSYNCED_MODEL=ON` (or the env var) is the explicit opt-out; it
  WARNs, and nothing in the tree sets it.
- `codegen entry`'s `derive_entry_tiers` returned `Ok(0)` when the model FILE
  was absent — the one case where the plan had a model, because
  `plan_from_model` loads through `load_model`, which resolves a missing path on
  the fly. It now asks the same `load_model`, so the tiers come from the model
  the nodes came from, or the error is loud.

Left out of this sweep on purpose: `nros codegen-system` also bakes with no
model when discovery finds none, but that is a documented model-less road the
vendor seams use, not an entry derivation.

**The lane.** `examples/fixtures.toml` row `workspace-zephyr-cpp-derived-tiers`
(west leaf `build-ws-cpp-derived-tiers-entry-zenoh`, native_sim/native/64,
cpp, zenoh, `prj.conf;prj-zenoh.conf` plus the NSOS fragment the emitter
appends, locator 7691 shared with the authored cpp row — one cell, run in
sequence). `just zephyr build-fixtures` runs `nros sync` on the workspace in
its workspace-entry prep, since no native row syncs it as a side effect. The
cell is the existing `(ZephyrNativeSim, Cpp, Zenoh, RealtimeTiers, Workspace,
Runtime)` coordinate — like the Rust derived row, a second image under one
cell — consumed by `realtime_tiers_e2e` as `zephyr/cpp-derived` with a new
`Proof::ConsoleTicks`: under a router, each of the four nodes must print
`[<node>] tick=1` (it ticked at least twice), order-independent over the
accumulated console. No `lane_scope` registry change: that registry is per
platform, and Zephyr was already admitted.

### Measured

- Unsynced, direct `west build --cmake-only` of `src/zephyr_entry`:
  `CMake Error … the SystemModel for bringup … does not exist:
  …/demo_bringup/config/system_model.yaml … nros sync`.
- After `nros sync`: configures, and the generated entry carries
  `tier[0..3]` `derived-<node>` setups. Hand boot under `rmw_zenohd`: all four
  nodes print `tick=` lines.
- Through the lane (`NROS_ZEPHYR_FIXTURE_FILTER=build-ws-cpp-derived-tiers-entry-zenoh`
  on `just zephyr build-fixtures`): the prep synced the workspace and the leaf
  built. The lane's exit was 1 over an unrelated compile-check west fixture
  (`zephyr_self_pkg_sibling`, "No SOURCES given to target: app").
- `realtime_tiers_e2e` against that image: `zephyr/cpp-derived` PASSED (the
  run's other rows failed MISSING/STALE for fixtures this worktree never
  built). Negative control: renaming one expected node to `mrm_handler_x` makes
  it FAIL with `` node `mrm_handler_x` never ticked twice `` and the console.
- `matrix_fixture_coverage` 10/10 (G1–G5, `fixture_rows_all_modeled_by_matrix`),
  `check west-leaf-vocabulary` (74 names, all modelled), `fixture-require`,
  `fixture-id-guard`, `fixture-artifact-dir-inputs` OK.
