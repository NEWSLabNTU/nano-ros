---
id: 2598
title: "A resolved model stays stale after `nros configure`, and a launch parameter file is not one of its inputs"
status: open
type: bug
area: [cli, orchestration, launch]
severity: high
found: 2026-11-15
related: [phase-478]
---

GitHub: NEWSLabNTU/nano-ros#3923

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md), I12.**
Reported by the Autoware Safety Island on 0.6.0-nros10, reproduced on main
(play_launch pin `cac40d8b`).

## What happens

Two predicates answer "is this resolved model current?", and they disagree.

1. **`nros configure` asks the configure stamp.** Its digest walks a fixed set
   of file names: `system.toml`, `package.xml`, interface files, and launch
   files under `launch/` (`configure_stamp::is_input`). It reports
   `configure: up to date` when that digest matches and every model file
   exists (`cmd/build.rs` `workspace_model_missing`).
2. **Every consumer asks the model gate** (`model_gate::verify`), which hashes
   each input the MODEL recorded in `meta.inputs`.

An input that the model records and the stamp does not walk can change
without moving the stamp. The build then refuses the model:

```text
SystemModel `.../maximum_safety_model.yaml` is stale: input hash changed
`config/maximum_safety/param_talker.param.yaml`
  Run `nros configure` to re-resolve it.
```

But `nros configure` answers `up to date` and does nothing, so the advice
loops. Only `rm -rf build` gets out. That is #3923.

The second half is worse because it is silent. The resolver
(`ros-launch-resolve`, `resolve/src/model.rs`) records the launch files, the
contracts, the platform file and `system.toml`, but **not a `<param
from="...">` file**, although it copies that file's values into the model. So
after a parameter-file edit, neither predicate notices. `nros build` and
`nros configure` keep the old values with no warning.

Measured with a mixed image shaped like the island's
(`<param from="$(dirname)/../config/$(var branch)/x.param.yaml"/>`,
`[image.host_maximum_safety] args = { branch = "maximum_safety" }`): editing
the branch file from 200 to 250 and running `nros build` and then
`nros configure` left the generated entry passing `"200"`. The default
model and a `[[model]]` variant behave the same.

## Fix

- **nano-ros:** `workspace_model_missing` asks `model_gate::verify` once the
  model exists. A stale model is "not current", so configure re-resolves it
  and names the input that moved:
  `configure output missing: the resolved model ... is stale: input hash changed
  config/maximum_safety/param_talker.param.yaml`.
- **play_launch:** `build_checked_model` adds every parameter file the parser
  recorded (`param_sources` entries of kind `file` with a `path`) to the
  provenance inputs. Committed as play_launch `191d949f`; it reaches nano-ros
  with the next pin bump.

Tests: `workspace_model_staleness_tests` (nano-ros) and
`model::tests::a_param_file_is_a_model_input` (play_launch). Each fails with
its fix removed.

## Workaround

Until both halves ship: `rm -rf build/nros/models` and run `nros configure`
after editing a parameter file.
