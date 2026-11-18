# Phase 566 -- the metadata probe runs a node with its launch values

**Status (2026-11-18). W1-W3 landed for C and C++; W4 (the Rust harness)
is open.** Closed the second half of [issue 2640](../issues/archived/2640-metadata-probe-no-launch-values.md).
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

### D1 -- where the values come from: the resolve run's model, through `resolved_params`

The seeds come from the SAME resolve run whose model the bake reads, through
the SAME projection: `NodeInstance::resolved_params(fqn)` on
`model.structure.nodes`, then `launch_param_text`. That is what
`codegen/entry` (`PlanNode.params`) and `plan_record_from_model` already do,
so a seed and a baked value are one computation, not two that agree.

- **No second parse.** An earlier draft ran `nros-launch-resolve --record`
  a second time for the probe. Two parses of one launch tree are two outputs
  that can differ without anyone noticing: a different binary, a different
  argument binding (`meta.args`, phase-326), a different `--system`. That is
  issue 0409's class: a stale resolver wrote a well-formed model with every
  `params` / `params_files` quietly gone, and nothing compared it with what
  the bake used. One run, one projection, leaves nothing to compare.
- **No ordering loop.** The earlier draft feared one. There is none: `nros
  sync` resolves every bringup's model (`resolve_system_models`) BEFORE it
  refreshes the sidecars (`refresh_source_metadata`, which runs last), and
  the resolver's inputs are the launch tree and `system.toml`, never a
  sidecar. The sizing join (issue 1594, `fresh_probe_inventory`) reads
  sidecars LATER, onto rows of the model; it writes nothing back into
  `structure.nodes`. So the probe reads a model that cannot depend on the
  probe's own output.
- **Which models.** Every model the sync just resolved
  (`build/nros/models/<bringup>/<stem>_model.yaml`, or `NROS_MODEL_DIR`),
  read through `model_gate::verify`. A refused or quarantined model is not a
  seed source; the sync has already failed on it (phase-460 W1).
- Each model consulted is a configure-time input (issue 2361), recorded as
  its path and its `sha256`.

### D2 -- which instance seeds a component

The probe is per COMPONENT (`package`, `executable` or `plugin`). The models
may instantiate it several times, across bringups and launch files. A
component whose instances differ is common (Autoware launches one node type
under several names with different values), and today it probes fine on its
declaration defaults. So the rule refuses only when seeding is actually
needed, and never regresses a component that probes today.

- **No instance:** no seeds. The probe runs as today.
- **One instance, or several whose resolved maps are EQUAL:** seed that map.
- **Several instances whose maps DIFFER on any key:** probe UNSEEDED, exactly
  as today.
  - The unseeded probe succeeds: keep its sidecar, record why it is unseeded
    (D4, `probe.unseeded`), and print one note:

        sync: metadata probe for controller_pkg::controller ran on its
        declaration defaults: its launch instances disagree on ctrl_period
        (/control/controller 0.03, /carla/controller 0.05), so no one set of
        values constructs it

  - The unseeded probe fails: **refuse**, with the probe's own error and the
    disagreement:

        metadata probe for controller_pkg::controller: the node cannot be
        constructed on its declaration defaults (<probe error>), and its
        launch instances disagree on parameters, so there is no one set of
        values to construct it with: /control/controller
        (launch/system.launch.xml) has ctrl_period = 0.03, /carla/controller
        (launch/system_carla.launch.xml) has ctrl_period = 0.05. Probe one
        instance per bringup (not supported yet), or make the values agree.

  This is the same outcome the component has today (it is unprobeable), now
  with the cause named.

Why not choose an instance when they differ? Two candidates, and both fail:

1. Keys the constructor never reads (a value only a timer callback reads
   later). The probe cannot tell which keys the constructor reads, so it
   cannot know which differences are harmless.
2. Keys that only change values, not entity counts (a gain, not a topic or a
   rate). The census exists to observe what the code registers, and a
   parameter can decide that (`if (use_x) create_subscription(...)`). Picking
   an instance makes the sidecar describe one instance and be applied to all
   of them, which is the silent wrong answer issue 1594's join refuses
   elsewhere.

