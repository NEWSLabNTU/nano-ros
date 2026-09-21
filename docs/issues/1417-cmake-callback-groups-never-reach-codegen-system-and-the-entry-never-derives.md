---
id: 1417
title: "For a cmake image the tier derivation is unreachable twice: codegen-system
  collects callback groups from cargo metadata only, and codegen entry never
  derives at all - derived tiers reach nros-plan.json and no image"
status: open
type: bug
area: [cli, codegen, cmake]
severity: high
found: 2026-09-21
related: [issue-1371, issue-1372, issue-1312, issue-1397, phase-459, rfc-0032, rfc-0047, rfc-0052]
---

## What was observed

Brief D experiments E5b-E5e on the Autoware Safety Island (four C++
components registered with `nros_components_register_node`, one timer each
at 30 Hz or 10 Hz), pin f8655e9b7, re-verified below against 783cdfa14:

* `[[component]] group_tiers = { main = "ctrl" }` without `[tiers.ctrl]`:
  `codegen-system` refuses, `references undeclared tier 'ctrl'`
  (`packages/cli/nros-cli-core/src/orchestration/model_ingest.rs:108`);
  `codegen entry` refuses, `names tier ctrl, which has no [tiers.ctrl]
  definition`.
* With `[tiers.ctrl]` declared: derivation is skipped, because
  `codegen_system.rs:433` derives only when `model.execution.tiers.is_empty()`.
* Groups with no bindings, model resolved without `--system`: refused at
  `model_ingest.rs:212`, the `group_tiers` reached no node.
* With an authored `[tiers.ctrl.zephyr] priority = 5` (E5e) the entry ends in
  `run_tiers` - from the AUTHORED number, written three times.

`build-board/nros-metadata.json` carries `callback_groups: []` for all four
components. The realizer never ran on this image and could not have been made
to from any authored input.

## The two gaps (verified at 783cdfa14)

**Gap 1 - `codegen-system` cannot see cmake groups.** `collect_callback_groups`
(`packages/cli/nros-cli-core/src/orchestration/tier_resolver.rs:35-90`) reads
`[[component]].group_tiers`, then `cfg.component_packages[pkg].nros.callback_groups`.
`NrosConfig::from_workspace` (`orchestration/nros_config.rs:198-212`) returns
`component_packages: BTreeMap::new()` for a workspace with no root
`Cargo.toml`. The cmake keyword `CALLBACK_GROUPS` (`cmake/NanoRosVerbs.cmake:255`
on `nano_ros_add_node`, `:441` on `nros_components_register_node`, emitted by
`cmake/NanoRosNodeRegister.cmake:1252` into `nros-metadata.json`) is read
only by `codegen entry`'s `metadata::enrich_plan`. So on the cmake road the
only way to give `codegen-system` a group is `group_tiers`, and a
`group_tiers` binding needs a declared tier, and a declared tier disables
derivation. The gate in `derive.rs:96` ("a node with no groups stays on the
default tier") is therefore always taken for a cmake image, whatever the
author writes.

**Gap 2 - the entry never derives.** `codegen entry` builds its plan in
`plan_from_model` (`packages/cli/nros-cli-core/src/codegen/entry/mod.rs:751`),
takes tiers from `model.execution.tiers` (`:918`) and resolves them with
`resolve_plan_sched` (`:985`); `grep derive_ packages/cli/nros-cli-core/src/codegen/entry`
is empty. The derived tiers `codegen-system` produces live in its in-memory
`SystemToml` and reach `nros-system/nros-plan.json` (`codegen_system.rs:1135`)
and nothing else. So even with gap 1 closed, `run_tiers` would not be
emitted for a derived schedule; every `run_tiers` in the tree today comes
from an authored `[tiers.*.<rtos>]`.

RFC-0032 section 5.1 describes the degenerate gate as the case where nothing
was declared. On the cmake road it is the only case.

## What is not this issue

* The silence of the groupless note: issue 1371.
* The trigger rate being rebuilt from the output's `min_rate_hz`: issue 1372.
* `--target zephyr-<rmw>` naming no block: issues 1312 and 1397, resolved;
  the Zephyr module passes `--for-entry` (`zephyr/cmake/nros_system_generate.cmake:217-224`).
* The allocation placing rank 0 at Zephyr priority 0, above the transport:
  issue 1418.

## What would fix it

phase-459 W1 and W2: `collect_callback_groups` gains the workspace metadata
as a third source through the `load_workspace_metadata` reader
`model_ingest.rs:344` already uses; `plan_from_model` runs
`derive_tiers_from_contracts` when the model declares no tiers and any node
carries groups, with the board's RTOS key. W3 adds `[tiers.X] derived = true`
so a binding can name a tier and still ask for its priority.

## Acceptance

phase-459's W0 fixture (four components, `CALLBACK_GROUPS main`, no
`group_tiers`, no tiers): `codegen-system` derives two tiers;
`codegen entry --lang cpp --board zephyr` ends in `run_tiers(..., 2u)` with
the two 30 Hz nodes in tier 0 and the two 10 Hz nodes in tier 1. Keyword
removed: four groupless notes, `run_components`.
