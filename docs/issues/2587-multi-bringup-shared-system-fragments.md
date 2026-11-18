---
id: 2587
title: "Two bringups in one workspace cannot share a `system.toml` fragment, so shared topology is duplicated"
status: open
type: limitation
area: [workspace, cli]
severity: low
found: 2026-11-15
related: [2556, phase-555, phase-580]
---

Filed from RFC-0024's open question 2 by phase-555 W3 (ruling A1: a design
doc goes Stable only after its open questions are filed and it points at
them).

## What is true now

A workspace may hold several bringups, each with its own `system.toml`
(RFC-0121 D2). There is no way for two of them to share a fragment, such as
the `[[domain]]` / `[[bridge]]` topology or a knob table: no `include =`
key, and no workspace-level defaults table. Every bringup states everything
itself, which is RFC-0024 §5's option (a), "duplicate".

Measured 2026-11-15 with `git ls-files`: no tracked `system.toml` has an
`include` key or a `[defaults]` table. Several example workspaces hold
several bringups: `workspaces/cpp` 5, `workspaces/c` 4, `workspaces/rust` 4,
and `workspaces/realtime-c` and `workspaces/realtime-rust` 2 each.

## Why it is only a limitation

Duplication is honest and traceable, and RFC-0024 §5 chose it deliberately
"until a real workspace hits the pain". This issue records that the question
is still open, so the RFC can be Stable without carrying it.

## Options (from RFC-0024 §5)

- (a) Duplicate (today).
- (b) An `include = "<path>"` key in `system.toml` that the resolver
  expands. This adds path-resolution rules, and every consumer of
  `system.toml` would have to read the expanded form.
- (c) A workspace-level defaults table that each bringup inherits and
  overrides. A workspace-root `nros.toml` holds only workspace-wide facts
  (RFC-0024 §5, RFC-0121 D2) and never a system definition, so shared system
  defaults there would need RFC-0121 D2 amended, or another home.

## Done when

A workspace with two bringups that share a fragment states it once and both
images build from it, or this is closed as wontfix with the reason.

## Ruled (2026-11-18)

Option (b), ruled by the user (relayed by the nros 241 manager):

- an `include = ["<path>", …]` key, expanded by the one loader;
- paths relative to the including file;
- cycles refused;
- an include may not set `[system].name`.

The merge rules are in RFC-0024 §5. Implemented by
[phase-580](../roadmap/phase-580-system-toml-include.md) in three PRs (Rust,
Python, CMake with the user-facing docs). Each PR works on its own: a road
that cannot expand includes yet refuses them.
