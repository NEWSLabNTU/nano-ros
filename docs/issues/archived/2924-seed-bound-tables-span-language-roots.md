---
id: 2924
title: "A west image's message-bound seed reads one language root of the probe's bound tables, so a closure split across `nano_ros_c` and `nano_ros_cpp` refuses on a fresh tree"
status: resolved
type: bug
area: [cli, build, census]
severity: high
found: 2026-11-24
resolved: 2026-11-24
resolved_in: "fix(cli, issue 2924): the west seed finds each bound table under either language root"
related: [phase-478, phase-590, 2911, 2644, 2876]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)**
(the owned west entry fixtures are island-shaped). Blocked the nros12 tier-1
lane ([phase-590](../../roadmap/phase-590-nros12-release-plan.md)).

## What happens

On a fresh worktree, `just build-test-fixtures lane=tier1` stops in the census
pre-pass, before any stage, on the compile-check row
`west_owned_entry_workspace_module` (image `demo_bringup:zephyr`):

```
`demo_bringup:zephyr`: message bounds not seeded, because what it declares
could not be derived: the metadata probe generated no bound table for
`builtin_interfaces` (<fixture>/build/nros-metadata/metadata-probe-cmake/build/
nros-codegen/nano_ros_cpp/builtin_interfaces/nros_message_bounds.json)
(NROS_PROBE_FAILURE=refuse)
```

Reproduced on a fresh worktree with
`NROS_PROBE_FAILURE=refuse bash scripts/build/census-prepass.sh --only images --platform zephyr --id west_owned_entry_workspace_module`
(rc 1). The pre-pass lists this row since PR #4723 (issue 2911), which made it
the first step to configure the image under `refuse`.

## Root cause

The seed for a C/C++ west image (`write_west_seed`) composes over the
interface closure of the image's packages, and reads each package's bound
table from the metadata probe's codegen dir. `probe_bound_tables` chose ONE
language root for the whole closure: `nano_ros_<lang>` if it existed, else the
other one (issue 2644).

The probe emits each interface package in the language of the code that asked
for it. In this fixture the C node `talker_pkg` pulls `std_msgs` and
`builtin_interfaces` into `nano_ros_c`, and the probe's
`nros_workspace_interfaces()` emits the workspace's own `demo_msgs` into
`nano_ros_cpp`. The closure (`builtin_interfaces`, `demo_msgs`, `std_msgs`)
spans both roots. `nano_ros_cpp` exists, so it was chosen, and the first
package not under it refused.

A bound prices a TYPE, not a language, so any root's table is the same bound.

## Fix

`probe_bound_tables` returns both roots, the preferred language's first, and
`write_west_seed_over` looks each package up across them, taking the first
root that holds its table. A package found under neither still refuses, and
the error names every path tried. The leaf and Rust west roads pass their one
root as a one-element list and are unchanged.

Tests: `message_bound_knobs::tests::a_closure_split_across_table_roots_is_seeded`
(a closure with one package per root: one root alone refuses, both seed);
`probe_bound_tables_tests` updated for the list.

## Why the local gate passed

`just ci gate` builds no fixtures; the census pre-pass runs only in the tier-1
and tier-2 fixture lanes on the self-hosted runner. No unit test held a
closure whose tables span both language roots; the new one does.
