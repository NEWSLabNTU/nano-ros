---
id: 197
title: "pure-C workspace `nros plan` fails at cmake-configure — missing source metadata for c_talker_pkg/c_listener_pkg"
status: open
type: bug
area: cli
related: [phase-287, issue-0183, rfc-0048]
---

## Summary

Configuring `examples/workspaces/c` (a declared native fixture — many rows in
`examples/fixtures.toml`) aborts at cmake-configure time:

```
CMake Error at cmake/nano_ros_workspace_metadata.cmake:72 (message):
  stderr: Error: planning failed with 2 error(s): missing-source-metadata:
  missing source metadata for c_talker_pkg/talker [package=c_talker_pkg
  instance=c_talker_pkg.talker.0]
  (…/build-workspace-fixtures/nros-plan/record.json);
  missing-source-metadata: missing source metadata for
  c_listener_pkg/listener [package=c_listener_pkg
  instance=c_listener_pkg.listener.0]
-- Configuring incomplete, errors occurred!
ninja: error: rebuilding 'build.ninja': subcommand failed
error: recipe `build-workspace-fixtures` failed on line 155 with exit code 1
```

So `just native build-workspace-fixtures` returns non-zero for the pure-C
workspace. Surfaced while rebuilding fixtures for #183 (the ws-bridge-rust
workspace built fine — this is a separate, C-workspace-only failure).

## Root cause — metadata plumbing gap for C nodes at plan time

1. `examples/workspaces/c/CMakeLists.txt` calls `nano_ros_workspace_metadata()`,
   which shells `nros plan` at **configure** time
   (`cmake/nano_ros_workspace_metadata.cmake:60`):

   ```
   "${NROS_BIN}" plan --workspace <root> --out-dir <plan_dir> <system> <launch_rel>
   ```

   Note: **no `--metadata` flag** is passed.

2. `nros plan` errors when `find_source_metadata(package, executable)` returns
   `None` for a non-container node
   (`packages/cli/nros-cli-core/src/orchestration/planner.rs:2265`).

3. The plan's source-metadata inputs are: explicit `--metadata` files +
   workspace discovery, which scans each package's source root for
   `<pkg>/{metadata,nros,target/nros}/*.json`
   (`packages/cli/nros-cli-core/src/orchestration/workspace.rs:422`). The two C
   packages carry **no** such JSON under their source roots (only
   `package.xml` / `CMakeLists.txt` / `src/*.c` / `generated/`).

4. C-node source metadata *is* emitted — `nano_ros_add_node`
   (`cmake/NanoRosVerbs.cmake:157`) internally calls `nano_ros_node_register`
   (`cmake/NanoRosNodeRegister.cmake:137`), whose `_nros_metadata_emit` writes
   `${CMAKE_BINARY_DIR}/nros-metadata.json`
   (`cmake/NanoRosNodeRegister.cmake:119`) — but into the **build dir**, which
   the planner's source-root scan never reads and which the configure-time
   `nros plan` call never forwards via `--metadata`.

Net: the C node metadata lands in the build tree, but the workspace `nros plan`
invocation neither receives it (`--metadata`) nor discovers it (source-root
scan). Rust workspaces sidestep this entirely — their per-node metadata is
synthesised from cargo `[package.metadata.nros.{component,node}]` and is
available at plan time (`planner.rs:82` comment), so ws-bridge-rust plans
cleanly.

## Regression window

287-W6 ament-shape migration of the C workspace members:
`9c20918fc feat(287-W6): migrate C workspace node members to the ament shape`
and `ce522588a feat(287-W6): migrate workspace roots + Entry pkgs to the
RFC-0048 ament shape`. The `c_talker_pkg` / `c_listener_pkg` file sets are
unchanged across the migration (metadata was never checked in), so the break is
in how the migrated ament shape plumbs configure-time C-node metadata into the
plan — not a lost source file.

## Fix directions (not yet landed)

Any one of:

- Have `nano_ros_workspace_metadata()` pass the build-dir
  `nros-metadata.json` (and any per-package emitted metadata) to `nros plan`
  via `--metadata`, ensuring the emit runs before the plan in configure order.
- Emit C-node source metadata to a source-root location the workspace
  discovery already scans (`<pkg>/metadata/<node>.json`) at configure/sync
  time, mirroring the rust cargo-metadata path.
- Make the planner tolerate a container-less node whose metadata arrives via
  the cmake-summary channel (`nano_ros_node_register` → `nros_components.cmake`
  node summaries, `workspace.rs:304`) rather than a loaded `*.json` artifact.

Reproduce:

```
source ./activate.sh
cmake --regenerate-during-build -S examples/workspaces/c \
  -B examples/workspaces/c/build-workspace-fixtures
```
