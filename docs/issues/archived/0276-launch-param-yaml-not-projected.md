---
id: 276
title: "Launch *.param.yaml values not projected into node parameters — upstream no-default declares can't port verbatim"
status: resolved
type: enhancement
area: codegen
related: [0254, 0255]
resolved_in: "issue-0276 (launch param-file projection)"
---

## Finding (autoware-safety-island-example ports, 2026-07-24 — porting-notes 06)

Upstream Autoware nodes declare parameters with NO default
(`declare_parameter<double>("target_acceleration")`) and receive values from
`config/*.param.yaml` via the launch pipeline. nano-ros parameters are
node-local: `declare_parameter(name, default)` only. The port had to copy
every yaml value into in-code defaults — a silent-drift hazard against the
upstream configs (values now live in two places).

Distinct from 0254 (the compat API surface): even with a
`declare_parameter` shim, there is no channel that projects launch-time
param-file values into the node.

## Direction

The model/codegen already consumes the launch description: lower resolved
param-file values per node into the generated entry (compile-time baked on
embedded — matching the domain-id precedent), so no-default declares
resolve at boot. Runtime file loading is NOT required for parity on
embedded targets.

## Resolution (2026-07-26)

The channel now exists end-to-end.

1. **Parser.** `nros-launch-parser` used to hard-error on `<param from="…"/>`
   ("`<param>` missing `name=`"), so an upstream Autoware launch file could not
   even parse. The `param` arm now recognises both ROS forms; `from=` (with
   substitutions resolved) collects into the new `NodeSpec.param_files`.
2. **Projection.** `codegen/entry` reads each param file at CODEGEN time and
   flattens it into `PlanNode.params`, which the emitters already bake into the
   generated entry. Compile-time baking matches the domain-id precedent — no
   runtime file loading on embedded.
3. **Precedence** follows ROS: `/**` wildcard block, then the node-specific
   block, then inline `<param name= value=>` last (inline wins). Node keys match
   on the FQN (`/ns/node`), a bare `node`, or `/**/node`. Nested maps flatten to
   dotted names (`limits: {max_accel: …}` → `limits.max_accel`); sequences
   stringify as `[a, b]`.
4. **Rebuild correctness.** Every param file read is recorded as a depfile
   input, so editing the YAML re-runs codegen.
5. **Failure mode.** A `from=` naming a missing file is a hard error. Silently
   skipping it would ship a node whose no-default declares fail at boot — the
   exact failure this issue exists to prevent.

### Caveat — the seeding is gated on `param_services`

Both emitters write the values as `nros_cpp_declare_param(...)` seeds inside an
`if plan.param_services` block. With param services OFF the values are computed
and planned but never reach the node. Enabling `param_services` in `system.toml`
is therefore a precondition for the parity this issue describes.

Verbatim porting of an upstream node that calls `declare_parameter<T>(name)`
with NO default still additionally needs the compat API surface tracked in
**0254** — this issue delivers the value channel that such a declare reads from,
not the declare signature itself.

### Tests

- `nros-launch-parser`: `param_from_collects_param_files` (both forms, `$(var …)`
  in `from=`)
- `orchestration::params`: `param_file_values_matches_wildcard_fqn_and_bare_keys`,
  `param_file_values_node_block_outranks_wildcard`
- `codegen::entry`: `plan_from_launch_projects_param_file_values`,
  `plan_from_launch_rejects_missing_param_file`

## Follow-up (2026-07-26) — the model path needed the same projection

The resolution above fixed the LAUNCH path (`nros-launch-parser`
`<param from=>` → `NodeSpec.param_files` → `resolve_node_params` inside
`plan_from_launch`). But phase-296 R-code made the resolved SystemModel the
only live bake input: `codegen entry --launch` is a removal error, and
`plan_from_launch` is now reachable only from `entry_typed_plan.rs`. Every
live path — `plan_from_model`, the `nros::main!(model = …)` macro arm, and
`plan_record_from_model` — still read `inst.params` raw and dropped
`params_files` on the floor.

Closed by projecting model-side too: `NodeInstance::resolved_params(fqn)` in
the shared model crate (rlm `0612574`) merges the verbatim `params_files`
YAML under the inline `<param>` values, with ROS 2 section matching (`/**`,
node FQN, bare name), dotted flattening for nested maps, sequences →
`StrList`, and an unparsable file skipped rather than fatal. All three model
consumers call it. Tests: `model/tests/params_projection.rs` (wildcard+FQN
precedence, later-file-wins + foreign-section skip, unparsable-file).

Both projections now agree on precedence (files in declaration order, inline
`<param>` highest); the launch-side one stays for its test and for any
future re-enablement of that arm.

## Fidelity audit vs standard ROS (2026-07-27)

Checked both engines against `rcl_yaml_param_parser` semantics.

**play_launch (Linux spawn): faithful.** `node_cmdline.rs` materializes each
`params_files` entry to disk and passes one `--params-file` per file in
declaration order, then writes the inline `<param>` values to an
`overrides.yaml` rendered LAST. All node-key matching therefore happens inside
rcl itself — wildcards, namespaces and nested params are whatever ROS says
they are, by construction. (It also sidesteps `-p` parser limits: `::` in
names, empty values.)

**nano-ros (compile-time bake): had a matching gap, now closed.** The bake
cannot delegate to rcl — there is no rcl on the target — so
`resolved_params` re-implements the matcher, and the first cut recognized only
`/**`, exact FQN, and a bare name. rcl also accepts PARTIAL wildcards
(`/sensing/**`, `/*/planner`, `/foo/*/bar`), which real Autoware configs use,
so whole sections were being dropped silently. Fixed in rlm `2a8f952`:
segment-wise globbing where `**` consumes any number of segments (including
zero — `/sensing/**` also selects `/sensing`) and `*` exactly one.

**Known divergence, both engines, documented not fixed.** ROS 2 launch takes
ONE ordered `parameters=[…]` list mixing dicts and files, so
`parameters=[{"a": 1}, "f.yaml"]` lets the file win for `a`. Our model splits
the two (`NodeInstance.params` vs `.params_files`) and loses the interleaving,
so both engines apply files first and inline last — inline always wins.
play_launch (`overrides.yaml` last) and the nano-ros bake agree with each
other; they diverge from upstream only when a launch file deliberately orders
a file AFTER an inline dict for the same key. Restoring exact fidelity would
need an ordered parameter-source list in the model.
