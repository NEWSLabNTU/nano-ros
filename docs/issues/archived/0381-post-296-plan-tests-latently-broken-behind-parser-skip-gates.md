---
id: 0381
title: Five plan/launch tests still assert the deleted pre-296 parse path,
  hidden by play_launch_parser skip-gates
status: resolved
severity: medium
created: 2026-08-01
tags: [tests, orchestration, phase-296-fallout]
related: [0285, 0368]
phases: [phase-296]
---

## Resolution (2026-08-02)

Re-pointed the tests at the post-296 seams and dropped the
`play_launch_parser_available()` skip-gates that hid them:

- `launch_synth.rs` — the two tests asserting the DELETED launch-XML
  synthesis / `<pkg>.launch.xml`-precedence path were removed (that behavior
  moved to `ros-launch-resolve`, tested in its own repo); the Path-A refusal
  is re-pointed to the current contract (`nros plan` bails with "no committed
  SystemModel … launch-XML parse path was removed"). Needs only the `nros` CLI.
- `workspace_dirwalk_discovery.rs` — both tests now stage a committed
  `demo_bringup/config/system_model.yaml` and assert `nros plan` DISCOVERED it
  under the non-member bringup dir (the discovery signal), not full-plan
  success — the model names a component whose source-metadata this fixture
  doesn't stage, so the metadata walk fails after discovery (out of scope).
- `orchestration_includes.rs` — kept the portable `--record` chain test;
  removed the cycle + depth-cap tests (they drove the deleted parse path;
  include expansion + its cap/cycle checks are owned by `ros-launch-resolve`).

`self_bringup.rs` was NOT in the broken class — it exercises the LIVE
`synthesise_self_model` path (a 1-node model, no parser), verified passing.
All four files now RUN (previously all skip-hidden) and pass. The
tool-availability skip-gate class (fix item 3) was swept: no remaining
`play_launch_parser` gate guards a test whose code path no longer invokes it.
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
