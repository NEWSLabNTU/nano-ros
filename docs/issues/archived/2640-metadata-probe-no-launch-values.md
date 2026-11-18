---
id: 2640
title: "The C/C++ metadata probe cannot construct a node that declares parameters without defaults, or more of them than a default store holds"
status: resolved
type: bug
area: [cli, params, cmake]
severity: medium
found: 2026-11-16
resolved: 2026-11-18
related: [phase-478, phase-566]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md).**
Reported by the Autoware Safety Island (phase-15 P3) on nano-ros 5b9e8844a4.

## What happens

The metadata probe (`orchestration/metadata_probe_cmake.rs`, run from
`metadata_refresh.rs`) builds a one-node host plan for each C/C++ component
(`probe_plan`) with `params: Vec::new()` and `param_services: false`, and
links it against a store at the default knobs. An rclcpp-shaped Autoware
node therefore runs its constructor with no launch values and a store
smaller than its declarations:

- a `declare_parameter<T>(name)` with NO default reads `T()`: the island's
  controller divides by `ctrl_period = 0` and sizes a deque from the NaN;
- past the store's capacity every further declaration silently returned its
  compiled default (GitHub #4018; since issue 2634 it stops the run, so the
  component goes unprobed instead).

So the node cannot be probed, and the island duplicates every declaration's
default in C++ to keep the probe alive.

## Store size (resolved 2026-11-16)

The probe's build now states `NROS_MAX_PARAMETERS=1024`
(`params.max_parameters`) beside `NROS_EXECUTOR_MAX_CBS=64`
(`pin_probe_env`), for the reason issue 1969 pinned the executor's: the probe
is what produces the declaration count, so nothing can derive the size for it.
Both knobs are part of `probe_inputs_key`, so a sidecar from a probe built
under other sizes reads stale and is probed again. The probe's cargo directory
was already keyed on them (`nros_knob_key_fields`).

## Launch values (resolved 2026-11-18)

The design is [phase-566](../../roadmap/phase-566-probe-launch-seeds.md) (draft,
for review): the probe seeds the component's launch values from the model the
sync just resolved, through `resolved_params` (the bake's projection). A
component whose instances disagree is probed on its defaults as today, and is
refused with a plain message only when that unseeded probe fails. The sidecar
records `probe.seeds` (or `probe.unseeded`), with `sha256` digests.

## Resolution (2026-11-18)

Both halves are closed. The store half is above. The launch-value half is
[phase-566](../../roadmap/phase-566-probe-launch-seeds.md) W1-W3:

- **One projection.** `launch_param_text::baked_params` (nros-orchestration-ir)
  is the one function from a model's node to its baked launch text. The C/C++
  entry (`codegen/entry::plan_from_model`), `nros::main!`, the planner's record
  (`plan_record_from_model`) and the probe's seeds all call it; it was spelled
  three times before. `instance_executable` is the matching rule for a node's
  executable, shared with the entry.
- **The seeds.** `orchestration/probe_seeds.rs` reads the models the sync
  resolved (`<ws>/build/nros/models`, `NROS_MODEL_DIR`, or the sync's
  `--model-dir`) through `model_gate::verify`, finds each C/C++ component's
  instances by `(pkg, exec)` or its composable `plugin`, and applies D2: no
  instance, no seeds; agreeing instances, their values; disagreeing ones,
  unseeded, with a note when that probe succeeds and a refusal naming every
  instance's values when it fails. `probe_plan` puts the seeds on
  `PlanNode.params`, so the probe's main seeds them with the same
  `nros_cpp_declare_node_param` calls an image's entry makes, before it
  constructs the node.
- **Freshness.** `metadata_refresh::component_probe_key` is the package key
  plus `sha256:` over the seed set, asked by the refresh, the failure marker
  and every reader (`fresh_probe_rows`, `stale_sidecar_components`). The
  sidecar records the seeds as a CLI-stamped `probe` object.

Measured on `packages/testing/nros-tests/fixtures/probe_seeds_ws` (one C and
one C++ component, each reading `ctrl_period_ms` with no default and declaring
41 parameters; launch values 20 and 25 ms), built by its `workspace-probe-seeds`
row:

| | C (`seeds_c`) | C++ (`seeds_cpp`) |
|---|---|---|
| unseeded probe (main's `params: Vec::new()`) | probe FAILED, exit 255; the row's build refused | timers `[]`, 41 parameters |
| seeded probe | timers `[20]`, 41 parameters, `probe.seeds` | timers `[25]`, 41 parameters, `probe.seeds` |

Editing the C++ node's launch value to 30 re-probed it (timer `[30]`);
restoring it re-probed that component alone (`1 rebuilt, 1 already current`),
and a sync with nothing changed probed nothing.

Rust components are probed by the cargo harness, which takes no seeds yet
(phase-566 W4): their key is unchanged and they are probed on their defaults,
as before.
