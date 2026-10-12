---
id: 1849
title: "The Zephyr staleness tests prove an edit is seen by EDITING the tracked file, under every parallel test that reads it"
status: resolved
type: bug
area: [testing, zephyr]
severity: medium
found: 2026-10-12
related: [0466, phase-363, phase-484]
---

## Measured

A phase-484 W5b run of the test suite in parallel saw
`a_shared_cmake_input_marks_the_image_stale` race with its neighbours.

`packages/testing/nros-tests/tests/zephyr_leaf_staleness.rs` had three tests
that proved "the probe sees an edit" by writing the edit into a TRACKED file in
the checkout, resolving the fixture, and writing the original bytes back:

- `a_shared_cmake_input_marks_the_image_stale` appended to
  `cmake/NanoRosCodegenCore.cmake`.
- `rust_entry_sees_a_prj_conf_edit` and `c_entry_sees_a_prj_conf_edit`
  appended to a bringup's `boards/native_sim_native_64/prj.conf`.

nextest runs each test in its own process, all at once. Every Zephyr image
records `NanoRosCodegenCore.cmake` as a configure input (ninja's
`RERUN_CMAKE` edge, phase-363 W5). So during that window every test resolving
any Zephyr fixture had its probe read the module as edited (a false STALE
verdict) or as empty (`fs::write` truncates before it writes). A test killed
between the write and the restore left the edit in the checkout, where the next
`git add -u` would commit it.

No other test under `packages/testing/nros-tests/tests/` writes into the
tracked tree; the sweep was `grep -rn 'fs::write(' packages/testing/nros-tests/tests/`
and reading each hit, and every other hit writes into a temp dir.

## Fix

The probe is the thing under test, so the probe is what sees the edit.
`nros_tests::fixtures::staleness::with_simulated_edit(path, bytes, f)` runs `f`
with `path` reading as `bytes` for every freshness probe on the current thread:
its mtime reads as now, and its size and content hash come from `bytes`. The
file on disk never changes, and nothing outside the test process observes the
edit. The tests still drive the REAL resolvers.

While an edit is simulated the probe writes no `.nros-srcbaseline`. A first
sight during the edit would otherwise record the pretend bytes, and the real
file would then read as edited.

The `prj.conf` tests keep the #147 direction (a new mtime over unchanged bytes
is not stale) by simulating the ORIGINAL bytes. They no longer need to restore
anything.

## Verified

The unit test `a_simulated_edit_is_seen_by_the_probe_and_touches_nothing`
covers: the edit is seen, same bytes under a new mtime are not stale, the disk
bytes are unchanged, no baseline is written, and the edit ends with its closure.
Removing the baseline guard makes it fail ("a first sight during a simulated
edit recorded the pretend bytes").

The three Zephyr tests compile. They were not run here, because they need
built Zephyr fixtures that this worktree does not have.
