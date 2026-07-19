---
id: 237
title: "ws-safety-{cpp,c} plain workspace cmake doesn't set the safety-e2e build flag"
status: open
type: tech-debt
area: cmake
related: [phase-296]
---

## Summary

Building `examples/workspaces/ws-safety-cpp` (and `-c`) with a plain
`cmake -S <ws> -B <ws>/build-workspace-fixtures` configure fails to compile the
node package: `SafetyListener.cpp` calls
`nros::Node::create_subscription_with_safety(...)`, which is gated behind the
safety-e2e build flag that the plain configure does not set.

The real fixture builder DOES set it — the `[[workspace_fixture]]` rows for
these workspaces use a distinct `build_subdir = build-workspace-fixtures-safety-{talker,listener}`
with the safety define — so CI builds them fine. The gap is only in the
ad-hoc single-`cmake` build used during the phase-296 R4 migration.

## Impact

The phase-296 R4 migration of `ws-safety-{cpp,c}` to
`nano_ros_add_executable(... MODEL …)` is deferred: the MODEL swap itself is
mechanical (the model resolves; `ws-safety-rust` migrated + validated the same
shape), but it can't be **build-validated locally** without the safety flag —
so it stays on `LAUNCH` until the migration wires the flag.

## Fix options

1. **Migrate + rely on the fixture builder** (`just build-workspace-fixtures`
   with the `-safety-*` build_subdir + define) for validation, rather than a
   plain `cmake` build. Lowest effort; the MODEL swap is orthogonal to the
   safety flag.
2. **Have the workspace's own CMake enable the safety path by default** (or via
   a cache var the fixture rows already pass), so a plain configure builds it.
   Cleaner for users but touches the example's build.

## Workaround

`ws-safety-{cpp,c}` remain on `LAUNCH`; the safety demo is otherwise covered by
`ws-safety-rust` (migrated + native `case_14` validated).
