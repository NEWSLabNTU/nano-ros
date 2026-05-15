# Phase 126 - ROS 2 workflow orchestration MVP

**Goal.** Turn the design in
[`docs/design/ros2-user-workflow.md`](../design/ros2-user-workflow.md) into
an implementation plan that standard ROS 2 users can follow: a colcon-like
workspace of component packages, launch files as the system description, ROS
launch manifests as graph requirements, and one generated nano-ros binary for
the target.

**Status.** MVP implementation integrated through schema, Rust metadata,
planner/checker, and generated-package build scaffolding. Coverage phase is
starting.

**Priority.** P1. This is the user workflow layer above Phase 123 build/API
work and Phase 110 scheduling.

**Depends on.**

- Phase 111: `nros` CLI exists, but needs orchestration verbs.
- Phase 123: source-ship/package workflow and C/C++/Rust ergonomics.
- Phase 124: RMW dispatch, wake, services/actions, and probe coverage.
- `~/repos/play_launch`: `play_launch_parser` emits `record.json`; ROS launch
  manifest crates provide graph requirements and static checks.

**Related phase docs.**

- [Phase 126.A - schema and plan IR](phase-126A-schema-plan-ir.md)
- [Phase 126.B - component metadata API](phase-126B-component-metadata-api.md)
- [Phase 126.C - launch and manifest planner](phase-126C-launch-manifest-planner.md)
- [Phase 126.D - generated main and build](phase-126D-generated-main-build.md)

## Implementation gap summary

The runtime foundation is strong: `nros-node` already has a multi-node
executor, service/action paths, callback handles, and `SchedContext`. C and
C++ bindings expose most executor features. Codegen packages already provide
message/service/action generation and colcon integration.

The first orchestration layer is now in place:

- source metadata schemas and Rust component-mode metadata APIs exist;
- `nros metadata` / `nros plan` / `nros check` scaffolding exists in
  `nros-cli-core`;
- `nros-plan.json` has typed schema, fixtures, and checker coverage;
- `nros-orchestration` runtime crate exists;
- generated Rust entry package scaffolding exists, with deterministic package
  output tests;
- planner and generator tests cover multi-instance, private-name/remap,
  parameter, manifest, callback-effect, and generated-package cases.

Remaining orchestration gaps are coverage and vertical integration:

- no committed end-to-end fixture workspace that drives metadata -> plan ->
  generated package -> binary;
- generated main still emits static tables/scaffolding rather than full runtime
  component instantiation for all paths;
- C/C++ component metadata/linking remains deferred;
- RTOS/QEMU generated-binary build is not yet covered;
- broad `just` matrix has not been run for the integrated Phase 126 path.

## Progress update - 2026-05-15

Integrated on `main` through:

- top-level `400ca008 phase-126D: update generated target dir submodule`;
- nested `packages/codegen` `59c9f95 phase-126D: place generated target beside package`.

Recent focused validation:

- `cargo test --manifest-path packages/Cargo.toml -p nros-cli-core orchestration`
  passed with 19 tests;
- `cargo test -p nros-cli-core generated_package` passed with 3 tests;
- `cargo test -p nros component` passed with 7 tests after metadata ergonomics
  landed.

Coverage work now becomes the priority. The team should move from feature
construction to fixture coverage, integration coverage, and regression gates.

## Parallel work groups

### Group A - schemas and shared IR

Owns serializable Rust types for source metadata, `nros.toml`, and
`nros-plan.json`. During coverage work, this group owns schema regression
fixtures and compatibility tests.

Primary files:

