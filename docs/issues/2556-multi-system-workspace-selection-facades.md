---
id: 2556
title: "A workspace with more than one bringup gets no selection facades, so a capability cannot travel with its entry"
status: open
type: limitation
area: [cli, workspace, codegen]
severity: low
found: 2026-11-14
related: [2557, phase-555]
---

Filed from RFC-0066's open questions by phase-555 W3 (ruling A1: a design doc
goes Stable only after its open questions are filed and it points at them).

## What is true now

`nros configure` generates per-entry selection facades only for a workspace with
exactly one bringup (one `system.toml`). With several it says so and leaves
entry manifests on their hand-written features
(`packages/cli/nros-cli-core/src/cmd/ws.rs`, the `bringups` match in the
facade step: "selection facades are not generated for multi-system
workspaces"). C/C++ entries write one `NROS_FEATURES` per configure (issue
2557), so a workspace has one capability set whatever its languages.

## Why it matters

RFC-0066 R2 (capability sets are per workspace, so a feature demo is a node
package in `workspaces/features/`) is correct under this limit, and does not
depend on lifting it. Lifting it would let a capability travel with its entry,
the image-level granularity `NanoRosFeatureSet.cmake` already claims, and
would make RFC-0066's rejected per-language fold (R1) viable.

## Where it belongs

Multi-system workspaces are RFC-0121's layout question. phase-531 (the
workspace-first layout) is where the facade shape is reworked; no work item
there names this yet.

## Done when

A workspace with two bringups gets a facade per entry, each carrying its own
system's capabilities, or RFC-0121 records that one system per workspace is
the design and RFC-0066's open question is closed against it.
