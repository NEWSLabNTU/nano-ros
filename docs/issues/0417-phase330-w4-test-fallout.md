---
id: 417
title: "tests still read committed SystemModels that phase-330 W4 deleted"
status: open
type: bug
area: testing
related: [phase-330, phase-336, issue-0380]
---

## Symptom

`just ci` (tier 1) fails `test-all` with a handful of tests reporting a missing
model file:

```
read /mnt/wd/aeon/nano-ros/examples/workspaces/rust/src/demo_bringup/config/
  multihost_robot1_model.yaml: No such file or directory (os error 2)

committed model missing at …/examples/workspaces/features/src/demo_bringup/
  config/rust_qos_model.yaml
```

Reproduces on a clean `origin/main`: the model files are absent from the tree
while the tests that read them are tracked.

## Cause

phase-330 W4 made the SystemModel a pure BUILD ARTIFACT — `39d007dfc` deleted
every committed copy, and `check-no-tracked-models` now rejects adding one back.
The consumer migration was completed for the CLI (`plan`, `codegen-system`,
`nros-build`, `nros-macros`, `NanoRosEntry.cmake` all resolve through
`nros_orchestration_ir::model_location`), but a set of tests still name the
committed path directly and read it as a source file.

The tests are not wrong to want a model — the model is their INPUT. They are
wrong to expect the repo to ship it.

## Affected

Confirmed failing (2026-08-05, `just ci`):

- `nros-tests::multihost_partition_bake committed_per_host_models_carry_their_binding`
- `nros-tests::multihost_partition_bake multihost_bake_emits_only_the_hosts_node`
- `nros-tests::qos_override_e2e the_committed_model_declares_a_reliability_override_that_lowers`
- `nros-tests::native_main_macro_misuse rebuilds_on_model_touch`

Other files naming a committed model path, not yet triaged:

- `packages/testing/nros-tests/tests/workspace_dirwalk_discovery.rs`
- `packages/testing/nros-tests/tests/zephyr_edf_deadline_applied.rs`
- `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`

## Fix

Two shapes already applied to the same class in phase-336 W7, either works:

1. **Carry the model inline** (`entry_typed_plan`, `plan_pipeline_e2e`): the test
   holds the YAML as a `const` and writes it to a tempdir. Right when the model
   is small and the topology IS the fixture.
2. **Generate it**: run `nros sync` for the workspace and read through
   `model_location`, which already searches the build-output rungs. Right when
   the test needs the model the real flow would produce.

Do NOT re-commit the yamls — `check-no-tracked-models` rejects them, and issue
0380 records four separate times a regeneration silently deleted hand-edits.

## Notes

Found while verifying phase-336 (build-profile propagation), not caused by it.
Three related consumer-side defects from the same W4 shift were fixed there:
`entry_typed_plan` and `plan_pipeline_e2e` (same class as above), and three CLI
error messages that instructed users to create the committed path that
`check-no-tracked-models` rejects.
