---
id: 2244
title: "The native C++ workspace Entry exits within 300 ms instead of entering its spin loop (tier 1 regression)"
status: wontfix
type: bug
area: [cpp, workspace, testing]
severity: medium
found: 2026-11-04
related: [phase-466]
---

> **Closed 2026-11-05 as a duplicate of [#2250](2250-cpp-entry-test-passed-on-retry-loop.md)**, fixed by #3091. Read that issue for the cause and the fix; this file is kept as the record of how the symptom was first reported.

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-11-04: the one real failure in the scheduled tier-1 run of 2026-11-04.

## Measured

The scheduled run-matrix on main, run 41784686434 (2026-11-04, head
`8c49a148bc`), job 126442748140 (`tier 1 (cells)`), had one real failure
among 269 (`Real failures: 1 / 1 total failures`; the other 268 are lane
skips the junit rewrite reclassified):

    nros-tests::workspace_metadata cmake_cpp_workspace_entry_starts_prebuilt_runtime
    panicked at packages/testing/nros-tests/tests/workspace_metadata.rs:180:5:
    C++ workspace Entry should enter its native spin loop when started from the prebuilt binary

The test spawns `build_native_workspace_cpp_entry()`'s binary, sleeps
300 ms, and asserts `proc.is_running()`. The process had already exited.
The test does not keep the child's output, so the junit carries no reason.

The same test PASSED in the tier-1 junit of run-matrix run 41205983383
(2026-10-31, head `37d6466751`). Runs 41280184109, 41399708559 and
41589064854 (2026-11-01 to 11-03) uploaded no tier-1 junit: their cells
never ran. So the change landed somewhere in
`37d6466751..8c49a148bc`.

## What it is not

- **Not a lane skip.** The junit rewrite classified it as a real failure,
  not as one of the 268 `lane` skips.
- **Not a missing fixture.** `.require()` passed; the binary existed and
  was spawned.
- **Not stale-fixture noise in the ordinary sense.** Tier 1 rebuilds its
  fixtures in the same job. A run against a fresh build of `8c49a148bc` is
  still the confirmation to make.

## What would close it

1. Run the fixture binary by hand and read why it exits. The assert should
   also print the child's output, so the next red names its own cause.
2. Bisect `37d6466751..8c49a148bc` over the files the C++ workspace Entry
   compiles (`packages/api/nros-cpp`, `cmake/`, the Entry codegen), if the
   output alone does not say.

Acceptance: `cmake_cpp_workspace_entry_starts_prebuilt_runtime` passes in a
scheduled tier 1.
