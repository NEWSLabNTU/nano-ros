---
id: 2425
title: "A Rust node package in a workspace gets no source-metadata sidecar, so its in-place subscriptions are priced as buffered and a migrated image's arena more than doubles"
status: open
type: bug
severity: medium
area: [cli, sizing, orchestration]
related: [phase-531, 1340, 1305, 1594, 1639]
found: 2026-11-10
---

**Home: [phase-531](../roadmap/phase-531-workspace-first-project-layout.md)**, W7 step-3 prep. It blocks migrating `packages/testing/nros-tests/bins/in-place-subscriptions`, issue 1340's measurement witness.

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
