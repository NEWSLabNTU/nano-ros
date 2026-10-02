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

## Revised direction (2026-10-03, RFC-0100 Amendment 1)

Two things changed the answer above.

* **The language is no longer the probe's alone.** phase-474 put each node's
  component KIND on `LoweredNode` (`c` / `rust` / `rclcpp` / `configure`, read
  from `nros-metadata.json`'s `lang` — `codegen::entry::lower::component_kind`),
  so the model road has a per-COMPONENT language from the same plan the entry is
  generated from, with no join through a sidecar.
* **But a language is still a proxy.** Issue 1319's table has a C/C++ row with
  no type hint that takes `RX_BUF` exactly like the Rust generic registration —
  so "C/C++ ⇒ `typed_bound`" is an inference of the kind W3 removed for the
  in-place row.

So the preferred fix is the W3 shape applied to the buffered rows: the
registration funnel already computes the slot size it claims, so the probe
records WHICH buffered row (`typed_bound` / `unbounded`) each subscription took,
beside `in_place`, and the join carries that observation. Language disappears
from `registration_path` entirely. The per-component language from the plan is
the fallback for a row the probe did not observe — never the image-wide
`DescriptorInputs::language`, which a model image of several packages cannot
have.

In an N:1 cmake configure (RFC-0100 D12) a component's observation is per
component, so it unions without conflict; if two entries' sidecars ever disagree
for one component, the row refuses (D12 rule 2).

Files: the probe sidecar schema (`nros::node_metadata`, a version bump), the
registration funnel that reports `in_place` today, `contract_join`, and
`sizing_descriptor::registration_path`. No overlap with issues 1608 / 1647.
