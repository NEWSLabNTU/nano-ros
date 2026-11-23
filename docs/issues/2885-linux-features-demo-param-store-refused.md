---
id: 2885
title: "Tier 1 and tier 2 cannot build their linux fixtures: `features/demo_bringup`'s configure refuses to size its parameter store"
status: open
type: bug
area: [fixtures, params, ci]
severity: high
found: 2026-11-23
related: [phase-590]
---

## What fails

`run-matrix.yml` has had no green run since `nros-0.6.0-nros11`. In run 44993007292
(2026-11-22, main `98b5d8e0c7`), `tier 1 (cells)` and `tier 2 nightly (pairwise cover)` both stop
at `just build tier1` / `just build tier2-nightly` with `FAILED stage(s): linux(rc=2)`. The first
error line in the linux stage log is:

```text
CMake Error at cmake/NanoRosEntityFacts.cmake:529 (message):
  Error: the parameter store cannot be sized: the entity inventory REFUSED
Error: configure failed: `... cmake -S build/posix-zenoh-linux-demo_bringup -B build/posix-zenoh-linux-demo_bringup/cmake
  -DNROS_WS_PREAMBLE=.../examples/workspaces/features/src/demo_bringup/cmake/preamble.cmake ...`
```

`tier 2 (1-wise matrix)` builds (`just ci matrix`), and its run fails on 6 real failures instead
(issues 2891 and 2815).

## Lead

Not reproduced locally yet. The refusal is the parameter-capacity rule the 0.6.0-nros12 breaking
fragments describe (`+param-capacity-from-declared-code`, `+image-param-bounds-stated`): a string
or array parameter declared in code or launch must state its capacity. A feature-demo image that
declares one without a capacity is the likely subject. Issue 2702's per-image configure
(`2a83e9aa61`, after this run) changes the build dir names, so re-read the next scheduled run
before fixing.

## Acceptance

`just build tier1` and `just build tier2-nightly` pass their linux stage in a scheduled
`run-matrix` run. A release blocker: [phase-590](../roadmap/phase-590-nros12-release-plan.md) W1.
