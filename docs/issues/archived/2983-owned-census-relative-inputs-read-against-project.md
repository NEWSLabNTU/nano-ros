---
id: 2983
title: "A taken owned-entry census reads stale to `nros configure` the moment it is taken, so every owned image's host configure runs unseeded"
status: resolved
type: bug
severity: high
area: [cli, census, configure]
found: 2026-11-26
resolved: 2026-11-26
resolved_in: "fix/owned-census-absolute-inputs"
related: [2900, 2980, phase-530]
---

## What happened

The autoware safety island's Zephyr FVP lane went red on release candidate
ebbb3a570d (run 45825154165, a fresh container). It was green on
1a638e9958. Within one `nros build`, owned-entry censuses taken minutes
earlier were judged stale:

    entity census .../census/owned/unit_test_bringup/fvp-aemv8r-smp.census.json is stale:
    source_tree `` changed since the census was taken
    (recorded fnv1a64:3d1ec9029b886439, now fnv1a64:77b0ec91932c67c5)

`dds_loopback_test_bringup`'s census failed the same way, with a different
recorded digest and the same "now" digest. Then
`controller_bringup:fvp-tap`'s configure stopped unseeded at
`cmake/NanoRosPromise.cmake:402`.

## Why

It was a regression in the candidate, introduced by issue 2900's fix
(94a28565c5, PR #4825), which made an owned entry's census stale when its
build files change.

The owned road takes a census with `workspace = <the entry package>`
(`census_host_program` → `run_census`). The fix for 2900 recorded the
program's inputs through `collect_inputs`, which writes paths relative to
that workspace. The entry package's own `source_tree` row therefore had the
path `""`, and its `build_file` row `CMakeLists.txt`.

- The take's own check resolves those rows against the package, so the
  census read fresh.
- `configure_stamp::currency` resolves every census against the project
  root (`census_freshness(census, project)`). There `""` is the whole
  workspace: a different digest, the same for every bringup, and one that
  moves as the build writes. So every taken owned census read stale to the
  configure stamp, the image's host configure got no seed, and the promise
  check stopped it.

Issue 2980's fix (scoping the census reads to the image's own bringup)
limits the spread, but each of these censuses was its own image's, so the
fix for 2980 alone does not help.

## Fix

- The owned road records its program inputs as absolute paths, as the
  stated-empty owned census already did, for the same reason: its readers
  resolve against different roots.
- `records_entry_build_file` requires that absolute row, so a census taken
  by the candidate is re-taken once rather than trusted.

## The gap that let it ship

The test for 2900 read the census back only against the package, which is
what its taker does, and never against the project root, which is what the
configure stamp does. Two tests in
`packages/cli/nros-cli-core/src/cmd/entity_census.rs` close that gap:

- `every_census_road_reads_fresh_against_the_project_root` produces a
  census on every road (owned taken, owned stated, model, leaf) the way its
  producer writes it, and reads it back the way the stamp does.
- `an_owned_census_goes_stale_when_the_entrys_build_files_change` asserts
  that the package and the project root agree, that every row is absolute,
  and that a census in the shape this issue found is not believed.

With the fix reverted, both fail.
