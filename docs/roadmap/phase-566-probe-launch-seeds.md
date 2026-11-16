# Phase 566 — the metadata probe runs a node with its launch values

**Status (2026-11-16). DRAFT: design for review; no item has started.**
Closes the second half of [issue 2640](../issues/2640-metadata-probe-no-launch-values.md).
The first half, a probe parameter store that cannot fill, is a separate fix
and is not designed here.

## Why

The C/C++ metadata probe (`orchestration/metadata_probe_cmake.rs`) is the
census. It runs each component's constructor on the host to count what the code
registers, and writes the sidecar (`<pkg>/<component>.json`) that sizing, the
contract join and the planner read. Its one-node plan (`probe_plan`) has
`params: Vec::new()`. So an rclcpp-shaped node runs its constructor with NO
launch values.

That is harmless for a node whose every `declare_parameter` has a default. An
Autoware-shaped node declares without one and takes its value from a
`config/*.param.yaml` through `<param from>`. Under the probe such a declaration
reads `T()`. The Autoware Safety Island's controller divides by
`ctrl_period = 0` and sizes a deque from the resulting NaN, so the constructor
fails and the component cannot be probed. The island works around it by
restating every default in C++, which is a second copy of its parameter files
that can drift from them.

A probe that seeds the launch values does what the image does: a firmware entry
seeds every launch parameter before it builds the node (`declare_calls.jinja`).
The probe already constructs the node through the same emitter
(`emit_typed_probe`). Only the seeds are missing.

## Design

### D1 -- where the values come from: the launch RECORD

Each bringup's launch tree is parsed by `nros-launch-resolve --record` (the
raw parse, `LaunchDump`), and the probe reads the record, not the SystemModel.

- The record holds what the probe needs: per node and per container,
  `package`, `executable` / `plugin`, the fully-qualified name, and
  `param_sources` in ROS order (inline values and `<param from>` file contents).
  `ros_launch_manifest_model::NodeInstance::resolved_params` already folds that
  order, and the same projection feeds the bake.
- The record comes from launch XML alone. The model is built on top of the
  contracts, and the model road's sizing joins the workspace's FRESH probe
  sidecars onto its rows (issue 1594, `fresh_probe_inventory`). If the probe
  read the model, a sidecar's freshness would depend on a file whose own
  consumers depend on that sidecar, which is an ordering loop inside one
  `nros configure`. The record has no such dependency.
- One record per bringup per sync, written beside the model
  (`build/nros/models/<bringup>/<stem>.record.json`). It is a configure-time
  input, listed with the other consulted inputs (issue 2361).

### D2 -- which instance seeds a component

The probe is per COMPONENT (`package`, `executable`). A launch tree may
instantiate the component several times, across bringups and launch files.

- No instance: no seeds. The probe runs as today.
- One instance, or several whose resolved parameter maps are EQUAL: seed that
  map.
- Several instances whose maps DIFFER on any key: **refuse**, and record the
  component as unprobeable with a plain message. For example:

      metadata probe for controller_pkg::controller: its launch instances
      disagree on parameters, so there is no one set of values to construct it
      with: /control/controller (launch/system.launch.xml) has
      ctrl_period = 0.03, /carla/controller (launch/system_carla.launch.xml) has
      ctrl_period = 0.05. Probe one instance per bringup (not supported yet), or
      make the values agree.

Is there a case where choosing is right? I looked for one and found two
candidates. Both fail:

1. Keys the constructor never reads (a value only a timer callback reads later).
   The probe cannot tell which keys the constructor reads, so it cannot know
   which differences are harmless.
2. Keys that only change values, not entity counts (a gain, not a topic or a
   rate). The census exists to observe exactly what the code registers, and a
   parameter can decide that (`if (use_x) create_subscription(...)`). Picking
   an instance would make the sidecar describe one instance and be applied to
   all of them. That is the silent wrong answer issue 1594's join refuses
   elsewhere.

So no choice is made. Where two instances differ in a way that matters, the
honest fix is a sidecar per instance, which changes the key every consumer
joins on. That is out of scope here, and the refusal names it.

