---
id: 1648
title: "On the model road an OBSERVED `in_place: false` subscription on a schemaless backend still refuses its `registration_path` — the row needs the component's LANGUAGE, which the probe sidecar knows and the join does not carry"
status: open
type: tech-debt
area: [build, cli]
severity: low
found: 2026-10-03
related: [1594, 1340, 1319, 1522, rfc-0100]
---

## What

Issue 1594 joined the metadata probe's per-subscription `in_place` observation
onto the model road's rows (`contract_join::observe_registrations`). A row
observed `true` on an in-place backend now states `registration_path =
"in_place"`. A row observed `false` falls through to the buffered rows, and on a
SCHEMALESS backend (zenoh, XRCE) those turn on the entry language — a Rust
registration takes `RX_BUF` (`unbounded`), a C/C++ one its typed hint
(`typed_bound`). `write_for_model` has no ONE entry language (a workspace image
is several packages), so the row refuses.

## Why it is not urgent

The refusal is the safe direction: a refused path keeps its receive region, and
every consumer prices it at the worst case. No in-tree workspace has an observed
`false` subscription today (measured 2026-10-03 on `examples/workspaces/cpp`:
the one subscription sidecar says `in_place: true`).

## What closing it looks like

The probe sidecar states the component's language (`SourceMetadata::language`).
Carry it per ROW through the join (an `EntityDecl` field beside
`in_place_capable`, or a per-component language on `ComponentEntities`) and let
`registration_path` read the row's language before the image-wide
`DescriptorInputs::language`. Never infer the language from anything else:
phase-457 W3 removed that inference for the in-place row, and the buffered rows
deserve the same evidence.