The defaults fallback is not that choice. It is what the probe observes today
for every multi-instance component, and the note and `probe.unseeded` make it
visible instead of silent. Where two instances differ in a way that matters,
the honest fix is a sidecar per instance, which changes the key every
consumer joins on. That is out of scope, and the refusal names it.

Keys that EXIST in one instance and are absent in another count as a
difference: absence means the declaration's default, which is a value.

**Rust components.** D2 does NOT apply to Rust components before W4. Until
the cargo harness (`metadata_build.rs`) takes seeds, a Rust component is
probed unseeded exactly as today, whatever its instances say: no refusal, no
note, no `probe` object, and its key's seeds field is `-` (D4). W4 applies D1
and D2 to Rust unchanged, fallback included.

### D3 -- how the values reach the node

`probe_plan` fills `PlanNode.params` from the seeded map (D2), already in
`launch_param_text` form (issue 2638's writer, D1), the same text a firmware
entry bakes. The probe's main seeds them with the same
`nros_cpp_declare_node_param` calls before it builds the node, so the probe and
the image cannot disagree about a value's type. `qos_overrides.*` keys are
split out exactly as the entry lowering splits them.

The probe's store must hold the seeds plus the declarations. That is the first
half of issue 2640 (a probe store sized so it cannot fill), resolved by #4044
(`NROS_MAX_PARAMETERS=1024`, part of the key as `<knobs>`).

### D4 -- `probe` provenance and the sidecar schema

One digest algorithm: `sha256`, the one the bake's side already uses for
files and locks (`model_gate`'s `meta.inputs`, `configure_stamp`,
`project_lock`). The probe key's existing FNV-1a fields stay what they are
(`source_digest` and `probe_closure_digest` walk source trees, and
`recompute_digest` dispatches on the prefix); everything this phase adds is
`sha256:`.

The sidecar gains one object, written only when the probe had launch
instances to consider:

```json
"probe": {
  "seeds": {
    "instances": ["/control/controller"],
    "sources": [{"path": "build/nros/models/controller_bringup/system_model.yaml",
                 "sha256": "<model file digest>"}],
    "digest": "sha256:<over the seed set, below>"
  }
}
```

or, for the D2 fallback, `"unseeded": {"reason": "instances disagree",
"instances": [...], "keys": [...], "sources": [...], "digest": "sha256:..."}`
in place of `"seeds"`.

The seed-set digest is `sha256` over the canonical text of the DISTINCT
resolved maps of the component's instances, each map its sorted
`(key, baked text)` pairs, the maps sorted. One map when they agree, several
when they do not, so a change to any instance's values moves it, including a
change that makes disagreeing instances agree.

`SOURCE_METADATA_SCHEMA_VERSION` goes from 5 to 6, additive like 2 to 5: a
reader that does not know `probe` ignores it.

What the bump does to the model road's contract join (issue 1594,
`fresh_probe_inventory`, `merge_declared_endpoints_into_winners`):

- **The join's rule is unchanged.** It keys rows by component and trusts a
  sidecar only when it is FRESH against `probe_inputs_key`. Nothing in the
  join reads `probe`.
- **Freshness must cover the seeds**, or an edit to a param file leaves a
  sidecar that reads fresh while describing the old values. The join would
  then trust it, which is issue 1578's failure (a stale positive cache) in a
  new place. So `probe_inputs_key` becomes
  `<pkg digest>:<CLI stamp>:<closure>:<knobs>:<seed-set digest or "-">`
  (`-` for no instances, and for every Rust component before W4). A seed
  change restales the sidecar, the next sync re-probes, and until then the
  join SKIPS it with its existing note. The rows are unobserved, never
  under-sized.
- **A version-5 sidecar on disk is stale after the upgrade**, because the key
  gained a field. Every C/C++ component re-probes once. That is the same
  one-time cost every CLI stamp change already has.