Keys that EXIST in one instance and are absent in another count as a
difference: absence means the declaration's default, which is a value.

### D3 -- how the values reach the node

`probe_plan` fills `PlanNode.params` from the chosen map, through
`nros_orchestration_ir::launch_param_text` (issue 2638's writer), the same
text a firmware entry bakes. The probe's main seeds them with the same
`nros_cpp_declare_node_param` calls before it builds the node, so the probe and
the image cannot disagree about a value's type. `qos_overrides.*` keys are
split out exactly as the entry lowering splits them.

The probe's store must hold the seeds plus the declarations. That is the first
half of issue 2640 (a probe store sized so it cannot fill), and this phase
depends on it.

### D4 -- `probe.seeds` provenance and the sidecar schema

The sidecar gains one object, written only when seeds were applied:

```json
"probe": {
  "seeds": {
    "instances": ["/control/controller"],
    "sources": [{"path": "build/nros/models/controller_bringup/system.record.json",
                 "sha256": "<record digest>"}],
    "digest": "<fnv-1a over the sorted (key, baked text) pairs>"
  }
}
```

`SOURCE_METADATA_SCHEMA_VERSION` goes from 5 to 6, additive like 2 to 5: a
reader that does not know `probe` ignores it.

What the bump does to the model road's contract join (issue 1594,
`fresh_probe_inventory`, `merge_declared_endpoints_into_winners`):

- **The join's rule is unchanged.** It keys rows by component and trusts a
  sidecar only when it is FRESH against `probe_inputs_key`. Nothing in the join
  reads `probe`.
- **Freshness must cover the seeds**, or an edit to a param file leaves a
  sidecar that reads fresh while describing the old values. The join would
  then trust it, which is issue 1578's failure (a stale positive cache) in a
  new place. So `probe_inputs_key` becomes
  `<pkg digest>:<CLI stamp>:<closure>:<seeds digest or "-">`. A seed change
  restales the sidecar, the next sync re-probes, and until then the join SKIPS
  it with its existing note. The rows are unobserved, never under-sized.
- **A version-5 sidecar on disk is stale after the upgrade**, because the key
  gained a field. Every C/C++ component re-probes once. That is the same
  one-time cost every CLI stamp change already has.
- **A refused component (D2) has no sidecar**, and the join treats it as it
  treats any unprobeable component today: its endpoints come from declarations
  only, and the sizing reports them unobserved. The unprobeable marker is keyed
  on the same key, so making the instances agree clears it.
- The negative-cache marker carries the D2 message as its recorded reason, so a
  later sync prints the cause, not only "skipped" (issue 1469).

The Rust cargo harness (`metadata_build.rs`) runs Rust components the same
way. It gets the same seeds through the harness's launch-params table. Rust
nodes rarely declare without a default, so it is item W4, not a precondition.

## Work items

- [ ] W1: `nros configure` writes `<stem>.record.json` per model
      (`nros-launch-resolve --record`), and lists it as a consulted input.
- [ ] W2: `metadata_refresh` maps each probed component to its record
      instances and applies D2 (seed, or refuse with the message).
- [ ] W3: `probe_plan` carries the seeds (D3); `probe_inputs_key` gains the
      seeds digest; the sidecar writes `probe.seeds`; schema 6 (D4).
- [ ] W4: the Rust harness takes the same seeds.

## Test

- A fixture component that declares `ctrl_period` WITHOUT a default and
  creates a timer at `1 / ctrl_period`. Unseeded, the probe fails (NaN). With
  the launch file's value, the sidecar records the timer and
  `probe.seeds.instances`.
- Two launch files instantiating it with different `ctrl_period`: the probe
  refuses with the D2 message, and the marker carries it.
- Editing only the param file restales the sidecar (the key changes), and the
  next sync re-probes. `fresh_probe_inventory` skips the stale one meanwhile.
- A version-5 sidecar is read as stale, never trusted.

## Not done here

- A sidecar per INSTANCE (the case D2 refuses). It changes the join key from
  component to instance for every consumer.
- Seeding from the SystemModel. See D1 for why the record is the source.
