# Phase 126 - ROS 2 workflow orchestration MVP

**Goal.** Turn the design in
[`docs/design/ros2-user-workflow.md`](../design/ros2-user-workflow.md) into
an implementation plan that standard ROS 2 users can follow: a colcon-like
workspace of component packages, launch files as the system description, ROS
launch manifests as graph requirements, and one generated nano-ros binary for
the target.

**Status.** Complete (archived). MVP shipped end-to-end: schema,
Rust source metadata, launch planner + checker, and generated
per-board binaries. All milestones M1–M7 met; the M5 platform-
coverage extension verified 9 boards (native, FreeRTOS, NuttX,
Zephyr, ThreadX-Linux, ThreadX-riscv64, ESP32-C3, bare-metal
Cortex-M, STM32F4), with Orin SPE triaged as a license-gated opt-in
cell. The nine "Deliberate deferrals" moved to
[Phase 172](../phase-172-orchestration-deferred.md).

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

Remaining orchestration gap is broad validation:

- broad `just` matrix has not been run for the integrated Phase 126 path.

**2026-05-20.** `just test-all` now invokes the codegen-side
orchestration E2E suite (5 tests: plan/check/build native, mixed-C
component archive, services/actions, FreeRTOS-QEMU boot, one-shot
`nros build --launch`) via the new `just native _test-orchestration-e2e`
step. Each test self-gates on toolchain availability; the suite
runs alongside the existing C codegen step at the bottom of
`test-all`. Promotes Phase 126's coverage from "tested manually
inside the codegen submodule" to "exercised every time CI runs
`just ci`". Verified locally: 5 passed; 0 failed in ~43 s wall.

## Progress update - 2026-05-15

Integrated on `main` through:

- top-level `0ab89120 phase-126: update codegen launch deps`;
- nested `packages/codegen` `9b98daf phase-126: vendor launch parser deps`.

Recent focused validation:

- `cargo test --manifest-path packages/Cargo.toml -p nros-cli-core orchestration`
  passed with 19 tests;
- `cargo test -p nros-cli-core generated_package` passed with 3 tests;
- `cargo test -p nros component` passed with 7 tests after metadata ergonomics
  landed.

Coverage work now becomes the priority. The team should move from feature
construction to fixture coverage, integration coverage, and regression gates.

## Progress update - 2026-05-15 E2E fixture

Integrated nested `packages/codegen` `0cc6a3d phase-126: add orchestration e2e
fixture`.

The committed fixture under
`packages/codegen/testing_workspaces/orchestration_e2e/` now drives:

- source metadata preservation;
- `play_launch_parser` launch parsing into `record.json`;
- manifest-backed `nros plan`;
- `nros check`;
- generated package creation and native Cargo build.

Focused validation:

- `cargo test --manifest-path packages/nros-cli-core/Cargo.toml
  fixture_workspace_plans_checks_and_builds_generated_package -- --nocapture`;
- `cargo test --manifest-path packages/nros-cli-core/Cargo.toml`.

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
- [x] Generated `main.rs` opens one executor, creates one default `SchedContext`,
  instantiates all planned Rust components, binds callbacks, and spins.
  (Already wired in `templates/orchestration/main.rs.jinja` — pre-existed
  126D archival; verified by 126.M4 close.)
- [x] Generated code is readable and checked into `build/`, not hidden in opaque
  binary blobs.
- [x] `nros build` runs metadata, plan, generation, and Cargo build in one
  command. (Codegen submodule `555707e`: `nros build --launch <file>` chains
  `metadata::run` → `plan::run` → `build_generated_package`; also dropped
  dead `nros/rmw-*-cffi` + `link-tcp` feature emissions left over from
  Phase 128.C, unblocking every existing orchestration_e2e Cargo build.)

### M5 - RTOS generated binary

Extend M4 to one RTOS/QEMU target.

Acceptance:

- [x] Build uses existing board/platform recipes.
  (`nros-board-mps2-an385-freertos` `path =` dep wired in generated
  Cargo.toml; `Cargo.toml.jinja` adds the FreeRTOS branch.)
- [x] Transport/env compile-time options are captured in `nros-plan.json`.
  (`plan.build.target` / `plan.build.rmw` / `platform_feature(board)`
  drive every feature flag the generator emits.)
- [x] Runtime hot path is allocation-free; static capacities come
  from the plan. (`nros_generated.rs` emits `pub static` / `pub const`
  tables; `CallbackHandleTable` is const-generic on `CALLBACK_COUNT`;
  `SCHED_CONTEXTS` / `INSTANCES` / `NODES` / `PARAMETERS` / `CALLBACK_BINDINGS`
  are all static. No heap on the target.)
- [x] No JSON/TOML parsing happens on target. (Generated `build.rs`
  reads `nros-plan.json` on the host, emits Rust-typed static tables;
  target code only sees `nros_generated::*`.)

Closed by codegen submodule `71f1bb0` (FreeRTOS e2e banner assertion
fix unblocks `fixture_workspace_builds_and_boots_generated_freertos_package`).
M4's generator + template work (codegen `555707e` /
`templates/orchestration/{Cargo.toml,build.rs,main.rs}.jinja`) covers
the runtime + build wiring; M5 just had to land the QEMU boot check
+ correct the assertion drift.

