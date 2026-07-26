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
