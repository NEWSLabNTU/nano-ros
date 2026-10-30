---
id: 2112
title: "Tier 2 runs the FreeRTOS lending cell but never builds its `loan-copy-sub` image, so the cell fails MISSING"
status: open
type: bug
area: [ci, testing, fixtures, freertos]
severity: medium
found: 2026-10-27
related: [1941, phase-510, phase-466]
---

## Measured

`run-matrix` run 40461297569 (schedule, 2026-10-27 06:28 UTC, main
`99d5e1df0`), job 122046305909 `tier 2 (1-wise matrix)`. The rewrite counted
`Real failures: 2 / 2 total failures`; one of them is:

```
nros-tests::lending_freertos_e2e freertos_loan_publish_delivers_and_saves_the_payload_copy
panicked at packages/testing/nros-tests/src/fixtures/binaries/mod.rs:770:9:
Test fixture binary MISSING for an in-lane coordinate:
  .../nros-fixture-tree/build/cargo-fixtures/freertos/thumbv7m-none-eabi/nros-minsizerel/loan-copy-sub
A gated run already asserted this lane's fixtures are built and fresh,
so this is a broken promise, not an environment skip.
```

So the resolver puts the cell IN the tier-2 lane, while the lane's fixture
build produced no `loan-copy-sub`. In the build log the leaf
(`packages/testing/nros-bench/loan-copy-count`, row in `examples/fixtures.toml`)
appears only as `generate-rust:` lines; no line builds its `[[bin]]`s.

## Why this is visible now

This is the first tier-2 run in days whose fixture build finished. The runs on
2026-10-24, -25 and -26 stopped earlier, at issue 2025's Zephyr link, so this
cell's verdict was hidden behind that.

## What it is not

- Not a stale fixture: the message is MISSING, not STALE.
- Not issue 1941's drift gate: this run's `check-lock` reported
  `52 lock(s), 0 drift(s)`. The same leaf, `nros-bench/loan-copy-count`, IS in
  1941's fourth sighting (the tier-2 *nightly* on newslab-118), so the two may
  share a cause there, but not here.

## What would close it

Find why the tier-2 lane's build omits the `loan-copy-count` row while the
run-side lane keeps its cell (the 0828 build/run lane split:
`nros_lane_build_lane` vs `CiLane::run_scope`, and
`lane_build_covers_run::every_unskippable_row_is_in_its_lane_build`), make the
two agree, and show the cell green in a tier-2 run.

## Sighting, 2026-10-28

- Run: run-matrix 40643270547 (main at `e0f81b4ac`, after #2764 merged).

- Job: 122666117078, `tier 2 (1-wise matrix)`. The same MISSING panic as before, at `fixtures/binaries/mod.rs:770`, for `…/freertos/thumbv7m-none-eabi/nros-minsizerel/loan-copy-sub`. Merging #2764 did not close it.