#### M5 platform-coverage extension (beyond the single-RTOS bar)

M5's acceptance only required ONE RTOS target (satisfied by FreeRTOS).
The generator was then extended board-by-board so `nros build` emits a
correct package for every supported board. Each board contributes a
`render_platform_dependencies` arm + a `render_cargo_config` branch +
a `main.rs.jinja` (or `lib.rs.jinja`) entry + a local `platform-*`
feature alias + an `orchestration_e2e` fixture test.

Three boards (mps2-an385, ESP32, STM32F4) share the single
`nros/platform-bare-metal` nros feature; the generator disambiguates
them with per-board discriminators (`esp32_chip()` / `stm32_chip()`)
+ DISTINCT local entry-gating aliases (`platform-bare-metal` /
`platform-esp32-qemu` / `platform-stm32`) so exactly one board entry
compiles. ThreadX likewise splits one `nros/platform-threadx` feature
into `platform-threadx` (host-hosted Linux) + `platform-threadx-riscv64`
(bare-metal RV64) by target.

| Board | Target | Generated entry | e2e fixture | Status |
|---|---|---|---|---|
| native | host | `fn main` (from_env) | `fixture_workspace_plans_checks_and_builds_generated_package` | ✅ verified |
| FreeRTOS (mps2-an385) | thumbv7m-none-eabi | `_start` → board run | `…_freertos_package` (QEMU boot) | ✅ verified |
| NuttX (qemu-arm) | armv7a-nuttx-eabihf | `nsh_main` chain | `…_nuttx_package` | ✅ verified |
| Zephyr (native_sim) | host (CMake) | staticlib `rust_main` | `…_zephyr_package_shape` | ✅ verified |
| ThreadX-Linux | x86_64-linux | `fn main` → board run | `…_threadx_linux_package` | ✅ verified |
| ThreadX-riscv64 | riscv64gc-none-elf | `#[no_mangle] extern "C" fn main` | `…_threadx_riscv64_package` | ✅ verified |
| ESP32-C3 | riscv32imc-none-elf | `#[esp_hal::main]` | `…_esp32_package` (QEMU boot) | ✅ verified |
| bare-metal Cortex-M | thumbv7m-none-eabi | `#[cortex_m_rt::entry]` | `…_bare_metal_package` | ✅ verified |
| STM32F4 | thumbv7em-none-eabihf | `#[cortex_m_rt::entry]` + defmt | `…_stm32f4_package` | ✅ verified |
| **Orin SPE (Cortex-R5F)** | armv7r-none-eabihf | C-entry `nros_app_rust_entry` (staticlib) | — | **opt-in / unverified** |

**Orin SPE is a deliberate opt-in gap.** It is the only board whose
dependency chain (down to `zpico-sys`'s build.rs) refuses to compile
without an NVIDIA SDK Manager install exporting `$NV_SPE_FSP_DIR`
(FreeRTOS FSP headers + `libtegra_aon_fsp.a`). Per `CLAUDE.md`'s SDK-
tier policy — *"ARM FVP, NVIDIA SDK Manager, license-gated installs
stay opt-in entirely"* — the generator is NOT extended to orin-spe:
the codegen shape cannot be compiled, let alone verified, in CI or on
a dev host without the license-gated SDK, so shipping blind codegen
would violate trust-but-verify. The board also has no in-tree example
to mirror and no QEMU path (real Cortex-R5F SPE hardware only).

If a contributor with an SDK Manager install wants orin-spe codegen,
the generator work mirrors the Zephyr staticlib pattern: a
`crate-type = ["staticlib"]` package exporting
`#[no_mangle] pub extern "C" fn nros_app_rust_entry()` that calls
`nros_board_orin_spe::run(Config::default(), …)`, targeting
`armv7r-none-eabihf`, consumed by NVIDIA's FSP Makefile via
`ENABLE_NROS_APP := 1`. The e2e fixture would gate on `$NV_SPE_FSP_DIR`
(skip when absent, like the NuttX `$NUTTX_DIR` gate).

### M6 - mixed-language components

Merge C/C++ component ABI and generated archive linking.

Acceptance:

- [x] C and C++ packages can expose component registration thunks.
- [x] Generated Rust package links C/C++ static archives in plan order.
- [x] C++ symbols do not cross the Rust boundary directly; generated C ABI thunks
  are the stable boundary.

### M7 - services/actions and workflow polish

Finish services/actions in metadata, plan, checker, and generated runtime.

Acceptance:

- [x] Services and actions are represented like pub/sub with role-specific
  endpoints and callbacks.
- [x] `nros check` validates pub/sub/service/action graph consistency.
- [x] Docs show the standard workflow end to end.

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

These nine items were kept out of the MVP and now live in
[Phase 172 — Orchestration follow-ups](../phase-172-orchestration-deferred.md):

- lifecycle node orchestration;
- automatic callback-chain inference;
- automatic callback-group inference;
- incremental/staleness optimization;
- hardened metadata-mode sandboxing;
- polished `nros explain`;
- multi-tier scheduling;
- runtime parameter override persistence;
- generated shared state.
