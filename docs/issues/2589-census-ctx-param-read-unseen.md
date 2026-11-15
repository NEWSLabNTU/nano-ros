---
id: 2589
title: "The entity census does not see a Rust node's `ctx.param()` read, so its contract row reads `param-phantom`"
status: open
type: bug
area: [api, cli, census]
severity: low
found: 2026-11-15
related: [phase-478, issue-2568]
---

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md).**
Split from [issue 2568](archived/2568-rust-ctx-param-misses-launch-values.md),
whose `ctx.param()` half is fixed.

## What happens

The census records a parameter only when a node declares it, through
`census_hooks::on_param_declare`. A Rust node that reads a launch value with
`ctx.param(name)` (`NodeContext::param`, `packages/api/nros/src/node.rs`)
looks up the baked launch string and records nothing, on every road,
including `nros::main!`. So a contract row for that parameter always reads
`param-phantom`. The island's L4 scaffold waives the row in `system.toml`.

## Direction

A census hook in `NodeContext::param`, plus a census rule that a read claims
its contract row. The rule must skip the type comparison, because a read
only has a string.

Workaround: declare the parameter with `declare_parameter_*`, which the
census does see.
