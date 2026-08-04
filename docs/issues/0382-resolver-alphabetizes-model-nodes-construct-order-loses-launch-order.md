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

## Located (2026-08-04) — it is an UPSTREAM type, not a nano-ros bug

The alphabetization is structural, in the schema crate:

    ros-launch-manifest / model/src/lib.rs:163
        pub nodes: BTreeMap<String, NodeInstance>,

A `BTreeMap` serializes in key order, so `/listener` precedes `/talker` no
matter what the launch file said. `structure.scopes`, `topics`, `services` are
the same shape; `nodes` is the one whose order is semantic.

**Launch order is not recoverable downstream.** The model carries no index: a
node instance has `scope`, `pkg`, `exec`, `params`, `param_sources`,
`node_name` — nothing ordinal. nano-ros's entry emitter iterates
`model.structure.nodes` directly (`codegen/entry/mod.rs:369`), so it inherits
the map order and has nothing else to sort by. The information is destroyed at
resolve time, which is why no consumer-side fix exists.

**Deliberately NOT worked around in-tree.** The tempting stopgap is to order
construct calls by the `[[component]]` sequence in `system.toml`, which nano-ros
does own. It would make this test pass. It would also silently substitute a
DIFFERENT ordering semantic — authored component order, not launch declaration
order — and the two can disagree without anyone noticing. That is the
plausible-but-wrong class this repo keeps paying for; a green test would hide it.

## The change, and where each part lands

Three repos, in order:

1. **ros-launch-manifest** (tag-pinned dep, currently `v0.1.2`) — either
   `nodes: IndexMap<String, NodeInstance>` (preserves insertion order; adds an
   `indexmap` dep and changes a public type), or additively a
   `declaration_index: u32` on `NodeInstance` (schema stays a mapping, consumers
   sort). The additive form is the smaller blast radius. New tag either way.
2. **play_launch / ros-launch-resolve** — populate the order. The resolver knows
   declaration order at parse time; today it simply drops it. Add the
   resolver-level test the fix shape below asks for (a launch file with
   deliberately non-alphabetical node names -> model preserves order) so the
   property is owned where it can break.
3. **nano-ros** — bump the dep, and if the additive form is chosen, sort by
   `declaration_index` in `plan_from_model`.

Not landed here: steps 1 and 2 are pushes to repos outside this checkout, and
the tag bump is a maintainer decision. Step 3 is a two-line change once the tag
exists.

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
