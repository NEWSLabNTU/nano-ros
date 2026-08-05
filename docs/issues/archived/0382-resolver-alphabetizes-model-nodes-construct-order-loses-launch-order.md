---
id: 0382
title: Resolver serializes model nodes alphabetized — entry construct order
  no longer follows launch declaration order
status: resolved  # fixed 2026-08-05
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

## Retracted (2026-08-05) — the 2026-08-04 "upstream, not fixable here" analysis was WRONG

That entry claimed `structure.nodes` was still a `BTreeMap` in
`ros-launch-manifest`, that launch order was unrecoverable downstream, and that
closing this needed a three-repo change (rlm schema + tag, resolver, dep bump).
All of it followed from one bad read.

It inspected `~/.cargo/git/checkouts/ros-launch-manifest-*/172aa53/model/src/lib.rs`.
`172aa53` is **v0.1.0** — a stale cargo checkout of an old tag left in the
cache. The manifests pin **v0.1.4**, where the field has been
`IndexMap<String, NodeInstance>` since `62e90af` (released v0.1.3). The fix was
already in the build; nothing upstream was needed.

Read the dependency the MANIFEST names, not whichever checkout happens to sit in
`~/.cargo/git/checkouts`. `git describe --tags <rev>` in the source repo settles
which tag a checkout is, in one command, and would have prevented the whole
detour.

The wrong sections are removed rather than left with a footnote: they were a
confident, specific cross-repo plan, and the next reader would have started
executing it well before reaching any correction.

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

## Resolution (2026-08-05)

All three steps landed. Steps 1 and 2 upstream — `ros-launch-manifest` v0.1.4
carries `nodes: IndexMap<String, NodeInstance>` (the order-preserving form, not
the additive `declaration_index`), and the resolver populates it. Step 3 was the
tag bump, already in `nros-orchestration-ir`; no consumer-side sort is needed
because the map now IS the order.

`cpp_multi_node_entry::multi_node_workspace_cpp_typed_configures_and_builds` —
the test this issue was filed from — passes: the generated TU constructs
`talker_pkg::Talker` as `__nros_comp_0` and `listener_pkg::Listener` as
`__nros_comp_1`, matching the launch XML rather than the alphabet.

The regeneration hazard this issue flagged turned out to be the load-bearing
part. The committed TU was stale (generated 2026-08-04 07:34, before the pin
moved) and still showed listener-first, so the test kept failing after the fix
had landed. Regenerating it needed `nros codegen entry`, which died on the
model phase-330 W4 had deleted — see `archived/0414`, whose CMake half was fixed
in the same change as this one.

One consumer-side assertion moved with it: `entry_typed_plan` asserted the
ALPHABETICAL order and reasoned in a comment that the model "has no launch order
to preserve". It does now.
