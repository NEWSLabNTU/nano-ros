# Phase 126.A - schema and plan IR

**Goal.** Define the build-time data contracts that let component metadata,
launch records, ROS manifests, and RTOS deployment config meet at one stable
artifact: `build/<system_pkg>/nros/nros-plan.json`.

**Status.** Implemented for MVP. Coverage hardening continues.

**Priority.** P1. This phase is the merge base for the rest of Phase 126.

**Depends on.** Phase 126 umbrella, Phase 111 CLI crate layout, Phase 123
workflow decisions.

## Scope

This phase owns schemas only. It should not implement launch parsing,
component metadata generation, or generated main logic.

Artifacts:

| Artifact | Owner | Purpose |
|---|---|---|
| source metadata JSON | generated | Actual nodes/entities/callbacks a component can create |
| component `nros.toml` | user/package | Reusable component linkage and optional metadata overrides |
| system `nros.toml` | deployer | Target, board, scheduling, config overlays, manifest dirs |
| `nros-plan.json` | generated | Checked build IR consumed by generated package `build.rs` |

## Work items

- [x] **126.A.1 - Add orchestration schema module.**
  Create typed Rust structs under `nros-cli-core` for source metadata,
  component config, system config, and plan output.
- [x] **126.A.2 - Define source metadata schema.**
  Include package ID, component ID, language, exported symbol, nodes,
  publishers, subscribers, timers, services, actions, callbacks, parameters,
  unresolved names, QoS, and optional effects.
- [x] **126.A.3 - Define `nros.toml` schema.**
  Split component-level config from system-level deployment config. Component
  config describes reusable implementation facts. System config describes
  target, board, overlays, scheduling, and explicit endpoint mappings.
- [x] **126.A.4 - Define `nros-plan.json` schema.**
  Include components, launch instances, resolved names, parameter tables,
  callback groups, sched contexts, interface set, build options, traceability
  back to source metadata and `record.json`.
- [x] **126.A.5 - Add golden fixtures.**
  Cover one talker/listener, two launch instances of the same component,
  private topic remap, and one service/action shape.
- [x] **126.A.6 - Add compatibility rules.**
  Decide version field behavior, unknown field behavior, and error messages for
  missing required fields.

## Progress update - 2026-05-15

Integrated schema coverage includes:

- source metadata, component config, system config, and `nros-plan.json`
  schemas;
- private/relative/absolute source name fixtures;
- multiple launch instances and multiple source nodes;
- callback effects and service/action plan shapes;
- scheduling context and build-option fields;
- strict unknown-field checks.

Latest focused validation:

- `cargo test --manifest-path packages/Cargo.toml -p nros-cli-core orchestration`
  passed after downstream planner/build coverage landed.

Next coverage focus:

- compatibility tests for future schema version bumps;
- clearer fixture naming for user-facing examples;
- schema explainability assertions that every plan object has trace/source
  context.

## Schema decisions

- `nros-plan.json` is visible to users and lives under
  `build/<system_pkg>/nros/`.
- Launch files own node instances. Component `nros.toml` does not declare
  instances.
- Source metadata records unresolved names such as `~/cmd`; planner resolves
  final graph names using launch namespace/remap rules.
- ROS launch manifest endpoint IDs do not need to match source entity IDs.
  Matching is by instance, direction/role, resolved name, and interface type.
- Scheduling parameters live in system config overlays, not ROS launch
  manifests. The plan records effective values.
- Runtime target code does not parse these files. `build.rs` converts plan JSON
  into typed Rust tables.

## Files

- `packages/codegen/packages/nros-cli-core/src/orchestration/mod.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/schema.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/source_metadata.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/config.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/plan.rs` (new)
- `packages/codegen/packages/nros-cli-core/tests/orchestration_schema.rs` (new)
- `examples/orchestration-workspace/fixtures/` (new, may be added by Phase 126.E)

## Acceptance criteria

- [x] Schemas round-trip through `serde_json`/`toml`.
- [x] Every schema has a `version` field.
- [x] Golden fixtures have stable formatted output.
- [x] `nros-plan.json` includes enough trace data to explain where each
      instance, entity, callback, parameter, and scheduling binding came from.
- [x] Schema module has no dependency on generated code, play_launch parser, or
      nano-ros runtime crates.