- `packages/codegen/packages/nros-cli-core/src/orchestration/` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/schema.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/plan.rs` (new)
- `packages/codegen/packages/nros-cli-core/tests/orchestration_schema.rs` (new)

Output: checked schema structs plus golden fixture tests.

### Group B - component metadata API

Adds the natural API shape that makes metadata generation unavoidable in
component mode. During coverage work, this group owns Rust metadata fixture
packages and metadata JSON regression cases. Rust is the MVP language; C/C++
follow after the first generated binary works.

Primary files:

- `packages/core/nros/src/component.rs` (new)
- `packages/core/nros/src/component_metadata.rs` (new)
- `packages/core/nros-macros/src/lib.rs`
- later: `packages/core/nros-c/include/nros/component.h`
- later: `packages/core/nros-cpp/include/nros/component.hpp`

Output: Rust component packages emit source metadata in host metadata mode.

### Group C - launch and manifest planner

Consumes `record.json` from `play_launch_parser`, ROS launch manifests, source
metadata, and `nros.toml`. Emits normalized `nros-plan.json`. During coverage
work, this group owns end-to-end planner fixtures and `nros check` diagnostics.

Primary files:

- `packages/codegen/packages/nros-cli-core/src/cmd/metadata.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/cmd/plan.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/cmd/check.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/planner.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/names.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/manifest.rs` (new)

Output: `build/<system_pkg>/nros/record.json`,
`build/<system_pkg>/nros/metadata/*.json`, and
`build/<system_pkg>/nros/nros-plan.json`.

### Group D - generated main and build

Creates the runtime and generated package that turns the plan into one target
binary. `build.rs` reads JSON on the host and emits typed Rust tables; RTOS
code does not parse JSON. During coverage work, this group owns generated
package snapshot tests, build-arg/feature/target-dir coverage, and the first
native generated package build fixture.

Primary files:

- `packages/core/nros-orchestration/` (new crate)
- `packages/codegen/packages/nros-cli-core/src/orchestration/generate.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/cmd/build.rs`
- `packages/codegen/packages/nros-cli-core/templates/orchestration/` (new)

Output: generated Rust package under
`build/<system_pkg>/nros/generated/`, artifacts under
`build/<system_pkg>/nros/target/<triple>/<profile>/`.

### Group E - fixtures, integration, and docs

Keeps one vertical workflow building while Groups A-D develop in parallel.
This is now the main coordination track for coverage.

Primary files:

- `examples/orchestration-workspace/` (new)
- `packages/testing/nros-tests/tests/orchestration.rs` (new)
- `docs/design/ros2-user-workflow.md`

Output: one Rust component workspace with launch file, ROS launch manifest,
`nros.toml`, generated plan, native run, then one RTOS/QEMU run.

## Merge milestones

### M1 - schema lock

Merge Group A. Schemas are allowed to evolve, but field ownership and artifact
paths should be stable enough for branch work.

Acceptance:

- [x] `source-metadata.json`, `nros.toml`, and `nros-plan.json` have Rust structs.
- [x] Golden fixtures round-trip through JSON/TOML.
- [x] Unknown fields are either rejected or intentionally preserved; the rule is
  documented.

### M2 - Rust source metadata

Merge Group B's Rust MVP and the `nros metadata` shell from Group C.

Acceptance:

- [x] A Rust component can be compiled/run in metadata mode.
- [x] Metadata includes package, component, node, unresolved topic/service/action
  names, type names, callback IDs, timer IDs, parameters, and optional effects.
- [x] Missing component export fails clearly.

### M3 - plan generation

Merge Group C's planner.

Acceptance:

- [x] `nros plan <system_pkg> <launch_file>` calls the play_launch parser library.
- [x] Launch node instances map to source components by package/executable.
- [x] Multiple instances of the same component produce distinct plan instances.
- [x] Private names and remaps resolve to final graph names.
- [x] ROS manifest endpoints validate against resolved source metadata.
- [x] Parameter precedence follows ROS convention and is recorded in the plan.

### M4 - native generated binary

Merge Group D's single-tier native generated package.

Acceptance:

- [x] Generated package builds with Cargo.
- [ ] Generated `main.rs` opens one executor, creates one default `SchedContext`,
  instantiates all planned Rust components, binds callbacks, and spins.
- [x] Generated code is readable and checked into `build/`, not hidden in opaque
  binary blobs.
- [ ] `nros build` runs metadata, plan, generation, and Cargo build in one command.

### M5 - RTOS generated binary

Extend M4 to one RTOS/QEMU target.

Acceptance:

- Build uses existing board/platform recipes.
- Transport/env compile-time options are captured in `nros-plan.json`.
- Runtime hot path is allocation-free; static capacities come from the plan.
- No JSON/TOML parsing happens on target.

### M6 - mixed-language components

Merge C/C++ component ABI and generated archive linking.

Acceptance:

- C and C++ packages can expose component registration thunks.
- Generated Rust package links C/C++ static archives in plan order.
- C++ symbols do not cross the Rust boundary directly; generated C ABI thunks
  are the stable boundary.

### M7 - services/actions and workflow polish

Finish services/actions in metadata, plan, checker, and generated runtime.

Acceptance:

- Services and actions are represented like pub/sub with role-specific
  endpoints and callbacks.
- `nros check` validates pub/sub/service/action graph consistency.
- Docs show the standard workflow end to end.

## Branching guidance

Use one branch per group. Merge only at milestones unless a shared schema change
is blocking. The cleanest order is:

```text
M1 schema lock
  |-- Group B component metadata
  |-- Group C launch/manifest planner
  |-- Group D generated runtime scaffolding
  |-- Group E fixtures
M2 Rust metadata
M3 plan generation
M4 native generated binary
M5 RTOS generated binary
M6 mixed language
M7 services/actions polish
```

Group D can create placeholder generated packages before Group C is complete,
but must consume real `nros-plan.json` by M4.

## Deliberate deferrals

- lifecycle node orchestration;
- automatic callback-chain inference;
- automatic callback-group inference;
- incremental/staleness optimization;
- hardened metadata-mode sandboxing;
- polished `nros explain`;
- multi-tier scheduling;
- runtime parameter override persistence;
- generated shared state.
