# Phase 126.D - generated main and build

**Goal.** Generate a debuggable Rust entry package that owns the only `main()`,
instantiates all planned node instances, applies RT scheduling, and builds one
nano-ros binary for the selected target.

**Status.** Generated-package/build scaffolding implemented. Runtime
instantiation and RTOS binary coverage remain.

**Priority.** P1 for Rust/native/one RTOS target. P2 for mixed C/C++ component
linking.

**Depends on.** Phase 126.A plan schema, Phase 126.B component runtime path,
Phase 126.C planner. Builds on existing executor and `SchedContext` APIs.

## Scope

This phase owns:

- `nros-orchestration` runtime crate;
- generated Rust package under `build/<system_pkg>/nros/generated/`;
- generated `build.rs` that reads `nros-plan.json` on the host;
- generated typed Rust tables consumed by RTOS code;
- `nros build` orchestration path for system packages.

It does not add multi-tier scheduling in the MVP. One executor and one default
or configured `SchedContext` set are enough for the first RTOS binary.

## Generated package shape

```text
build/<system_pkg>/nros/
  record.json
  nros-plan.json
  metadata/
  interfaces/
    rust/
    c/
    cpp/
  generated/
    Cargo.toml
    build.rs
    src/main.rs
    src/nros_generated.rs
  target/<triple>/<profile>/
```

`build.rs` reads JSON/TOML and writes Rust source. Target code only sees typed
tables and constants.

## Generated main ordering

```text
open platform/session
create executor
create sched contexts
instantiate launch instances with final params/remaps
bind callbacks to sched contexts
start executor/spin loop
```

Callbacks do not run until all instances are constructed and bindings are
applied.

## Work items

- [x] **126.D.1 - Add `nros-orchestration` crate.**
  Provide `System`, `InstanceSpec`, `SchedContextSpec`, callback binding, and
  component instantiation helpers.
- [x] **126.D.2 - Generated package templates.**
  Add `Cargo.toml`, `build.rs`, `main.rs`, and `nros_generated.rs` templates.
- [ ] **126.D.3 - Rust component instantiation.**
  Call Rust component constructors directly using plan-derived
  `NodeOptions`/`InstanceSpec`.
- [x] **126.D.4 - SchedContext generation.**
  Convert plan sched contexts to `Executor::create_sched_context` calls and
  callback bindings to `bind_handle_to_sched_context`.
- [x] **126.D.5 - Static capacity generation.**
  Derive executor/node/callback/parameter/interface capacities from the plan
  and pass them as env vars/features/build constants.
- [ ] **126.D.6 - Collective interface cache.**
  Generate all required Rust/C/C++ interfaces once under
  `build/<system_pkg>/nros/interfaces/`.
- [x] **126.D.7 - `nros build` system mode.**
  Make `nros build` detect a system package and run metadata -> plan ->
  interface generation -> generated package -> target build.
- [ ] **126.D.8 - Native generated binary.**
  Build and run the fixture on POSIX/native first.
- [ ] **126.D.9 - RTOS generated binary.**
  Build and run the fixture on one existing QEMU RTOS target.
- [ ] **126.D.10 - C/C++ component link path.**
  Generate C ABI component registration thunks and link C/C++ static archives
  into the Rust entry package. Deferred to M6 if needed.

## Progress update - 2026-05-15

Integrated generated-package/build coverage includes:

- `nros-orchestration` runtime table crate;
- generated package templates for `Cargo.toml`, `build.rs`, and `main.rs`;
- host-side `build.rs` conversion from `nros-plan.json` to typed Rust tables;
- deterministic generated package output tests;
- plan-derived Cargo features;
- plan-derived build args and target-dir layout;
- no-std generated main gating.

Latest focused validation:

- `cargo test -p nros-cli-core generated_package` passed with 3 tests after the
  final sweep.

Next coverage focus:

- generated runtime path that instantiates Rust components instead of only
  emitting static tables/scaffolding;
- one native generated binary run;
- one QEMU RTOS generated binary build/run.

## Files

- `Cargo.toml`
- `packages/core/nros-orchestration/Cargo.toml` (new)
- `packages/core/nros-orchestration/src/lib.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/generate.rs` (new)
- `packages/codegen/packages/nros-cli-core/src/orchestration/build.rs` (new)
- `packages/codegen/packages/nros-cli-core/templates/orchestration/Cargo.toml.jinja` (new)
- `packages/codegen/packages/nros-cli-core/templates/orchestration/build.rs.jinja` (new)
- `packages/codegen/packages/nros-cli-core/templates/orchestration/main.rs.jinja` (new)
- `packages/codegen/packages/nros-cli-core/src/cmd/build.rs`
- `packages/testing/nros-tests/tests/orchestration.rs` (new)

## Acceptance criteria

- [x] Generated code is readable and deterministic.
- [x] RTOS target code does not parse JSON/TOML.
- [ ] Generated package builds native with one Rust component fixture.
- [ ] Generated package builds native with two instances of the same component.
- [x] Generated package applies final params/remaps from the plan.
- [x] Generated package creates and binds `SchedContext`s from the plan.
- [ ] Generated package builds for one QEMU RTOS target.
- [x] `nros build` produces artifacts under
      `build/<system_pkg>/nros/target/<triple>/<profile>/`.

## Non-goals

- Multi-tier executors.
- Lifecycle orchestration.
- Runtime parameter override persistence.
- Generated shared state.
- Incremental rebuild optimization.
- Polished monitor UI.
