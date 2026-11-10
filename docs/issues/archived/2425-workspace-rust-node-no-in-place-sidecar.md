---
id: 2425
title: "A Rust node package in a workspace gets no source-metadata sidecar, so its in-place subscriptions are priced as buffered and a migrated image's arena more than doubles"
status: resolved
type: bug
severity: medium
area: [cli, sizing, orchestration]
related: [1340, 1305, 1594, 1639]
found: 2026-11-10
resolved: 2026-11-10
---

**Home: [phase-531](../../roadmap/phase-531-workspace-first-project-layout.md)**, W7 step-3 prep. It blocks migrating `packages/testing/nros-tests/bins/in-place-subscriptions`, issue 1340's measurement witness.

## Measured

`bins/in-place-subscriptions` registers eight `KEEP_LAST(1)` subscriptions in
place, stated in `system.contract.yaml`. Built both ways on `main` at
`29bbc628ec`, image `linux`, `producers = false` in both:

| | leaf (`nros build linux`) | migrated workspace (`nros build in_place_subscriptions_bringup:linux`) |
| --- | --- | --- |
| sizing descriptor `[[endpoint]] registration_path` | `in_place` on all 8 | refused on all 8 |
| `EXECUTOR_BACKING` | 20,160 B | 44,736 B |
| source-metadata sidecar | `metadata/eight_listener.json` | none written |

The migration was `nros dev migrate-bringup` in place, which gave
`src/in_place_subscriptions` (node) and `src/in_place_subscriptions_bringup`.
The workspace descriptor's refusal reads: "no per-endpoint observation was
joined onto this row: the observation lives in the workspace's metadata-probe
sidecars, and none that is CURRENT could be attributed to this endpoint".

## Cause (what is known)

The leaf's configure runs the source-metadata harness for its component
(`sync: source metadata — 1 rebuilt`) and writes `metadata/eight_listener.json`,
which carries each subscription's observed `in_place`. The workspace configure
of the same component writes no sidecar at all: its log has no
`source metadata` line. With nothing to join, the contract join refuses
`registration_path` per row, which RFC-0100 rules a buffered row (it keeps
its receive region). That is correct for an unobserved row; what is missing
is the observation.

Not yet established: whether the workspace road's candidate scan
(`metadata_refresh::refresh_stale_sidecars`) does not see a Rust node package
that has no `nros.toml` / `[package.metadata.nros]` component declaration, or
whether it sees it and writes the sidecar somewhere the contract join does not
read.

## What would close it

A Rust node package in a workspace gets the same per-endpoint observation the
leaf road gave it, and the migrated `in-place-subscriptions` image prices its
eight rows `in_place` with an arena equal to the leaf's (20,160 B on this
host). Then the bin migrates, and issue 1340's measurement recipe names the
workspace image.

## Found with

The `producers = false` half of the same measurement was a separate defect,
fixed on the way: the generated workspace entry wrote the host board's
census and profile features whatever the image said
(`EntrySpec::host_producers`, phase-531 W7 slice 1).

## Resolution

Two defects, and the arena needed both fixed.

1. **Discovery.** `Workspace::discover` made a Rust package a component only
   from its own manifest (`[package.metadata.nros.node]`), or from a leaf's
   `system.toml` (issue 1603). A bringup's `[[component]] pkg = "<p>"` row was
   never read for it. `leaf_system::read` answers only beside a manifest, and
   a bringup has none. Discovery now reads those rows for the workspace's node
   packages (`bringup_declared_node_summaries`, through `declaring_system`,
   which reads a bringup by its first `[image.<id>]`). Measured on a migrated
   copy of `bins/in-place-subscriptions`: `sync: source metadata — 1 rebuilt`,
   and registration observed for 8 of 8 rows (was 0 of 8).
2. **`producers = false` ignored by the generated entry.** It wrote the host
   board's `census` feature anyway, which links `nros-rmw-metadata`, a
   `buffered-dispatch` backend. So `nros-node` priced every in-place row as
   buffered even with the sidecar (issue 1577's build warning said so). Fixed
   by `EntrySpec::host_producers` in #3485.

With both: `EXECUTOR_BACKING` 20,160 B, equal to the leaf (was 44,736 B).

**Sweep.** One other tracked package had the gap:
`examples/zephyr/rust/talker/src/zephyr_talker_node`. Its manifest says the
probe answers for it with `metadata/talker.json`, but configure had written no
sidecar since it became a workspace. Now it does.

**Class guard.** `Workspace::undeclared_component_rows` lists every bringup row
naming a workspace package that no declaration covers. `nros configure`
prints each one as "source metadata — no producer for …", and the CLI test
`every_tracked_bringup_component_row_reaches_a_declaration` fails on any
tracked workspace that has one. Mutation-checked: with the discovery fix
disabled, the test names `zephyr_talker_node`.

Unblocked: `bins/in-place-subscriptions` can migrate with phase-531 W7's
step-3 prep slices.
