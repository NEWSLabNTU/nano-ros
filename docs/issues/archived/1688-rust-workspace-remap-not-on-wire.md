---
id: 1688
title: "A Rust workspace entry's launch `<remap>` does not reach the wire —
  `/remapped_out` never receives a sample"
status: resolved
type: bug
area: [codegen, testing]
severity: medium
found: 2026-10-05
related: [1651, 1684, 1687, 1270, phase-480]
---

## Failing test

`nros-tests::workspace_features_e2e workspace_features::case_03_rust_remap`:

    [rust remap] /remapped_out never received 3 samples — the launch/model <remap>
    did not reach the wire (the publisher is on the wrong name)

## Evidence

- CI: workflow_dispatch run 37252649866, tree of commit
  dc5691a584cfb9f2a53b4b92372370d81ee8cd38.
- Local, SOLO (`-j1`), fixtures built from that tree 2026-10-05 02:43–03:24
  UTC by `just build-test-fixtures lane=tier1`: red again (22 s), so not load
  and not a stale fixture.

Possibly the same layer as issue 1687 (per-entity configuration from the model
not reaching the live entity) — filed separately because remaps and QoS take
different paths (`runtime.remaps` vs the QoS profile) and only a diagnosis can
join them. Not bisected; no lane ran this between 2026-06-17 and now (issue
1651).

## Acceptance

The cell passes solo and in the tier-1 `test-all`.

## Resolution — 2026-10-06 (phase-480 W1)

**Not a remap defect, and not the same layer as issue 1687.** Run by hand
against `rmw_zenohd`, `native_rust_remap_entry` printed

    nros: application error: Capability { name: "lifecycle", reason: "Transport::InvalidConfig" }

and exited before publishing once — on any name. `InvalidConfig` is the zenoh
shim's spelling of `ZpicoError::Full`. The image had been built with
`ZPICO_MAX_PUBLISHERS=1` and `NROS_DECLARED_INFRA_QUERYABLES="param"`: its own
`/remapped_out` publisher took the one slot, and lifecycle's
`transition_event` publisher found none.

**Root cause.** The lifecycle family has two spellings in a resolved model.
`features = ["lifecycle"]` lands in `execution.features`; the typed
`[lifecycle] autostart = …` block lands on every node as
`lifecycle_autostart`. The REGISTRATION reads the second (`nros::main!`'s
`lifecycle_code`, `codegen::entry`'s `Plan::lifecycle`).
`InfraServices::from_model`, which every model road sizes through (entity
inventory, cmake `entity-facts`, `model_ingest`), read only the first.
`examples/workspaces/features` declares lifecycle with the block, so every
image there registered five lifecycle servers and a publisher into pools
sized without them (issue 1270's class, through the spelling 1270 did not
read).

**Fix** (`fix/1688-lifecycle-block-sizing`): `InfraServices::from_model` ORs
`model_declares_lifecycle_block(model)`. The opposite disagreement (feature
listed, no block) still over-counts, which is the safe direction, and is
unchanged.

**Before / after** (fixtures rebuilt from each tree, solo):

- before: `native_rust_remap` derived `ZPICO_MAX_PUBLISHERS=1`, infra `param`;
  `case_03_rust_remap` red after 22 s.
- after: `ZPICO_MAX_PUBLISHERS=2`, infra `param+lifecycle`;
  `case_03_rust_remap` PASS (7.8 s, and 3.2 s again after rebasing onto
  3a4bf2e191). `case_01_rust_lifecycle` also PASS.
- test `the_typed_lifecycle_block_sizes_the_pools_like_the_feature` (all three
  autostart values); with the OR removed it fails at `left: "none"`.

Sweep: `grep -rn -e '== "lifecycle"' -e 'lifecycle_autostart' packages/cli
packages/core --include='*.rs'` and `grep -rln '^\[lifecycle\]'
--include=system.toml examples packages` (one bringup uses the block).

**Not measured / left behind:**

- A standalone leaf (`LeafSystem`) reads `[system] features` only and has no
  typed-block field; whether a leaf can declare `[lifecycle]` at all was not
  checked.
- The `native_rust_qos` image still logs eight `liveliness: declare failed
  (Full)` lines after this fix. Re-measured: it is NOT this under-count — the
  image's infra token is `param+lifecycle` now — but the undeclared
  `ZPICO_MAX_LIVELINESS` default (16) against the 24 tokens the image needs.
  Filed as issue 1713 ([resolved](1713-liveliness-pool-default-short-for-param-and-lifecycle-images.md)).
- No tier-2 build: the change only adds lifecycle's entities to an image whose
  model carries a typed `[lifecycle]` block, and the only such bringup is the
  native-only features workspace.
