# Phase 126.C - launch and manifest planner

**Goal.** Implement the host-side planner that turns ROS 2 launch files, ROS
launch manifests, source metadata, and `nros.toml` into a checked
`nros-plan.json`.

**Status.** Draft, not started.

**Priority.** P1.

**Depends on.** Phase 126.A schemas and Phase 126.B Rust source metadata. Uses
`play_launch_parser` and `ros-launch-manifest` as libraries.

## Scope

This phase owns `nros metadata`, `nros plan`, and `nros check` behavior. It
does not own generated target code; that is Phase 126.D.

The planner consumes:

- frozen `record.json` from `play_launch_parser`;
- generated source metadata JSON;
- ROS launch manifests from `ros-launch-manifest`;
- component and system `nros.toml`;
- selected config overlays and current compile-time transport options.

The planner emits:

- `build/<system_pkg>/nros/record.json`;
- `build/<system_pkg>/nros/metadata/<pkg>.json`;
- `build/<system_pkg>/nros/nros-plan.json`.

## Work items

- [ ] **126.C.1 - CLI verbs.**
  Add `nros metadata`, `nros plan`, and `nros check` to `nros-cli-core`.
- [ ] **126.C.2 - play_launch parser adapter.**
  Call `play_launch_parser::parse_launch_file` directly, not the
  `play_launch` CLI. Preserve `record.json` for user inspection.
- [ ] **126.C.3 - Workspace/package discovery.**
  Discover colcon-like `src/*` packages, package manifests, component
  metadata, launch files, and `nros.toml`.
- [ ] **126.C.4 - Instance normalization.**
  Convert `record.json` nodes and composable load nodes into nano-ros launch
  instances keyed by ROS `package` + `executable`.
- [ ] **126.C.5 - ROS name resolution.**
  Resolve namespaces, private names, relative names, and remaps using ROS 2
  conventions. Preserve trace data to source placeholder names.
- [ ] **126.C.6 - Parameter resolution.**
  Apply ROS precedence: source defaults, package defaults, parameter files,
  launch inline params, launch CLI args, then nano-ros deployment overlays.
- [ ] **126.C.7 - Manifest matching.**
  Match manifest endpoints to source entities by instance, role, resolved name,
  and interface type. Support explicit `endpoint_mappings` for ambiguity.
- [ ] **126.C.8 - Scheduling normalization.**
  Convert system config callback groups and sched contexts into plan entries.
  Validate that every local callback has a sched binding, defaulting only when
  the config says to default.
- [ ] **126.C.9 - Services/actions.**
  Treat services/actions like role-specific endpoint groups: service
  request/response, action goal/cancel/result/feedback/status.
- [ ] **126.C.10 - Checker diagnostics.**
  Fail for missing components, missing entities, unresolved types, QoS
  mismatch, ambiguous mappings, and invalid sched bindings.

## Files

- `packages/codegen/packages/nros-cli-core/src/cmd/mod.rs`
- `packages/codegen/packages/nros-cli-core/src/cmd/metadata.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/cmd/plan.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/cmd/check.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/planner.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/workspace.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/names.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/params.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/manifest.rs` (new)

## Acceptance criteria

- [ ] `nros plan <system_pkg> <launch_file> -- <launch_args...>` writes
      `record.json` and `nros-plan.json`.
- [ ] Multiple instances of the same package/executable map to separate plan
      instances with distinct names, parameters, callbacks, and telemetry IDs.
- [ ] Private source topic names resolve correctly through launch remaps.
- [ ] ROS manifest pub/sub endpoints validate against source metadata.
- [ ] Services/actions are represented in the plan even if full runtime support
      lands later in Phase 126.D/M7.
- [ ] `nros check` can run after `nros plan` and explain every plan error with
      package, instance, entity, and source artifact references.
