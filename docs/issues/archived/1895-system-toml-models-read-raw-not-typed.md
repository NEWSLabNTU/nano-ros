---
id: 1895
title: "`system.toml` `[[model]]` and `default_launch` are parsed three ways, and the typed field has no reader"
status: resolved
resolved: 2026-10-15
type: tech-debt
area: [cli, orchestration]
severity: low
found: 2026-10-14
related: [0934, RFC-0103, phase-484]
---

Split out of [issue 0934](0934-config-redundancy-map.md) (R9) when
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

## Fix (2026-10-15)

The survey above undercounted. Six readers parsed this slice, four of them raw:

| reader | keys | how |
| --- | --- | --- |
| `model_location::launch_to_model_rel` | both | raw `toml::Table` |
| `model_location::default_launch` | `default_launch` | raw |
| `model_location::model_rel_to_inputs` | `[[model]]` | raw |
| `cmd/ws.rs` `system_toml_model_decls` + its own `ModelDecl` | `[[model]]` | raw |
| `cmd/ws.rs` model resolution | `default_launch` | raw |
| `fixtures-manifest.py` `_system_default_launch` | `default_launch` | Python |

Every raw one also failed OPEN: a `system.toml` that did not parse read as
"no declarations", a `[[model]]` row missing `out` was dropped, and an `args`
value that was not a string vanished, so a declared binding variant could stop
existing without a word.

- `nros_orchestration_ir::system_launch` — `SystemLaunch { default_launch,
  models }` and `ModelDecl` (`deny_unknown_fields`, `args` a string map), with
  `read(path)`: an absent file is an empty slice, an unparsable one an error
  naming it. It reads only its own keys; the rest is the CLI schema's to check.
- All five Rust readers use it. `model_rel_to_inputs` and `default_launch`
  return `Result` now; `model_gate`'s staleness check treats a parse error as a
  STALE reason rather than "nothing to compare". `ws.rs` reads the slice once
  per bringup and its private `ModelDecl` is gone.
- The CLI's `SystemToml.models` is now `Vec<system_launch::ModelDecl>`, and
  `SystemModelEntry` is deleted, so a `[[model]]` row has one schema in the
  tree. The field cannot simply be removed, as the direction suggested:
  `SystemToml` is `deny_unknown_fields`, so dropping it would refuse every
  bringup that declares a variant.
- **Not done: the Python reader stays.** `fixtures-manifest.py validate-*`
  runs on the fast line, and that lane builds no `nros` binary to ask. It reads
  one key; the Rust test `the_fixture_manifest_reads_the_same_key_and_default`
  holds that key and its `DEFAULT_LAUNCH_FILE` to the module.
- Measured: all 250 tracked `system.toml` files parse, and all 38 `[[model]]`
  rows in them meet the stricter row schema, so the tightening refuses nothing
  in the tree today.
