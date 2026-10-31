---
id: 2175
title: "`nros materialize <image>` copies the generated entry to `src/<entry>/` without rebasing its relative `path =` dependencies, so the materialized entry does not build"
status: open
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
