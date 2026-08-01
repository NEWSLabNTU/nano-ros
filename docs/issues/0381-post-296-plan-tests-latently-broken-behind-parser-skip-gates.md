---
id: 0381
title: Five plan/launch tests still assert the deleted pre-296 parse path,
  hidden by play_launch_parser skip-gates
status: open
severity: medium
created: 2026-08-01
tags: [tests, orchestration, phase-296-fallout]
related: [0285, 0368]
phases: [phase-296]
---

## What happened

phase-296 R-code deleted `nros plan`'s launch-XML parse path (committed
SystemModel is the only input). Five tests still stage raw launch XML and
expect the old resolver behavior:

- `launch_synth::nros_plan_refuses_path_a_bringup_with_no_launch` —
  expects the "Path A / synthesis is disallowed" wording; plan now bails
  earlier with the phase-296 "no committed SystemModel" error.
- `launch_synth::nros_plan_picks_pkg_named_default` — expects a
  `record.json` produced by parsing the picked launch file.
- `workspace_dirwalk_discovery::*` (2 tests) — dirwalk discovery is still
  live logic, but the staged bringups carry no committed model, so plan
  bails before the discovery result is observable.
- `orchestration_includes::depth_cap_rejects_over_16` — the `<include>`
  depth cap moved into ros-launch-resolve; probing it through `nros plan`
  can no longer reach it.

## Why nobody saw it

Each test opens with `play_launch_parser_available()` → `skip!`. No dev or
CI host had `play_launch_parser` on PATH, so the suite skipped everywhere
since phase-296 landed. The first host provisioned through the product
path (`nros setup` installs the parser — 2026-08-01) ran them for the
first time and they all failed. Same shape as issue 0196: a skip-gate
keyed on tool availability quietly retired the tests instead of the
behavior.

## Fix shape

1. Re-point each test at the post-296 seam it actually wants:
   - discovery tests: stage a committed `config/system_model.yaml` and
     assert plan's CHOICE (which bringup/model was planned), not parse
     output;
   - Path-A refusal: assert the current no-model error contract, or the
     self-model synth path (`plan.rs` synthesise_self_model) where that is
     the intended behavior;
   - depth-cap: drive `nros-launch-resolve` directly (it owns include
     expansion now).
2. Drop the `play_launch_parser_available()` gates from tests that no
   longer shell out to the parser (post-296, `nros plan` never does).
3. Sweep for the class: any `skip!` gate keyed on tool availability where
   the tool is no longer invoked by the code under test.