- **A refused component (D2) has no sidecar**, and the join treats it as it
  treats any unprobeable component today: its endpoints come from
  declarations only, and the sizing reports them unobserved. The unprobeable
  marker is keyed on the same key, so making the instances agree clears it.
- The negative-cache marker carries the D2 message as its recorded reason, so
  a later sync prints the cause, not only "skipped" (issue 1469).

## Work items

- [x] W1: `metadata_refresh` reads the models the sync just resolved
      (through `model_gate::verify`), maps each probed component to its
      instances, and projects each through `resolved_params` +
      `launch_param_text` (D1). The models are consulted inputs.
- [x] W2: D2: seed, or probe unseeded and keep (note + `probe.unseeded`), or
      refuse when the unseeded probe fails.
- [x] W3: `probe_plan` carries the seeds (D3); `probe_inputs_key` gains the
      seed-set digest (`sha256`); the sidecar writes `probe`; schema 6 (D4).
- [ ] W4: the Rust harness takes the same seeds, and D2 applies to Rust.

### As landed (W1-W3, 2026-11-18)

- The projection is one function, `launch_param_text::baked_params`, called
  by the C/C++ entry, `nros::main!`, `plan_record_from_model` and the probe
  (`orchestration/probe_seeds.rs`). The seeds leave out `qos_overrides.*`
  keys: they are the node's QoS table, not declared parameters, and the census
  records the QoS the code asks for. The D2 comparison and the digest are over
  the same filtered maps, so a QoS-only edit does not re-probe.
- The models are found by listing the models directory (the sync's
  `--model-dir`, else `NROS_MODEL_DIR`, else `<ws>/build/nros/models`), not
  from a list the sync hands over, so every reader that recomputes the key
  reads the same set. A reader that cannot see a `--model-dir` the sync was
  given reads the sidecar as stale and skips it with its note.
- D4's key field is appended only when launch instances name the component
  (`metadata_refresh::component_probe_key`). A component nothing launches, and
  every Rust component, keeps the key it had, so only seeded components
  re-probe once after the upgrade.
- The sidecar's `probe` object is stamped CLI-side beside `provenance`, so the
  recorder's `SOURCE_METADATA_SCHEMA_VERSION` (already 6) did not
  move; the CLI mirror (`SourceMetadata::probe`) reads it.
- Measured on `fixtures/probe_seeds_ws` (row `workspace-probe-seeds`, test
  `tests/probe_seeds_workspace.rs`): unseeded, the C component's probe failed
  and the C++ one recorded no timer; seeded, both record their launch periods
  (20 and 25 ms) and all 41 parameters.

## Test

- A fixture component that declares `ctrl_period` WITHOUT a default and
  creates a timer at `1 / ctrl_period`. Unseeded, the probe fails (NaN). With
  the launch file's value, the sidecar records the timer and
  `probe.seeds.instances`.
- The seeds equal the bake's values: for the fixture, the probe's
  `PlanNode.params` and the firmware entry's `PlanNode.params` for the same
  instance are the same pairs (one projection, D1).
- Two launch files instantiating it with different `ctrl_period`: the
  unseeded probe fails, so the probe refuses with the D2 message, and the
  marker carries it.
- A component that HAS a default for every key, instantiated twice with
  different values: the probe runs unseeded, succeeds, writes
  `probe.unseeded`, prints the note, and the sidecar's endpoints equal
  today's (no regression).
- Editing only the param file restales the sidecar (the key changes), and the
  next sync re-probes. `fresh_probe_inventory` skips the stale one meanwhile.
- A version-5 sidecar is read as stale, never trusted.
- A Rust component with disagreeing instances is probed as today, key field
  `-`, no `probe` object (before W4).

## Not done here

- A sidecar per INSTANCE (the case D2 refuses). It changes the join key from
  component to instance for every consumer.
- A second launch parse (`--record`) as the seed source. See D1.
- Seeding the keys disagreeing instances AGREE on, with the rest at defaults.
  It is closer to the launch than no seeds, but it is a third case to reason
  about, and a probe that passed unseeded could fail half-seeded. D2 keeps
  the two cases it has.
