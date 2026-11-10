---
id: 2422
title: "`zephyr_prjconf_meets_backend_requirements` walks into build output and times out on a built fixture tree; 12 more test walkers do not use the shared `treewalk` predicate"
status: open
type: bug
severity: medium
area: [testing, ci]
related: [1603]
found: 2026-11-10
---

## Measured

Tier-2 dispatch run 42777204809 on `main` `2865cc77c5`, job 129698400878
"tier 2 (1-wise matrix)", runner `nano-ros-runner-newslab-server-241`. The
build stage passed. The matrix preflight `build::check::source-gates` failed:

```
TIMEOUT [  60.151s] (5/5) nros-tests::zephyr_prjconf_requirements zephyr_prjconf_meets_backend_requirements
tier 2 (1-wise matrix): NO VERDICT: preflight failed before any cell
```

The same test took 0.871 s in the tier-1 cells job of run 42767563994. It is a
host-only string parse with no west and no SDK.

## Cause

`collect_overlays` in `tests/zephyr_prjconf_requirements.rs` recurses into
every directory under `examples/zephyr` and the workspace bringups, including
`build/`, `build-*`, `target*` and `generated`. On a tree where tier 2 has just
built every Zephyr image, that is the west build trees and cargo target dirs.
The walk is unbounded in the size of the build output, and on a loaded host
(load average about 25 on 241 at the time) it outlived nextest's 60 s. It
could also collect `prj-*.conf` copies that a build placed in its output tree.

`nros_tests::treewalk` already exists for exactly this. Its header records
five walkers that each had their own spelling of "is this build output", and
one of them (`examples_canonical_shape`) blew past the 60 s timeout the same
way. This walker was never moved onto it.

## Fixed here

`collect_overlays` skips any directory for which
`nros_tests::treewalk::is_skipped_dir(name)` is true.

## Still open: the sweep

These test files call `read_dir`, test `is_dir()` or `file_type()`, and never
name `treewalk`, `is_skipped_dir` or `is_build_output_dir`. Each needs reading,
because some may walk build output on purpose (looking for artifacts) or may
not recurse at all:

- `board_agnostic_run_plan.rs`
- `example_portability.rs`
- `fixture_source_coverage.rs`
- `freertos_firmware_entry.rs`
- `interface_subset_linkage.rs`
- `legacy_files_forbidden.rs`
- `nav2_compat.rs`
- `no_local_axis_tables.rs`
- `pkg_index.rs`
- `rmw_coordinate_truth.rs`
- `ros_editions_codegen.rs`
- `schema_serializer_round_trip.rs`

## What would close it

Each of those either uses the predicate or carries a comment saying why it
walks build output. Then add a gate, a source scan over `tests/*.rs` and
`src/**`, that refuses a recursive directory walk that names none of the
predicates and has no such exemption comment. Without the gate, a sixth
private spelling reappears, as this one did. The gate needs a negative
control: the pre-fix `collect_overlays` must fail it.
