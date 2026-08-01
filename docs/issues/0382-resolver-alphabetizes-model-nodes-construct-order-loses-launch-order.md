---
id: 0382
title: Resolver serializes model nodes alphabetized — entry construct order
  no longer follows launch declaration order
status: open
severity: medium
created: 2026-08-01
tags: [orchestration, codegen, system-model]
related: [0381]
phases: [phase-296]
---

## Symptom

`cpp_multi_node_entry::multi_node_workspace_cpp_typed_configures_and_builds`
asserts the generated entry TU constructs components in launch declaration
order (talker before listener — the launch XML order). The fixture's TU
constructs listener first.

## Root cause

The committed model for `examples/templates/multi-node-workspace-cpp`
(produced by the phase-296 R4/M2 migration and every re-resolve since)
serializes `structure.nodes` as a YAML mapping in ALPHABETICAL order
(`/listener`, `/talker`); launch declaration order is lost at resolve
time, and the entry emitter iterates the mapping in file order.

Construct order is semantic: components initialize (and `configure()`) in
that order, and the launch file is where the user expresses it.

## Fix shape

Either the resolver preserves declaration order in the emitted mapping
(YAML mappings are ordered in practice; serde_yaml/py yaml keep insertion
order), or the model grows an explicit order (e.g. a `structure.order`
list or per-node `index:`) that the emitter sorts by. Preference: preserve
declaration order in the mapping — no schema change, and every existing
consumer already iterates file order. Add a test at the resolver level
(launch with deliberately non-alphabetical node names → model preserves
order) so the property is owned where it can break.

Note: fixing this requires re-resolving affected committed models (the
alphabetization is baked into files on disk), which collides with issue
0380's regeneration hazard — land 0370's guard/schema decision first or
regenerate with care.
