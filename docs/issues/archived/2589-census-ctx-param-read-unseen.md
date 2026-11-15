---
id: 2589
title: "The entity census does not see a Rust node's `ctx.param()` read, so its contract row reads `param-phantom`"
status: resolved
type: bug
area: [api, cli, census]
severity: low
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that resolves it"
related: [phase-478, issue-2568]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md).**
Split from [issue 2568](2568-rust-ctx-param-misses-launch-values.md),
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

## Resolution (2026-11-15)

GitHub #3860's census half. The value half was issue 2568.

- **Runtime.** `NodeContext::param` calls `census_hooks::on_param_read`,
  which `metadata_mode::record_parameter_read` records as a parameter row
  marked `"read": true`, with no type and no default. `register()` usually
  reads before it creates its node, when the census has no node to attribute
  the read to (or still has an earlier component's node open), so the
  context holds those reads and records them on the node it creates.
- **Census.** `entity_census::check_params` lets a read claim the contract's
  `params:` row with no type comparison, so the row is `confirmed`, not
  `param-phantom`. A read of a name the contract does not declare raises
  nothing: a launch value for it is already refused at resolve time. A read
  row is never counted as a declaration (`read_census_params`, the census
  summary line), and parameter-store sizing reads the contract, not the
  sidecar, so a read changes no pool.
- **Sidecar.** `SourceParameter` accepts the additive `read` field; it is
  `deny_unknown_fields`, so without it every sidecar carrying a read would
  have failed to parse.
- Tests: `a_launch_parameter_read_before_create_node_lands_on_that_node`
  (fails with the deferral removed, landing the read on the earlier node),
  `a_parameter_read_is_recorded_as_a_read`,
  `a_parameter_the_code_reads_by_name_is_not_a_phantom`, and the read row in
  `schema_v2_fields_are_additive`.

The island can drop its `param-phantom` waiver once a release carries this.
