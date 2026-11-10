---
id: 2422
title: "`zephyr_prjconf_meets_backend_requirements` walks into build output and times out on a built fixture tree; 12 more test walkers do not use the shared `treewalk` predicate"
status: resolved
type: bug
severity: medium
area: [testing, ci]
related: [1603]
found: 2026-11-10
resolved: 2026-11-10
resolved_in: "every recursive walker in nros-tests names nros_tests::treewalk or a treewalk-exempt reason; check-test-tree-walkers holds it"
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

## Resolution (2026-11-10)

Each of the 12 files was read. Most recursive walks skip build output now, and
the ones whose subject IS build output say so with a `treewalk-exempt:` comment
inside the function:

| File | Verdict |
| --- | --- |
| `board_agnostic_run_plan.rs`, `nav2_compat.rs`, `freertos_firmware_entry.rs` | exempt: each searches a cargo `build` dir for the `run_plan.rs` a build wrote |
| `interface_subset_linkage.rs` (2 walks) | exempt: archives and `build.make`/`build.ninja` in a fixture's build dir |
| `rmw_coordinate_truth.rs` (`row_binary`) | exempt: depth-bounded search for the row's binary in its build output |
| `example_portability.rs` | its private `is_build_output` (`build*`/`target*` prefixes) now returns `treewalk::is_build_output_dir` |
| `legacy_files_forbidden.rs` | its private `skip_dir` listed exactly `is_skipped_dir`'s names; it calls it now |
| `no_local_axis_tables.rs` | skipped only `target`; uses `is_skipped_dir` |
| `ros_editions_codegen.rs`, `schema_serializer_round_trip.rs` | walk generated output on purpose, so they skip `is_build_output_dir` (not `generated/`) |
| `fixture_source_coverage.rs`, `pkg_index.rs` | one-level `read_dir`, not a walk; unchanged |

The gate found seven more walkers in `src/`, which the list above (from
`tests/` only) did not name:

- `fixtures/binaries/mod.rs`:
  - `newest_mtime_under` and `newest_path_after` are exempt: each walks an input cargo recorded, and a directory input means everything under it.
  - `find_build_script_outputs`, `linked_archive_copies` and `find_cargo_profile_dirs` are exempt: each is a bounded artifact search in a build dir.
  - `newest_source_after` walks zpico-sys `c/` sources and now skips `is_build_output_dir`.
- `run.rs` `descendants` is exempt: it walks the process tree under `/proc`.

Gate: `just check test-tree-walkers` (`scripts/check-test-tree-walkers.py`),
on the fast line. It flags any function in `packages/testing/nros-tests/{tests,src}`
that calls `read_dir` and recurses (a self-call or a `while let Some(..) =
x.pop()` worklist) without one of these:

- naming `treewalk::`, `is_skipped_dir` or `is_build_output_dir`;
- calling a same-file helper that names one of them;
- carrying `treewalk-exempt: <reason>`.

Its self-test runs on every invocation. The pre-fix `collect_overlays` fails it,
its fixed form passes, an unmarked worklist fails, and an exempt one, a
helper-mediated one and a one-level `read_dir` pass.
