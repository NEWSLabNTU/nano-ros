---
id: 1882
title: "`just check build` stopped running `native check` on 2026-09-03: the call sits in the `build-serial` registry recipe, which nothing invokes since the parallel runner, so native-example clippy has had no lane for six weeks"
status: resolved
type: bug
severity: medium
area: [build, testing]
related: [0993, 1881, 0898, phase-489]
found: 2026-10-14
resolved: 2026-10-14
resolved_in: "PR #2154"
---

## Measured

- `just/check.just`: `build:` is
  `@NROS_GATE_LANE=build-serial bash scripts/build/run-gates-parallel.sh`.
  `build-serial:` is the dependency-list registry, and its body ends in
  `@just native check` and `@echo "Build checks passed!"`.
- `run-gates-parallel.sh` runs `just check <gate>` for each name that
  `check-gate-lists.py --list build-serial` prints. It never runs the
  `build-serial` recipe, so it never reaches that body. No workflow, recipe or
  script runs `just check build-serial` either (`grep -rn build-serial just/
  justfile .github/ scripts/`).
- The nightly `check / build tier + no-std` job of gate.yml run 38336075498
  (2026-10-13, green) prints `check-build (parallel): 14 gate(s) ran … 11
  SKIPPED`. It does not print `native check:` or `Build checks passed!`.
- The switch landed in 537c938298 (#0993, 2026-09-03).

## Effect

Nothing on any event runs the native-example clippy fan-out (`native::check`).
The check-build job still runs `just generate-bindings` (issue 0898), and its
step name, "native::check asserts them", is now stale. A single
`check-lane-contracts` reach fix (issue 1881) is enough to make the restored
call checkable.

## Fix direction (not done)

Call `just native check` from `check::build` after the parallel runner, or make
it a gate on the `build-serial` list. Then measure its cost and its current
state, because six weeks unrun means it may already be red. Either way the
lane needs `generate-bindings`, which the check-build job already runs.

## Resolution

- `native-check` is a gate recipe (`just/check/lanes.just`) on the `build-serial`
  list, so `check::build` runs it. The `@just native check` line under the list
  is gone; `check-gate-lists` now refuses ANY non-comment body under the
  registry (it never runs), with a self-test case both ways.
- Run for the first time in six weeks, `native check` was RED on 27 of 49
  leaves (every `mps2-an385-baremetal` and `esp32-c3-baremetal` leaf): their
  manifests need the `<leaf>_nros_selection` crate and patch rows that
  `nros sync` renders (RFC-0098 D1), and cargo needs
  `--config build/<image>/nros-cargo.toml`. The lint now lives in
  `scripts/build/lint-example-leaf.sh`, called from both fan-out branches, and
  renders first.
- Measured in the ros2 distrobox, warm sccache, 32 cores: `just check
  native-check` 20m53s, green (two clippy warnings, no errors). Expect far more
  on a -P4 runner (issue 0993's lesson): this lane is `schedule` only.
