---
id: 2175
title: "`nros materialize <image>` copies the generated entry to `src/<entry>/` without rebasing its relative `path =` dependencies, so the materialized entry does not build"
status: resolved
type: bug
area: [cli, build]
severity: medium
found: 2026-10-31
related: [2167]
---

## What

Measured on `examples/workspaces/realtime-rust` (2026-10-31): `nros build linux`
generates `build/posix-zenoh/linux_entry/`, and `nros materialize linux` copies
it to `src/linux_entry/`. Its `Cargo.toml` keeps the generated location's
relative paths: `../../../generated/nros-selection/linux_entry`,
`../../../src/ctrl_pkg`, `../../../../../../packages/api/nros`. The new location
is one directory shallower, so every one points one level too high, and the
next `nros build linux` fails:

```
error: failed to load manifest for dependency `ctrl_pkg`
  failed to read `<checkout>/examples/workspaces/src/ctrl_pkg/Cargo.toml`
```

Dropping one `../` from each path made it build. So the first build after
materializing, which is the command the materialize message tells the user to
run next, fails.

## Direction

Rebase every relative `path =` in the copied manifest from the generated
directory to the destination (resolve against the source dir, then
re-relativize against the destination), or emit the materialized manifest
fresh for its destination. Add a test that materializes and then resolves the
manifest (`cargo metadata`, at build stage), since the stamp tests in
`builder/materialize.rs` never read the copied `Cargo.toml`.

## Resolution (2026-11-01)

`nros materialize` now rewrites the copied manifest with
`builder::materialize::rebase_manifest_paths`: every relative `path =` in
`[dependencies]`, `[dev-dependencies]`, `[build-dependencies]` (top level and
under `[target.*]`) and `[patch.*]` is resolved against the generated
directory and re-expressed relative to `src/<entry>/`
(`builder::paths::rebase_relative`). `[lib]` / `[[bin]]` paths are relative
to the package and left alone; formatting is kept (`toml_edit`).
`lexical_normalize` moved from `orchestration::facade` into `builder::paths`
so this has one spelling.

The verb also stopped copying build state. The generated entry dir holds the
image's cargo `target/` and its `nros-cargo.toml` settings file, and both were
copied into the user's `src/<entry>/`. Neither is copied now.

Verified:
- On `examples/workspaces/realtime-rust`: `nros build linux`, `nros materialize
  linux`, `nros build linux` builds. The rebased paths are
  `../ctrl_pkg`, `../../generated/nros-selection/linux_entry`,
  `../../../../../packages/api/nros`, and no `target/` appears.
- `build_verb_pipeline::a_materialized_entry_keeps_every_path_dependency_resolvable`
  checks that each path dep resolves to a `Cargo.toml` and that no `target/`
  is copied. It fails when the rebased manifest is not written back.
