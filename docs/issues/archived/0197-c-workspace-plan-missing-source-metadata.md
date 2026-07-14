---
id: 197
title: "pure-C workspace `nros plan` fails at cmake-configure — missing source metadata for c_talker_pkg/c_listener_pkg"
status: resolved
type: bug
area: cli
related: [phase-287, issue-0183, rfc-0048]
---

## Summary

Configuring `examples/workspaces/c` (a declared native fixture — many rows in
`examples/fixtures.toml`) aborted at cmake-configure time:

```
CMake Error at cmake/nano_ros_workspace_metadata.cmake:72 (message):
  stderr: Error: planning failed with 2 error(s): missing-source-metadata:
  missing source metadata for c_talker_pkg/talker [package=c_talker_pkg
  instance=c_talker_pkg.talker.0];
  missing-source-metadata: missing source metadata for
  c_listener_pkg/listener [package=c_listener_pkg
  instance=c_listener_pkg.listener.0]
-- Configuring incomplete, errors occurred!
error: recipe `build-workspace-fixtures` failed on line 155 with exit code 1
```

So `just native build-workspace-fixtures` returned non-zero for the pure-C
workspace. Surfaced while rebuilding fixtures for #183 (ws-bridge-rust built
fine — this was C-workspace-only).

## Root cause — a STALE in-tree `nros` CLI (not a source bug)

`examples/workspaces/c/CMakeLists.txt` shells `nros plan` at **configure** time
(`cmake/nano_ros_workspace_metadata.cmake:60`). `nros plan` errors when
`find_source_metadata(package, executable)` returns `None` for a non-container
node (`planner.rs:2265`).

The C node source metadata is synthesised at plan time by statically parsing
each package's `CMakeLists.txt`: `discover_cmake_node_metadata`
(`workspace.rs:450`) scans for the component verbs and
`synthetic_metadata_artifacts` (`workspace.rs:223`) turns each summary into a
JSON artifact the planner consumes (`planner.rs:88`).

The 287-W6 ament migration (`9c20918fc`, 2026-07-13) changed the C node
`CMakeLists.txt` from the all-keyword `nano_ros_node_register(...)` to the
ament verb `nano_ros_add_node(<name> CLASS … TYPED <src>)` **and, in the same
commit, added the matching CLI parser** `parse_add_node_call`
(`workspace.rs:483–488`). Source and workspace were migrated together and are
consistent (the unit test `discover_finds_add_node_from_cmakelists` passes).

The break was purely a **stale deployed binary**: the in-tree
`packages/cli/target/release/nros` was built **2026-07-11 15:58** — two days
*before* 9c20918fc landed — so it carried the old `nano_ros_node_register`
scanner but not `parse_add_node_call`. Confirmed: `strings nros | grep -c
nano_ros_add_node` → `0` on the stale binary. It parsed zero components from
the migrated CMakeLists → no synthetic metadata → `missing-source-metadata`.
Rust workspaces were unaffected because their metadata comes from cargo
`[package.metadata.nros]`, unchanged by the migration.

The fixture-build recipes never rebuild `nros` first — `build-workspace-fixtures`
/ `build-fixture-extras` / `build-fixture-rust` do not depend on `setup-cli`
(only `scaffold-journey` / `acceptance` do), and `just` module-scoping makes a
cross-module `: setup-cli` dependency awkward. So a checkout whose CLI predates
the latest cmake-verb migration plans against a binary that can't parse it, and
the failure lands two layers down as an opaque cmake `missing-source-metadata`.

## Resolution

1. **Immediate:** rebuilt the CLI (`just setup-cli` — the rebuilt binary carries
   `parse_add_node_call`). `nros plan --workspace examples/workspaces/c …` →
   `rc=0`; `cmake --regenerate-during-build -S examples/workspaces/c -B …` →
   "Configuring done / Generating done". Fixed.
2. **Durable (prevent recurrence):** added a fail-loud CLI-staleness guard to
   `nros_require_ws_sync` (`scripts/build/cargo.sh`) — the shared preflight that
   every fixture build already calls. When the resolved CLI is the per-checkout
   binary and any `packages/cli` source is newer than it (same prune+`-quit`
   scan `setup-cli` uses), it aborts with a `just setup-cli` hint instead of
   letting a stale CLI die downstream at cmake-configure. The `ws sync` probe
   already there does NOT catch this (a stale binary still has `ws sync`), so
   the staleness check is separate. Same fail-loud philosophy as the #181
   ws-sync guard.

## Notes

The mtime treadmill (a rebase refreshes cli source mtimes) will now trip this
guard too — which is correct: after a rebase the CLI must be rebuilt anyway
(`just setup-cli` is a fast cargo-noop when content is unchanged), and the guard
just makes the owed rebuild explicit rather than silently planning on a
mtime-stale (and possibly content-stale) binary.
