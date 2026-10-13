---
id: 1882
title: "`just check build` stopped running `native check` on 2026-09-03: the call sits in the `build-serial` registry recipe, which nothing invokes since the parallel runner, so native-example clippy has had no lane for six weeks"
status: open
type: bug
severity: medium
area: [build, testing]
related: [0993, 1881, 0898, phase-489]
found: 2026-10-14
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
