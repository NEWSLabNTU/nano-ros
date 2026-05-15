# Phase 126.D - generated main and build

**Goal.** Generate a debuggable Rust entry package that owns the only `main()`,
instantiates all planned node instances, applies RT scheduling, and builds one
nano-ros binary for the selected target.

**Status.** Complete for the Phase 126.D MVP. Generated native and FreeRTOS
entry packages build, native fixture runs against local `zenohd`, and the
generated FreeRTOS binary boots under QEMU in the E2E fixture.

**Priority.** P1 for Rust/native/one RTOS target. Mixed C/C++ component archive
linking first pass is complete for native generated packages.

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
- [x] **126.D.3 - Rust component instantiation.**
  Call Rust component constructors directly using plan-derived
  `NodeOptions`/`InstanceSpec`.
- [x] **126.D.4 - SchedContext generation.**
  Convert plan sched contexts to `Executor::create_sched_context` calls and
  callback bindings to `bind_handle_to_sched_context`.
- [x] **126.D.5 - Static capacity generation.**
  Derive executor/node/callback/parameter/interface capacities from the plan
  and pass them as env vars/features/build constants.
- [x] **126.D.6 - Collective interface cache.**
  Generate all required Rust/C/C++ interfaces once under
  `build/<system_pkg>/nros/interfaces/`.
- [x] **126.D.7 - `nros build` system mode.**
  Make `nros build` detect a system package and run metadata -> plan ->
  interface generation -> generated package -> target build.
- [x] **126.D.8 - Native generated binary.**
  Build and run the fixture on POSIX/native first.
- [x] **126.D.9 - RTOS generated binary.**
  Build and run the fixture on one existing QEMU RTOS target.
- [x] **126.D.10 - C/C++ component link path.**
  Generate C ABI component registration thunks and link C/C++ static archives
  into the Rust entry package. Covered by the mixed-language native E2E
  fixture.

## Progress update - 2026-05-15

Integrated generated-package/build coverage includes:

- `nros-orchestration` runtime table crate;
- generated package templates for `Cargo.toml`, `build.rs`, and `main.rs`;
- host-side `build.rs` conversion from `nros-plan.json` to typed Rust tables;
- deterministic generated package output tests;
- plan-derived Cargo features;
- plan-derived build args and target-dir layout;
- no-std generated main gating.
- plan-derived node tables plus generated Rust component dispatch using the
  `crate::module::Component` convention;
- E2E fixture Rust component crate linked into the generated package.
- generated backend registration for selected RMW backends;
- generated POSIX C platform-port dependency for native generated binaries;
- generated FreeRTOS board/panic dependencies, target cargo config, and `_start`
  wrapper for the MPS2-AN385 FreeRTOS board crate;
- generated timer/subscription callback handles that populate
  `CallbackHandleTable`;
- collective interface cache manifests under
  `build/<system_pkg>/nros/interfaces/{rust,c,cpp}/`;
- E2E fixture launches a local `zenohd` router and verifies the generated native
  binary stays alive in the executor spin loop;
- E2E fixture builds a generated FreeRTOS package for `thumbv7m-none-eabi` and
  boots it under QEMU long enough to assert the nros FreeRTOS platform banner.
- E2E fixture links generated native packages against C and C++ static archives
  through generated C ABI registration thunks.

Latest focused validation:

- `cargo test -p nros component` passed with 11 component/runtime tests.
- `cargo test -p nros-orchestration` passed.
- `cargo test --manifest-path packages/codegen/packages/nros-cli-core/Cargo.toml
  --test orchestration_generate` passed with 6 tests.
- `cargo test --manifest-path packages/codegen/packages/nros-cli-core/Cargo.toml
  --test orchestration_e2e` passed, including generated package compile with
  the fixture Rust component dependency, selected backend, POSIX platform C
  symbols, generated callback handles, interface cache manifests, a
  multi-instance generated package build, a live native run against local
  `zenohd`, generated FreeRTOS build/boot coverage, and mixed C/C++ static
  archive linking.
- `cargo check -p nros-node --features rmw-cffi` passed.
- `NROS_LOCATOR=tcp/127.0.0.1:7447 timeout 3s
  /tmp/orchestration_e2e-301-1778849578197518498/build/e2e_system/nros/target/x86_64-unknown-linux-gnu/debug/nros-e2e-generated`
  timed out with exit 124 while local `zenohd` was running, confirming the
  generated binary opens transport and spins.
- Generated FreeRTOS E2E built for `thumbv7m-none-eabi` and booted under
  `qemu-system-arm -cpu cortex-m3 -machine mps2-an385`, printing the nros QEMU
  FreeRTOS platform banner before the bounded timeout.

Next coverage focus:

- Phase M7 services/actions metadata, plan, checker, and generated-runtime
  coverage.

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
- [x] Generated package builds native with one Rust component fixture.
- [x] Generated package runs native with one Rust component fixture.
- [x] Generated package builds native with two instances of the same component.
- [x] Generated package applies final params/remaps from the plan.
- [x] Generated package creates and binds `SchedContext`s from the plan.
- [x] Generated package builds for one QEMU RTOS target.
- [x] `nros build` produces artifacts under
      `build/<system_pkg>/nros/target/<triple>/<profile>/`.

## Non-goals

- Multi-tier executors.
- Lifecycle orchestration.
- Runtime parameter override persistence.
- Generated shared state.
- Incremental rebuild optimization.
- Polished monitor UI.
