---
id: 2435
title: "A fresh metadata sidecar with no probe build is not probed again, so a clean build dir gets no message-bound seed"
status: resolved
type: bug
severity: high
area: [cli, cmake, build, zephyr]
found: 2026-11-11
related: [phase-530, issue-2405, issue-2361]
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)**. Found while building issue 2405's acceptance road, a hand-written C west entry configured from a clean build dir.

## Measured

The metadata probe has two outputs: the sidecar under the package's source
tree (`src/<pkg>/metadata/*.json`) and the probe project's build
(`build/nros-metadata/metadata-probe-cmake/build/`), whose
`nros-codegen/nano_ros_{c,cpp}` bound tables the message-bound seed reads.
Freshness asked about the sidecar alone. The sidecar lives in the source
tree, so it survives `rm -rf build`; a later `nros configure` reported the
probe "already current", found no bound tables, and wrote no
`resolved-message-bounds.cmake`. That was true on every road (generated
entries too), not only the hand-written one.

A second defect on the same road: `nano_ros_add_executable(... BRINGUP
"../demo_bringup")` in an entry's `CMakeLists.txt` made the configure compose
the entity inventory without the workspace (`refused`) while the seed said
`derived`, because `cmake/NanoRosEntry.cmake` took `DIRECTORY`/`NAME` of the
spelling as written.

## Fix

- `metadata_refresh.rs`: a C/C++ component counts as current only when its
  sidecar is fresh AND the probe build is present (`probe_build_present`,
  the probe project's `CMakeCache.txt`). `stale_sidecar_components` reports
  the missing build as a reason. Test:
  `a_fresh_sidecar_without_the_probe_build_is_probed_again`.
- `NanoRosEntry.cmake`: both bringup readers
  (`_nros_entry_require_fresh_census` and `nano_ros_entry`'s workspace
  inference) make the path absolute before splitting it.

Coverage: `tests/west_hand_written_entry.rs` over the
`west_hand_written_c_entry` fixture, whose entry spells its bringup relative
to itself and is configured from a clean build dir with its sidecars left in
the source tree.

Sweep: `git grep -n 'sidecar_is_fresh(' packages/cli` finds four callers.
The two that decide whether a C/C++ probe re-runs (`cpp_probe_options`) or
report why it must (`stale_sidecar_components`) now consult the probe build.
The Rust arm of `refresh_stale_sidecars` has no probe project to lose, and
the counts reader reads only the sidecar, so neither needs it. For the cmake
half: `git grep -n 'get_filename_component(.*DIRECTORY)' cmake/NanoRosEntry.cmake`;
both bringup splits now take an `ABSOLUTE` path.
