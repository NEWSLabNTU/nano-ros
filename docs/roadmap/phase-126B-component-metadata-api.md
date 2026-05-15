# Phase 126.B - component metadata API

**Goal.** Add component-mode APIs that are natural for ROS 2 users and make
metadata discovery a byproduct of normal node declaration.

**Status.** Rust MVP implemented through runtime node mapping. Coverage
hardening continues. C/C++ remain deferred.

**Priority.** P1 for Rust MVP, P2 for C/C++ after native generated binary.

**Depends on.** Phase 126.A schema types. Builds on existing `nros`,
`nros-node`, `nros-c`, `nros-cpp`, and `nros-macros`.

## Scope

MVP covers Rust components only:

- `nros::Component` trait;
- `nros::ComponentContext`;
- `nros::NodeOptions`;
- metadata-aware node/entity creation APIs;
- `nros::component!` export macro;
- host metadata mode that writes source metadata JSON.

C and C++ are planned in the same API shape but land after M4 unless needed
earlier for a specific integration.

## Design constraints

- Component packages do not define `main()`.
- Component API should feel close to rclrs/rclcpp/rclc, but must stay no_std and
  allocation-aware.
- Entity creation in component mode requires stable IDs. Anonymous callback
  creation should not exist in component mode.
- Forgetting `nros::component!` should produce "package has no exported nros
  component".
- Forgetting entity metadata should be impossible because component-mode
  entities are created through `ComponentContext`/metadata-aware node APIs.
- Callback effects (`reads`, `publishes`, `writes`) are optional additive
  metadata and must not replace the normal create/publish/read API.

## Work items

- [x] **126.B.1 - Rust component trait.**
  Add `Component`, `ComponentContext`, `NodeOptions`, and export macro to the
  public `nros` crate.
- [x] **126.B.2 - Metadata recorder context.**
  Implement a fake host-side context that records declarations instead of
  opening middleware.
- [x] **126.B.3 - Runtime context adapter.**
  Implement the runtime path that maps the same declarations to executor/node
  handles under generated main ownership.
- [x] **126.B.4 - Stable entity IDs.**
  Require IDs on publishers, subscriptions, timers, services, actions,
  callbacks, and parameters in component mode.
- [x] **126.B.5 - Name resolution placeholders.**
  Record source names and name kind: absolute, relative, private. Do not resolve
  remaps in source metadata.
- [x] **126.B.6 - Optional effect metadata.**
  Add builder-style `.reads()`, `.publishes()`, `.writes()` metadata that does
  not alter runtime behavior.
- [x] **126.B.7 - Metadata command hook.**
  Provide a library entry for `nros metadata` / `nros plan` to compile and run
  metadata mode for a package.
- [ ] **126.B.8 - C component API.**
  Add `nros_component_context_t`, `NROS_COMPONENT(...)`, and metadata/runtime
  dual-mode calls. Deferred unless M6 is pulled earlier.
- [ ] **126.B.9 - C++ component API.**
  Add `nros::ComponentNode`, `nros::NodeOptions`, and
  `NROS_COMPONENTS_REGISTER_NODE(...)`. Deferred unless M6 is pulled earlier.

## Progress update - 2026-05-15

Integrated Rust metadata coverage includes:

- multi-node component declaration tests;
- private-name placeholder metadata;
- parameter defaults;
- service/action declarations;
- distinct action callbacks;
- callback effect links in emitted JSON;
- source locations and planner-facing metadata shape.
- runtime adapter coverage for stable node IDs, executor node-handle mapping,
  duplicate node rejection, and unknown callback-effect entity rejection.

Latest focused validation:

- `cargo test -p nros component` passed with 11 component/metadata/runtime
  adapter tests.
- `cargo check -p nros --features rmw-cffi` passed, including the
  `ComponentExecutorRuntime` adapter backed by `Executor`.

Next coverage focus:

- generated-main wiring that calls `ComponentExecutorRuntime` during
  `126.D.3`;
- metadata-mode package fixture that produces JSON as part of a full workspace
  flow;
- negative tests for missing component export once host package discovery is
  wired end to end.

## Files

- `packages/core/nros/src/component.rs` (new)
- `packages/core/nros/src/component_metadata.rs` (new)
- `packages/core/nros/src/lib.rs`
- `packages/core/nros-macros/src/lib.rs`
- `packages/core/nros-node/src/executor/node_record.rs`
- later: `packages/core/nros-c/include/nros/component.h`
- later: `packages/core/nros-c/src/component.rs`
- later: `packages/core/nros-cpp/include/nros/component.hpp`
- later: `packages/core/nros-cpp/include/nros/component_node.hpp`

## Acceptance criteria

- [x] A Rust component package emits source metadata without opening transport.
- [ ] The same component can be instantiated by generated runtime code. The
      runtime adapter now exists; generated-main wiring is tracked by
      `126.D.3`.
- [x] Metadata contains unresolved source names, interface types, QoS,
      callbacks, params, and optional effects.
- [x] Component-mode entity APIs require stable IDs.
- [ ] Missing export macro fails clearly during metadata discovery/check.
- [x] Existing hand-written `main()` examples remain supported as simple-app
      path and are not pulled into orchestration.
