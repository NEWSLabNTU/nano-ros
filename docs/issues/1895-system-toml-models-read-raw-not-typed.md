---
id: 1895
title: "`system.toml` `[[model]]` and `default_launch` are parsed three ways, and the typed field has no reader"
status: open
type: tech-debt
area: [cli, orchestration]
severity: low
found: 2026-10-14
related: [0934, RFC-0103, phase-484]
---

Split out of [issue 0934](archived/0934-config-redundancy-map.md) (R9) when
that survey was closed.

## What is true on main (2026-10-14)

- `SystemToml::models` (`[[model]]`, `orchestration/cargo_metadata_schema.rs`
  ~:662) is parsed by serde and has **no production reader**: `git grep -n '\.models\b'`
  under `packages/cli/nros-cli-core/src` finds none.
- The reader that matters, `nros_orchestration_ir::model_location`
  (`model_location.rs` ~:150-170), re-parses `system.toml` as a raw
  `toml::Table` for both `[system] default_launch` and `[[model]]`. It has to,
  because the typed `SystemToml` lives in `nros-cli-core`, which sits ABOVE
  `nros-orchestration-ir` in the crate graph.
- `scripts/build/fixtures-manifest.py` (`_system_default_launch`, ~:1369)
  reads `default_launch` a third way.

Three parsers for one table drift independently: a schema change (a renamed
key, a new default) lands in the typed struct and the two raw readers keep
the old reading.

## Direction

Move the `[system]` / `[[model]]` slice of the schema into
`nros-orchestration-ir` (or a crate both depend on), have `model_location` read
it typed, and delete the unused field from the CLI struct. The Python reader
should ask `nros` rather than parse.
