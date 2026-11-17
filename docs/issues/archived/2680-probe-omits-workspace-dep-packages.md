---
id: 2680
title: "The C/C++ metadata probe adds a node package without the workspace packages it <depend>s on, so a node including a sibling package's header cannot be probed"
status: resolved
type: bug
severity: medium
area: [cli, cmake, build]
related: [phase-568, phase-313, phase-348, phase-367, issue-0662, issue-2280, issue-1578, issue-2608]
found: 2026-11-17
resolved_in: "PR #4153 (phase-568 W3)"
---

**GitHub:** #4149. **Home:** [phase-568](../../roadmap/phase-568-build-args-and-profiles.md) W3,
the source probe's reach. **Design review:** nros 118.

## What it was

The probe is ONE generated CMake project per workspace
(`orchestration/metadata_probe_cmake.rs`, `render_probe_cmakelists`) and it
added each COMPONENT package alone. A node package including a header from a
sibling workspace LIBRARY package it names in `<depend>` could not configure
in the probe, while the real workspace build (ws-order, phase-348 W4) built it.
Found in the Autoware safety island: `controller_pkg` needed an
`if(NOT TARGET autoware_controller_libs) add_subdirectory(...)` fallback of its
own, or `sync` failed with
`autoware/trajectory_follower_base/control_horizon.hpp: No such file or directory`.

## Resolution

The probe adds each member's build-time workspace dependency packages to the
same project, before the member, each once.

- **Which:** the transitive closure over `<depend>`, `<build_depend>`,
  `<build_export_depend>` (`package_xml` / `provider_scan` gained a tag-aware
  build-dependency set); `<exec_depend>` and `<test_depend>` are not followed.
  Matched on `<name>`, never the directory
  (`add_subdirectory(<dir> <package name>)`).
- **Not added, but walked through:** names outside the workspace, interface
  packages (the Find-stubs serve them), packages without a `CMakeLists.txt`,
  and members (added once, as members).
- **Order:** `topological_order_with_priority`, ties in member order, so a
  workspace with no workspace library deps renders a byte-identical
  CMakeLists.
- **Refused before configure, naming the package:** a cycle among the added
  packages; a dependency that is a west application (issue 2280's rule).
- **Blame:** a `CMake Error at <dep>/CMakeLists.txt` blames every member whose
  closure holds `<dep>`, as `workspace dependency `<dep>``, and the refuse
  message still closes with the probe-args note.
- **Freshness:** the probe inputs key (issue 1578) gains
  `name=source_digest(dir)` over the closure, only when it is non-empty, so
  dependency-free members' sidecars stay fresh.
- **Consumer fallbacks** see the target already defined and do nothing; they
  can be deleted.

New module `orchestration/probe_deps.rs`; result rows are matched back to
components by name.

## Verification

- Unit: `package_xml` 24, `probe_deps` 6 (order by name, filtered
  walk-through, cycle, exec back-edge, west dependency, key field),
  `workspace_dep_render_tests` 4 (dep before member, member that is also a
  dep added once, shared dep once, no-dep render unchanged),
  `metadata_probe_cmake` 26, `metadata_refresh::tests` 29 (incl. a
  dependency-blamed failure ending with `Note: <PROBE_ARGS_RULE>.` under
  refuse).
- End to end: fixture `packages/testing/nros-tests/fixtures/probe_workspace_dep_ws`
  (`dep_probe_pkg` includes `period_lib/period.h`, no fallback) configures
  under `NROS_PROBE_FAILURE=refuse` via `scripts/check-workspace-probes.sh`;
  the generated project adds `period_lib` before `dep_probe_pkg`.
