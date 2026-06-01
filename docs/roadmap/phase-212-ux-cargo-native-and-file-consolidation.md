# Phase 212 — Build-system-native UX + workspace layout

**Status:** OPEN
**Priority:** P1
**Depends on:** Phase 211 (orchestration foundation)
**Supersedes / breaks:** every `component_nros.toml`, every committed
`metadata/*.json`, every root `nros.toml`, the existing `nros build` /
`nros generate-rust` user surface, every Phase 211 fixture's workspace shape.
**No backward compatibility.** Clean break.

## Goal

**`nros` CLI scope:** codegen + env setup + orchestration. Never a build
verb (`nros build` / `test` / `flash` / `monitor` are in §Non-Goals).

**Vendor build tools own build:** cargo for Rust, cmake for C/C++, vendor
SDK for embedded (west / idf.py / make / pio / make).

The split is asymmetric for Rust by design:
- **Rust:** `nros generate-rust && cargo build` — two explicit steps.
  No `build.rs` auto-codegen (the `nros-build` build-dep crate was
  retracted — see §212.C). Cargo's `build.rs` was the only way to make
  codegen implicit, and entangling cargo with nros's codegen state was
  judged worse than the extra step.
- **C++:** `cmake -B build && cmake --build build` — one step. The cmake
  fn `nano_ros_generate_interfaces()` runs codegen at configure time.
- **Embedded:** vendor tool drives. Adapter shim shells
  `nros codegen-system` at configure time (Zephyr cmake fn, NuttX
  Makefile rule, ESP-IDF component, PIO pre-script, …).

Component packages declare themselves in their own native manifest
(Cargo.toml `[package.metadata.nros.*]` tables for Rust; cmake fns
`nano_ros_component_register` / `nano_ros_application` for C++).
Multi-node systems live in a dedicated `<system>_bringup` package
following standard ROS layout — Path A code-free shape.

## Architecture

See companion design documents (live, expected to iterate):
- `docs/design/multi-node-workspace-layout.md` — overall workspace shape +
  open questions
- `docs/design/workspace-layout-by-case.md` — concrete file trees for
  single/multi × rust/cpp + mixed
- `docs/design/rtos-integration-pattern.md` — universal embedded pattern

**Decision summary** (locked, see design docs for rationale):

1. **Build-system-native.** Cargo + CMake stay user-facing. `nros` never
   has a `build` / `test` / `flash` / `monitor` verb.
2. **Three pkg shapes** (§212.L):
   - **Component pkg** — lib only. `nros::component!()` (Rust) or
     `NROS_COMPONENT_REGISTER()` (C++). Codegen owns spin. Embedded-
     deployable. Composable.
   - **Application pkg** — user `main()`. Explicit spin control. Native
     only (RTOS rejects arbitrary main). Two init flavours: launch-aware
     (`nros::init_with_launch_auto()`) or launch-ignoring (`nros::init()`).
   - **Bringup pkg** — Path A code-free. Carries `package.xml` +
     `system.toml` + `launch/system.launch.xml`. NO `Cargo.toml`, NO
     `CMakeLists.txt`, NO `src/`. Required for multi-node composition.
3. **Per-pkg metadata location** (Option α, locked):
   - Rust component/application: `Cargo.toml`
     `[package.metadata.nros.{component,application,deploy.<target>,embedded}]`.
   - C++ component/application: cmake fns (`nano_ros_component_register`,
     `nano_ros_application`, `nano_ros_deploy`) write JSON to build dir
     at configure time; codegen consumes.
   - Bringup pkg (both langs): `system.toml` carries
     `[deploy.<target>]` + `[[domain]]` + `[[bridge]]` + `[[remap]]`.
     `system.toml` lives ONLY in bringup pkgs.
4. **Component class name** = `<pkg-dir-name>::<UserClass>` MANDATORY.
   The pkg dir name is the cargo `[package].name` (Rust) or top
   `project()` name (C++). Enforced by `nros check`.
5. **Launch file policy**:
   - REQUIRED in bringup pkg (`launch/system.launch.xml`).
   - OPTIONAL elsewhere. When absent in a single-pkg, `nros plan` /
     `nros codegen-system` / `nros launch` synthesise an implicit
     launch (`<launch><node pkg=… exec=…/></launch>`) in-memory.
   - Multiple files per pkg supported. Resolution: positional arg →
     `<pkg-name>.launch.xml` → `system.launch.xml` → single file → synth.
6. **Mixed Rust + C/C++** = cmake top-level via Corrosion bridge. Pure
   Rust = cargo top-level. Pure C/C++ = cmake top-level.
7. **Embedded RTOS** = vendor SDK retains its native build tool. nano-ros
   plugs into vendor's external-module hook + bakes the system spec into
   compile-time C config via `nros codegen-system`. Bringup pkg never
   reaches device.
8. **Diagnostics passthrough.** Rustc errors stay rustc errors. cmake
   errors stay cmake errors. `nros` errors only when `nros` owns the
   action. No colcon-style `Failed <<<` aggregation.
9. **No colcon as primary orchestrator.** Colcon stays AVAILABLE for
   Autoware-style outer integration via two-graph seam at `nros plan`.

**Irreducible per-Component-pkg user-authored items (Rust):**
- `Cargo.toml.[package].name` — pkg dir name
- `Cargo.toml.[lib]` + `crate-type = ["rlib", "staticlib"]`
- `Cargo.toml.[package.metadata.nros.component].class = "<pkg>::<UserClass>"`
- `Cargo.toml.[package.metadata.ament].{build_depend, exec_depend}` (non-cargo ROS deps)
- `package.xml` (colcon parity)
- `src/lib.rs` — `impl Component for UserClass` + `nros::component!(UserClass);`
- `launch/*.launch.xml` — OPTIONAL (synth fallback)

**Irreducible per-Application-pkg user-authored items (Rust):**
- `Cargo.toml.[package].name`
- `Cargo.toml.[[bin]] name = "<pkg>"`
- `Cargo.toml.[package.metadata.nros.application].deploy = ["native", …]`
  (must NOT include RTOS targets)
- `package.xml`
- `src/main.rs` w/ `nros::init()` OR `nros::init_with_launch_auto()`
- `launch/*.launch.xml` — OPTIONAL

C++ analogues use cmake fns + identical `package.xml` + `src/*.{cpp,hpp}`.

Everything else derives from these or becomes a build artifact.

## Work Items

### 212.A — `cargo-nros` binary shell (RETRACTED)

**Retracted 2026-06-02.** The original motivation was the cargo
subcommand convention: `cargo nros <verb>` for users already in a
Rust workspace. After A.1–A.3 landed and Wave 5's
`phase212_a_cargo_nros.rs` test confirmed byte-identical output to
the bare `nros <verb>`, a survey showed the cargo prefix added no
functional value:

* Path A bringup dirwalk (212.F.3) landed in `nros plan` directly;
  `cargo nros plan` was a thin re-dispatch with no extra discovery
  affordance.
* `[workspace.metadata.nros]` resolution (212.B) reads cargo metadata
  inside `nros plan` already — the cargo-subcommand shell did not
  add a workspace-root resolution step.
* No canonical user flow (example justfiles, integration tests,
  design-doc walkthroughs) reached for `cargo nros`; every flow used
  bare `nros <verb>`.
* C/C++ users (cmake / west / idf.py / make) never reach for a
  cargo prefix anyway.

Cost (≈8 MB binary, double help-text drift risk, double install
surface) outweighed the residual idiom signal.

Dropped:
- `nros-cli/packages/cargo-nros/` crate (deleted from workspace).
- `~/.nros/bin/cargo-nros` (must NOT be re-installed).
- `phase212_a_cargo_nros.rs` test.

Replaced by:
- A `cargo_nros_binary_absent` regression guard in
  `phase212_non_goals_grep.rs` so the binary stays out of
  `~/.nros/bin/`.
- Design docs + Phase 212.F.3 test surface rewritten to use bare
  `nros plan`.

### 212.B — `[workspace.metadata.nros]` schema + loader

Workspace-root `Cargo.toml` carries `[workspace.metadata.nros]` w/
`default_system = "<bringup-pkg-name>"` pointer (only). Per-component
`Cargo.toml` carries `[package.metadata.nros.component]` w/ overrides.
Per-system `system.toml` (in bringup pkg) carries everything else.

- [ ] **B.1** — Schema definition in `nros-cli-core::orchestration::schema`.
      Strict `deny_unknown_fields`. No second TOML dialect — vocabulary
      stays a strict subset of existing `nros-sdk-index.toml` /
      `app_config.h` field names.
- [ ] **B.2** — `NrosConfig::from_cargo_metadata(workspace_root: &Path)`
      reader via the `cargo_metadata` crate. Replaces today's
      `nros.toml` reader. No fallback. Pre-212 fixtures get migrated to
      the new shape (see 212.I).
- [ ] **B.3** — Per-component `[package.metadata.nros.component]` reader.
      Reads via `cargo metadata --no-deps` on each workspace member,
      walks `packages[*].metadata["nros"]["component"]`. Multi-component
      packages use `[package.metadata.nros.components.<Name>]`
      table-of-tables.
- [ ] **B.4** — `[package.metadata.ament]` reader for `nros emit
      package-xml` (see 212.G).
- **Tests:**
  - [ ] `loads_workspace_metadata_from_cargo_toml` — golden fixture
        round-trips through `NrosConfig::from_cargo_metadata`.
  - [ ] `single_component_package_loads_via_package_metadata` —
        per-component `[package.metadata.nros.component]` table parsed.
  - [ ] `multi_component_package_loads_table_of_tables` — `nros/Talker`
        + `nros/Listener` siblings in one crate.
  - [ ] `rejects_unknown_field_in_strict_mode` — `deny_unknown_fields`
        catches typos.
  - [ ] `nros_toml_file_in_workspace_root_is_rejected` — clean error
        pointing at the migration tool (212.I). No silent fallback.
- **Files:**
  `nros-cli/packages/nros-cli-core/src/orchestration/{config,schema,workspace}.rs`.

### 212.C — `nros-build` build-dependency crate (RETRACTED)

**Retracted.** The original goal was to make `cargo build` invoke
codegen automatically via a `build.rs` helper crate. C.1–C.7 landed
(commits `fae0522ba`, `4d34303db` C.6+C.7 gates) and the Phase 212.K
Option B Cyclone descriptor pipeline initially wired through it.

The retraction came from a design-direction shift: `nros` CLI scope =
codegen + env setup; vendor tool scope = build. Entangling cargo's
build.rs state with nros's codegen step (manifest discovery, idlc
shelling, stamp invalidation, missing-CLI hard-fail) created cross-tool
state that made `cargo check` failures hard to diagnose. The two-step
`nros generate-rust && cargo build` is honest about who owns what.

Dropped:
- `packages/nros-build/` crate (deleted)
- `phase212_h8_loc_budgets.rs::nros_build_under_budget_loc` gate
- `nros-build` build-dep entry from every consumer

Replaced by:
- `nros generate-rust` as the canonical explicit codegen step (Rust)
- `nano_ros_generate_interfaces()` cmake fn as the cmake configure-time
  codegen step (C++)
- Cyclone descriptor codegen migrated to inside `nros generate-rust`
  emit pipeline (Option B, commit `5c6aeab`); generated crates carry a
  thin self-contained `build.rs` that cc-compiles the emitted .c files
  without any `nros-build` dependency.

### 212.D — cmake-side mirror: `nano_ros_workspace_metadata()`

C/C++ users get the same uniform shape Rust users get via cargo metadata.

- [ ] **D.1** — cmake function `nano_ros_workspace_metadata(SYSTEM <bringup-pkg>
      [WORKSPACE_ROOT <dir>])` in `cmake/nano_ros_workspace_metadata.cmake`.
      ≤150 LoC HARD cap.
- [ ] **D.2** — Function shells `nros plan` at cmake configure time
      with the bringup pkg path; emits `${CMAKE_BINARY_DIR}/nros_components.cmake`;
      `include()`s it so component targets are visible to cmake natively.
- [ ] **D.3** — Cross-language interop: `corrosion_import_crate()`
      already supported for Rust components; the function exposes both
      C++ and Rust component targets uniformly. The plan stage decides
      which language each component is in.
- [ ] **D.4** — Documented user incantation: top-level `CMakeLists.txt`
      has `add_subdirectory(<nano-ros-repo>)` then
      `nano_ros_workspace_metadata(SYSTEM demo_bringup)` then
      `add_subdirectory(talker_pkg)` / `corrosion_import_crate(…)`.
- **Tests:**
  - [ ] `cmake_workspace_metadata_emits_components_cmake` — fixture
        cmake project that calls `nano_ros_workspace_metadata` produces
        the expected `nros_components.cmake` import.
  - [ ] `cmake_pure_cpp_multi_component_builds` — fixture w/ 2 C++
        components in 2 sibling pkgs + bringup pkg goes through
        `cmake --build` to a runnable entry binary.
  - [ ] `cmake_mixed_corrosion_bridge_builds` — fixture w/ 1 Rust + 1
        C++ component compiles end-to-end through cmake-top corrosion
        bridge.
- **Files:** `cmake/nano_ros_workspace_metadata.cmake`,
  `packages/testing/nros-tests/fixtures/multi_pkg_workspace_cpp/`,
  `packages/testing/nros-tests/fixtures/multi_pkg_workspace_mixed/`.

### 212.E — `nros codegen system` host-time bake

Single host-time verb that reads `system.toml` + `launch/*.xml` and emits
the baked compile-time C config used by every embedded RTOS adapter
(replaces today's per-example `app_config.h` baker).

- [ ] **E.1** — `nros codegen system --workspace <ws> --bringup <bringup-pkg>
      --target <triple> --out <build-dir>` subcommand. Reads
      `<bringup>/system.toml` + `<bringup>/launch/system.launch.xml`.
- [ ] **E.2** — Emits per-target tree under `<build-dir>/nros-system/`:
      `system_config.h` (domain, rmw, locator, qos), `system_main.c`
      (component registration glue), `Cargo.toml` workspace stub (if
      Rust target), `nros-plan.json` (the resolved plan).
- [ ] **E.3** — Hookless-vendor mode (`--ahead-of-vendor`) for
      PlatformIO + PX4: runs before the vendor tool sees the source
      tree, emits vendor-native artifacts (PIO `library.json` augment,
      PX4 module dirs) the vendor tool then consumes.
- **Tests:**
  - [ ] `codegen_system_emits_baked_headers_for_zephyr_native_sim` —
        fixture bringup → baked tree → linked into a Zephyr
        `native_sim/native/64` ELF.
  - [ ] `codegen_system_emits_baked_headers_for_freertos_qemu` —
        fixture bringup → baked tree → linked into a freertos
        thumbv7m-none-eabi staticlib.
  - [ ] `codegen_system_ahead_of_vendor_emits_pio_library_json` —
        hookless mode writes the expected PIO artifacts.
  - [ ] `codegen_system_idempotent_on_unchanged_input` — re-running
        with identical input produces byte-identical output.
- **Files:**
  `nros-cli/packages/nros-cli-core/src/cmd/codegen_system.rs`,
  fixture pairs under `packages/testing/nros-tests/fixtures/codegen_system_*`.

### 212.F — `<system>_bringup` package shape

Bringup pkg is pure declarative — Path A from the live design doc (no
`Cargo.toml`, excluded from workspace members).

- [ ] **F.1** — `nros new system <name>_bringup --components <list>`
      scaffolds the package with `package.xml`, `system.toml` skeleton,
      `launch/system.launch.xml` skeleton, `.gitignore`. Optionally
      `config/` sub-dir.
- [ ] **F.2** — `nros check` lint rejects bringup pkgs that contain
      `Cargo.toml`, `CMakeLists.txt`, `[[bin]]`, `add_executable`, or
      `src/`. Code does not belong in the bringup pkg.
- [ ] **F.3** — `nros plan <dir>` discovers bringup pkgs by
      dir-walk (sibling to workspace members; excluded from
      `[workspace] members`). The discovery walk is documented + tested.
- [ ] **F.4** — `system.toml` schema documented (see design doc §4).
      `[system]` + `[[component]]` + `[deploy.<target>]` + `[[domain]]` +
      `[[bridge]]` + optional `[[remap]]`.
- **Tests:**
  - [ ] `nros_new_system_scaffolds_bringup_pkg` — invocation produces
        the expected file tree.
  - [ ] `nros_check_rejects_cargo_toml_in_bringup` — lint diagnostic.
  - [ ] `cargo_nros_plan_discovers_bringup_via_dirwalk` — discovery
        walks outside `[workspace] members`.
  - [ ] `bringup_pkg_excluded_from_cargo_workspace_members` — workspace
        root `Cargo.toml` `exclude` list correctly populated by `nros new`.
- **Files:**
  `nros-cli/packages/nros-cli-core/src/cmd/{new,check}.rs`,
  `packages/testing/nros-tests/fixtures/multi_pkg_workspace_rust/`.

### 212.G — `nros check` cross-validates bringup `<exec_depend>`

**Scope retracted — `nros emit package-xml` verb removed.** Users
hand-write `package.xml`. Auto-regenerating it as a derived view of
`Cargo.toml` / `system.toml` was net friction for a small SSoT win,
and the only payoff (bringup `<exec_depend>` derived 1:1 from
`[[component]].pkg`) is now a check-time lint instead of a
write-time codegen verb.

The render helpers (`render_for_pkg`, `check_drift`) stay as internal
utilities for `nros migrate workspace` only. No CLI surface.

- [x] **G.1** — `nros emit package-xml` REMOVED (was a verb in 212.G.0
      drafts). Render helpers retained as
      `cmd::emit_package_xml::{render_for_pkg, check_drift}` for the
      migration sweep + drift detector below.
- [x] **G.2** — Drift detection moved into `nros check --bringup`:
      compares the bringup's hand-written `package.xml` `<exec_depend>`
      block against `[[component]].pkg` rows in `system.toml`. A
      mismatch (extras or missing) is a hard error with a `details`
      list. Lives in `cmd::bringup::check_exec_depend_drift`.
- **Tests** (`packages/testing/nros-tests/tests/phase212_g_check_exec_depend_drift.rs`):
  - [x] `check_passes_when_exec_depend_matches_components`.
  - [x] `check_rejects_missing_exec_depend` — drift names the missing pkg.
  - [x] `check_rejects_stray_exec_depend` — drift names the stray pkg.
- **Files:**
  `nros-cli/packages/nros-cli-core/src/cmd/bringup.rs`
  (`check_exec_depend_drift` + `parse_exec_depend`),
  `nros-cli/packages/nros-cli-core/src/cmd/emit_package_xml.rs`
  (internal helpers only, doc-block updated to reflect new role).

### 212.H — RTOS adapter audit + alignment

Each `integrations/<rtos>/` shell stays ≤200 LoC, matches the universal
pattern from `docs/design/rtos-integration-pattern.md`, and consumes the
baked tree from 212.E.

- [ ] **H.1 Zephyr** — `zephyr/module.yml` + `zephyr/CMakeLists.txt`
      provides `nros_system_generate()` cmake fn that shells `nros codegen
      system`. Today's `app_config.h` baker (per-example) retires.
- [ ] **H.2 NuttX** — `integrations/nuttx/` provides
      `apps/external/<bringup>/` symlink + `Makefile context::` rule
      that runs `nros codegen system` then `NROS_CARGO_BUILD`.
- [ ] **H.3 FreeRTOS** — per-board crate `freertos-<board>-bsp` runs
      `nros codegen system` in `build.rs`, emits `nros_config_generated.h`.
      No separate `integrations/freertos/` directory needed (cargo path
      IS the adapter).
- [ ] **H.4 ThreadX** — `cmake/platform/nano-ros-threadx.cmake` runs
      `nros codegen system` at cmake configure time + uses Corrosion to
      import Rust component crates. No `integrations/threadx/`.
- [ ] **H.5 ESP-IDF** — `integrations/esp-idf/` ESP-IDF component w/
      `idf_component_register` + `Kconfig.projbuild`; configure-time
      `add_subdirectory(<nano-ros-root>)` triggers `nros codegen system`.
- [ ] **H.6 PlatformIO** — repo-root `library.json` + pre-build
      `extra_script` that invokes `nros codegen system --ahead-of-vendor`.
- [ ] **H.7 PX4** — `integrations/px4/` template that the codegen
      emits one module dir per component into; user runs PX4's
      `make px4_sitl` after `nros plan`.
- [ ] **H.8 LoC audit** — each adapter shim ≤200 LoC verified by
      `tokei` in CI.
- **Tests (one per RTOS, all gated on respective SDK availability):**
  - [ ] `zephyr_native_sim_2_component_bringup_builds_and_publishes`
  - [ ] `nuttx_qemu_arm_2_component_bringup_builds`
  - [ ] `freertos_qemu_mps2_an385_2_component_bringup_builds`
  - [ ] `threadx_linux_2_component_bringup_builds_and_publishes`
  - [ ] `threadx_riscv64_qemu_2_component_bringup_builds`
  - [ ] `esp_idf_esp32c3_2_component_bringup_builds`
  - [ ] `platformio_zephyr_framework_2_component_bringup_builds`
  - [ ] `px4_sitl_2_component_module_builds`
  - [ ] `rtos_adapter_loc_budget_under_200` — `tokei` budget gate.
- **Files:**
  `zephyr/module.yml`, `zephyr/CMakeLists.txt`,
  `integrations/{nuttx,esp-idf,platformio,px4}/`,
  `cmake/platform/nano-ros-threadx.cmake`,
  per-board BSP crates under `packages/boards/`.

### 212.I — Migration tooling (INTERNAL ONLY)

The nano-ros tree is unreleased — no external users to bridge. Migrate
exists purely so the in-tree fixture sweep is mechanical instead of
hand-edit churn. **Hidden from `nros --help`** (clap `hide = true`);
still callable directly via `nros migrate workspace <dir>` for the
sweep + the regression test. Retires entirely once the fixture sweep
lands and the migrate tests are demoted to historical.

- [x] **I.1** — `nros migrate workspace <dir>` walks an existing
      pre-212 workspace and emits the new shape:
      - Reads `nros.toml`, writes `<bringup>/system.toml`.
      - For each component pkg: reads `component_nros.toml` /
        `nros/components/*.toml`, writes `Cargo.toml`
        `[package.metadata.nros.component]` (or `nros/components/<Name>.toml`
        for multi-component).
      - Deletes committed `metadata/*.json` (becomes a build artifact;
        the next `cargo build` regenerates).
      - Regenerates `package.xml` via the internal
        `emit_package_xml::render_for_pkg` helper (no user-facing verb
        any more — see 212.G).
- [x] **I.2** — Tool is idempotent (re-runnable on already-migrated
      trees w/o change) and reversible w/ `--dry-run`.
- [ ] **I.3** — Every fixture under
      `packages/testing/nros-tests/fixtures/orchestration_*` gets
      migrated in a single sweep after 212.B/C/F/G land. No mixed-shape
      transitional state in the tree.
- [x] **I.4** — `nros migrate` carries `#[command(hide = true)]` in
      clap; the verb is callable but never appears in `nros --help` or
      `nros --help`. Phase-doc CI grep checks help-text doesn't
      advertise it.
- **Tests:**
  - [x] `migrate_orchestration_e2e_fixture_round_trip` (nros-cli unit).
  - [x] `migrate_orchestration_composable_fixture_round_trip` (nros-cli unit).
  - [x] `migrate_idempotent_without_force_is_noop`
        (nano-ros integration).
  - [x] `migrate_dry_run_writes_no_files` (nano-ros integration).
  - [x] `migrate_workspace_e2e` (nano-ros integration).
- **Files:**
  `nros-cli/packages/nros-cli-core/src/cmd/migrate.rs`,
  `nano-ros/packages/testing/nros-tests/tests/phase212_i_migrate_workspace.rs`.

### 212.J — `nros launch` host-side launcher

Host-side launcher that reads `<bringup>/launch/system.launch.xml`
without depending on the ament index. Lets the user `nros launch
demo_bringup` instead of `ros2 launch demo_bringup …` when no ament
install exists.

- [ ] **J.1** — `nros launch <bringup-pkg-or-dir>` walks the resolved
      `nros-plan.json` from `nros plan` and spawns each component
      process w/ baked env (NROS_LOCATOR, ROS_DOMAIN_ID, params, remaps).
- [ ] **J.2** — `--target <deploy-target>` selects which `[deploy.*]`
      block to use.
- [ ] **J.3** — `nros launch --foreground` / `--detach` controls
      lifecycle; `Ctrl-C` propagates SIGTERM to children.
- [ ] **J.4** — Documented as the canonical desktop launcher for
      development; `ros2 launch` remains available for ament-installed
      consumers.
- [ ] **J.5** — Determines whether bringup pkg's `package.xml` needs
      `<buildtool_depend>ament_cmake</buildtool_depend>` (the design-doc
      open question). If `nros launch` covers the workflow, the tag is
      omitted.
- **Tests:**
  - [ ] `nros_launch_spawns_components` — fixture bringup spawns 2
        processes; both publish; foreground SIGTERM clean-shuts.
  - [ ] `nros_launch_detach_returns_pid_file` — detach mode produces a
        PID file the user can stop via `nros launch --stop`.
  - [ ] `ros2_launch_still_works_after_ament_install` — verifies the
        non-nros path remains compatible when the user does install via
        a colcon outer.
- **Files:**
  `nros-cli/packages/nros-cli-core/src/cmd/launch.rs`.

### 212.K — Cyclone-Rust pure cargo path

Make `cargo build --features rmw-cyclonedds` work end-to-end without
CMake on hosted targets (native, qemu native_sim).

- [ ] **K.1** — `cyclonedds-sys` crate at
      `packages/dds/cyclonedds-sys/` vendors Cyclone via the `cmake`
      build-script crate against `third-party/dds/cyclonedds` (pinned
      0.10.5). Forces `ENABLE_LTO=OFF`, `BUILD_IDLC=ON`. Separate host
      `idlc` build target. Exports `links = "ddsc"`, `cargo:idlc`,
      `cargo:include`.
- [ ] **K.2** — `nros-rmw-cyclonedds-sys` wrapper crate at
      `packages/dds/nros-rmw-cyclonedds-sys/` runs `cc::Build::cpp(true)`
      over existing `packages/dds/nros-rmw-cyclonedds/src/*.cpp`. Bakes
      `rmw_dds_common_graph` descriptor via bundled host `idlc`. Emits
      `cargo:rustc-link-lib=static:+whole-archive,-bundle=nros_rmw_cyclonedds`
      + `dylib=stdc++`. Risk: HIGH (semi-internal Cyclone headers).
- [ ] **K.3 (PRE-REQ)** — Port `scripts/cyclonedds/msg_to_cyclone_idl.py`
      to Rust as a `nros-msg-to-idl` library + build-dep helper. Python
      build-dep is a regression for the pure-cargo promise.
- [ ] **K.4** — Per-example descriptor codegen: extend `nros codegen`
      with `nros codegen cyclonedds-descriptors`. Emits a small Rust
      crate w/ the idlc C output + register TU; consumed via build-dep.
- [ ] **K.5** — `examples/native/rust/{talker,listener}/Cargo.toml` get
      a `rmw-cyclonedds` feature that pulls in the new sys crates. The
      CMakeLists path for cyclonedds is RETIRED; C++ examples retain
      their CMake path unchanged.
- [ ] **K.6** — Fallback acceptance: if the sys-crate wrapper proves
      too brittle across Cyclone bumps, retain the CMake path for
      cyclonedds AS the canonical Rust+Cyclone build. Don't force a
      Rust-only path against upstream churn.
- **Tests:**
  - [ ] `cyclonedds_sys_builds_native` — `cargo build -p cyclonedds-sys`
        on native_sim succeeds; `libddsc.a` linked.
  - [ ] `nros_rmw_cyclonedds_sys_register_symbol_exported` —
        `nros_rmw_cyclonedds_register` is whole-archive-linked + reachable.
  - [ ] `native_rust_cyclonedds_talker_listener_e2e` — `cargo build
        --features rmw-cyclonedds && <run>` end-to-end exchange w/o
        CMake.
  - [ ] `msg_to_cyclone_idl_rust_port_matches_python_output` — port
        produces byte-identical IDL for every fixture in
        `scripts/cyclonedds/test/`.
- **Files:**
  `packages/dds/{cyclonedds-sys,nros-rmw-cyclonedds-sys}/`,
  `packages/codegen/nros-msg-to-idl/`,
  `examples/native/rust/{talker,listener}/`,
  `nros-cli/packages/nros-cli-core/src/cmd/codegen_cyclonedds.rs`.

### 212.L — Pkg shape + unified launch model

Locks the canonical user-authored shapes (Component / Application /
Bringup) × (Rust / C++) and the single resolution pipeline that
underlies `nros codegen-system`, `nros plan`, `nros launch`, and every
RTOS adapter shim.

- [ ] **L.1 Component pkg shape** — Rust authors `Cargo.toml` (w/ `[lib]
      crate-type=["rlib","staticlib"]` + `[package.metadata.nros.
      component] class = "<pkg>::<UserClass>"`) + `package.xml` +
      `src/lib.rs` (`impl Component for UserClass` + `nros::component!(
      UserClass);`) + optional `launch/*.launch.xml`. C++ authors
      `CMakeLists.txt` (`nano_ros_component_register(NAME … CLASS …
      SOURCES … DEPLOY …)`) + `package.xml` + `src/<UserClass>.{cpp,
      hpp}` (`NROS_COMPONENT_REGISTER(UserClass, "<pkg>::UserClass")`)
      + optional launch. No user `main()` either language — codegen
      synthesises native `main` into `target/nros-system/<pkg>/` (out-
      of-tree) and `system_main.c` for embedded.
- [ ] **L.2 Application pkg shape** — Rust authors `Cargo.toml` (`[[bin]
      ] name = "<pkg>"` + `[package.metadata.nros.application].deploy
      = ["native", …]`) + `package.xml` + `src/main.rs` w/ explicit
      `nros::init()` OR `nros::init_with_launch_auto()` + optional
      launch. C++ analogous (`nano_ros_application(NAME … SOURCES …
      DEPLOY native)`). Application pkgs are NATIVE-ONLY; including any
      RTOS in `deploy` is a `nros check` error.
- [ ] **L.3 Bringup pkg shape (Path A)** — `package.xml` + `system.toml`
      + `launch/system.launch.xml`. NO `Cargo.toml`, NO `CMakeLists.txt`,
      NO `src/`. `system.toml` carries `[deploy.<target>]` +
      `[[domain]]` + `[[bridge]]` + `[[remap]]`. Multi-node composition
      via `<node pkg=…>` / `<include>` in launch.xml. Language-
      independent.
- [ ] **L.4 `<pkg>::<Class>` enforcement** — `nros check` MUST reject a
      component pkg whose `class` field doesn't start with the pkg
      directory name (which equals `Cargo.toml::[package].name` for
      Rust and `project()` for C++). Cross-cuts user docs ("the dir
      name IS the pkg name").
- [ ] **L.5 Init API patterns**:
      - Pattern 1 (Component pkg): `nros::component!(Ty);` — register
        trampoline; codegen owns runtime.
      - Pattern 2 (Application pkg + launch-aware): `nros::
        init_with_launch_auto(argc, argv)` reads `<pkg>/launch/*.xml`
        via the L.6 resolver + applies params / remaps / env. User
        owns spin.
      - Pattern 3 (Application pkg + custom spin): `nros::init(argc,
        argv)` — raw init, launch file ignored. For custom executors,
        gui main, async runtime integration, debug instrumentation.
      C++ counterparts: `nros::init_with_launch_auto/path` /
      `nros::init`.
- [ ] **L.6 Launch file resolution + synthesis** —
      - For Path A bringup pkg: launch file REQUIRED;
        `launch/system.launch.xml` is the canonical name. Missing
        launch → hard error.
      - For Component / Application pkg: launch file OPTIONAL. When
        absent, `nros plan` / `nros codegen-system` synthesise an
        in-memory `<launch><node pkg="<pkg>" exec="<exec>"/></launch>`
        (never written to disk).
      - Multi-launch resolution order: `--file <path>` arg → `<dir>/
        launch/<pkg>.launch.xml` → `<dir>/launch/system.launch.xml` →
        single `<dir>/launch/*.launch.xml` → synth (only for non-Path-A).
      - `--exec <name>` skips exec disambiguation when multiple
        `[[bin]]` / `add_executable` candidates exist.
- [ ] **L.7 `[workspace.metadata.nros]` schema** — single field
      `default_system = "<pkg-name-or-bringup-name>"`. Resolves either
      a Path A bringup pkg (has `system.toml`, no Cargo.toml) OR a
      self-bringup component/application pkg (has Cargo.toml w/
      `[package.metadata.nros.deploy.*]`). Loader handles both.
- [ ] **L.8 `[package.metadata.nros.deploy.<target>]` table (Option
      α)** — per-pkg deploy targets live in `Cargo.toml`
      `[package.metadata.nros.deploy.<target>]` (Rust) OR via
      `nano_ros_deploy(TARGET … RMW … DOMAIN_ID …)` cmake fn (C++).
      `system.toml` deploy table exists ONLY in Path A bringup pkgs.
      `nros check` rejects per-pkg `system.toml` outside bringup role.
- [ ] **L.9 C++ cmake fn surface** — `nano_ros_component_register(NAME
      <name> CLASS <UserClass> SOURCES … DEPLOY …)`,
      `nano_ros_application(NAME <name> SOURCES … DEPLOY …)`,
      `nano_ros_deploy(TARGET <name> RMW <rmw> DOMAIN_ID <n>
      LOCATOR <uri>)`, `nano_ros_bridge(…)`, `nano_ros_domain(…)`.
      All fns write metadata JSON to `${BUILD}/nros-metadata.json` so
      `nros codegen-system` reads it at configure time. No sidecar
      TOML for C++ pkgs.
- [ ] **L.10 `nros::component!()` macro + Component / ExecutableComponent
      traits** — already shipped per Phase 172 W.3 (see
      `packages/core/nros-macros/src/lib.rs:156`). Macro emits the
      register trampoline; `Component` trait declares nodes/pubs/
      subs/timers/services/actions; `ExecutableComponent` adds
      `init()` + `on_callback(state, cb_id, ctx)` + optional `tick(
      state, ctx)` bodies. Generated runtime owns the spin loop.
- [ ] **L.11 `.cargo/config.toml` lint** — `nros check` warns when a
      per-pkg `.cargo/config.toml` carries `[patch.crates-io]`
      entries; patches live exclusively in workspace-root
      `Cargo.toml` (auto-managed by `nros ws sync`). Phase 212.K wave
      11 hit a real shadow bug here.
- [ ] **L.12 Vendor-native platform configs out of scope** — `prj.conf`
      (Zephyr), `sdkconfig` (ESP-IDF), `platformio.ini` (PIO),
      Kconfig fragments (NuttX), linker scripts (FreeRTOS / ThreadX)
      stay vendor-native. nros does NOT replace them. Adapter shims
      shell `nros codegen-system` at configure time; vendor tools own
      the rest.
- **Tests:**
  - [ ] `nros_check_rejects_class_pkg_mismatch` — `class = "wrong::
        Talker"` in a pkg named `talker_pkg` → diagnostic.
  - [ ] `nros_check_rejects_system_toml_outside_bringup` — Path A
        bringup is the only valid `system.toml` location.
  - [ ] `application_pkg_with_rtos_deploy_is_rejected` — `deploy =
        ["zephyr"]` on Application pkg → error.
  - [ ] `launch_synth_emits_single_node_for_self_bringup` — Component
        pkg w/o launch file → synth `<launch><node pkg=… exec=…/>`.
  - [ ] `launch_synth_refuses_path_a_bringup_without_file` — missing
        bringup launch.xml → hard error.
  - [ ] `multi_launch_resolves_pkg_named_default` — `<pkg>/launch/
        <pkg>.launch.xml` wins when no `--file` arg given.
  - [ ] `cargo_config_patch_lint` — per-pkg `.cargo/config.toml` w/
        `[patch.crates-io]` → diagnostic.
- **Files:**
  `cmake/NanoRosComponentRegister.cmake` (NEW — C++ cmake fns),
  `nros-cli/packages/nros-cli-core/src/cmd/check.rs` (L.4 + L.8 + L.11
  lints), `nros-cli/packages/nros-cli-core/src/orchestration/launch_synth.rs`
  (NEW — L.6 synthesis), companion design docs.

### 212.M — Example migration sweep + pre-212 cleanup

Migrate every `examples/<plat>/<lang>/<example>/` to the §212.L
canonical shape, and remove every pre-212 file format from the tree.
A clean break — no transitional mixed-shape state allowed.

- [ ] **M.1 native/rust sweep** — `examples/native/rust/{talker,
      listener}/` → Component pkg (used by embedded fixtures too;
      Option B Cyclone codegen path drives via `nros generate-rust`).
      `examples/native/rust/{service-*,action-*,parameters,logging}/`
      → Application pkg + `nros::init_with_launch_auto()` (demos
      params/remaps from launch). Drop Phase 170.A `lib.rs::run()` +
      `main.rs::main(){run()}` split — Component pkgs become lib-only.
- [ ] **M.2 native/cpp sweep** — `examples/native/cpp/*` (~6 examples)
      → Application pkg with `nano_ros_application()`. Replace
      `find_package(std_msgs) via NrosRclcppCompat` with
      `nano_ros_generate_interfaces(LANGUAGE CPP PACKAGES …)`. Lose
      the explicit `NANO_ROS_PLATFORM=posix` / `nros_platform_link_app`
      calls (cmake fns subsume them).
- [ ] **M.3 Zephyr sweep** — `examples/zephyr/{c,cpp,rust}/*` →
      Component pkgs. Drop per-example `<nros/app_config.h>` Kconfig-
      synthesis (`packages/core/nros-c/include/nros/zephyr/
      app_config.h` retired). Wire via `nros_system_generate(.)` (the
      §212.L self-pkg case — `nros codegen-system --pkg <dir>` reads
      Cargo metadata for single-pkg defaults). RMW selection via
      `CONF_FILE=prj.conf;prj-<rmw>.conf` overlays stays vendor-native.
- [ ] **M.4 NuttX sweep** — `examples/qemu-arm-nuttx/*` +
      `examples/nuttx/{c,cpp,rust}/*` (~5 examples). Drop per-example
      `nros.toml` + `gen-app-config.py` baker. Route through
      `nros codegen-system --pkg <dir>` via the H.2 adapter shim.
- [ ] **M.5 FreeRTOS sweep** — `examples/qemu-arm-freertos/{rust,
      cpp}/*` + `examples/freertos/*`. Drop `nano_ros_read_config(
      nros.toml)` cmake fn. Per-board BSP crate (H.3) handles codegen
      via its own `build.rs` shelling `nros codegen-system`. Expand
      BSP crates beyond `freertos-qemu-mps2-an385-bsp` for any board
      that gets an example.
- [ ] **M.6 ThreadX sweep** — `examples/threadx-linux/{rust,cpp}/*`
      + `examples/threadx-riscv64/{rust,cpp}/*`. Mostly close already;
      mechanical conversion to Component pkg shape + `nros_threadx_
      codegen_system()` cmake fn (already H.4-shipped).
- [ ] **M.7 ESP-IDF / ESP32 sweep (BLOCKED)** — `examples/esp32/{
      rust,c,cpp}/*`. Currently sidesteps ESP-IDF (plain `cargo build`
      under `platform-bare-metal`). Migration = move under ESP-IDF
      `idf.py` workflow via `integrations/nano-ros` ESP-IDF component
      (H.5 carve-out shipped). **BLOCKED** on H.5 deeper gap: nros-
      node `executor/spin.rs` uses `alloc::sync::Arc` directly on
      `target_has_atomic = "ptr"`-gated branches; esp32c3's `riscv32imc`
      lacks ptr atomics. Fix path: swap to `portable_atomic_util::Arc`.
      Unblocks M.7.
- [ ] **M.8 PlatformIO sweep** — `examples/platformio/*` (when any
      land). H.6 extra_script handles framework-agnostic codegen via
      `nros codegen-system --ahead-of-vendor --framework <f>`.
- [ ] **M.9 PX4 sweep** — `examples/px4/cpp/uorb/nros-register-check/`
      stays as-is (the canonical PX4 surface per Phase 115.K.4).
      Multi-node PX4 case (H.7-shipped emit) operates on bringup pkgs
      writing into `$PX4_AUTOPILOT_DIR/src/modules/`.
- [ ] **M.10 Pre-212 file cleanup** — enumerate + delete every
      pre-212 file the migration sweep makes redundant:
      - `nros.toml` (any location)
      - `component_nros.toml` per-pkg
      - `gen-app-config.py` per-example baker
      - `app_config.h.in` / per-example `<nros/app_config.h>`
        Kconfig-synthesis
      - `nano_ros_read_config(nros.toml)` cmake fn (delete the fn
        + every caller)
      - Per-example committed `metadata/*.json`
      - Phase 170.A `lib.rs::run()` + `main.rs::main(){run()}`
        split files in `examples/native/rust/*` (Component pkg = lib
        only; codegen synthesises main)
      - Legacy `examples/native/rust/{talker,listener}/CMakeLists.
        txt` (Phase 175.A Cyclone CMake fallback — superseded by
        Option B pure-cargo path)
      - Stale `examples/native/rust/{talker,listener}/generated/`
        dirs from pre-Option-B codegen runs
- [ ] **M.11 `nros check` lints (defensive)** — after the sweep, add
      lints so no contributor reintroduces a pre-212 shape:
      - L.4 (`<pkg>::<Class>` mismatch)
      - L.8 (per-pkg `system.toml` outside bringup)
      - L.11 (`.cargo/config.toml` `[patch.crates-io]`)
      - `pre-212 files forbidden`: grep over the example dir for
        `nros.toml`, `component_nros.toml`, `gen-app-config.py`,
        `app_config.h.in`, committed `metadata/*.json` → hard error.
- [ ] **M.12 Regression test** —
      `packages/testing/nros-tests/tests/phase212_examples_canonical_
      shape.rs`. Walks `examples/` + asserts:
      - Every example dir has a `package.xml`.
      - Component / Application pkg classification matches
        `[package.metadata.nros.{component,application}]` table OR
        cmake fn call.
      - No pre-212 file shapes survive (M.10 list).
      - Path A bringup dirs have no Cargo.toml / CMakeLists.txt / src/.
      - All deploy targets in `[package.metadata.nros.deploy.*]`
        match the platform path the example lives under.
- **Tests** (per-wave, gated on SDK availability):
  - [ ] `native_rust_talker_listener_e2e_<rmw>` per RMW
  - [ ] `native_cpp_talker_listener_e2e_<rmw>` per RMW
  - [ ] `zephyr_<example>_builds` per migrated Zephyr example
  - [ ] Same for nuttx / freertos / threadx / platformio / px4
  - [ ] `pre_212_files_forbidden_in_examples` (M.12)
  - [ ] `class_pkg_match_enforced_in_examples` (M.11 + M.12)
- **Files:** `examples/` (tree-wide sweep), `packages/core/nros-c/
  include/nros/zephyr/app_config.h` (DELETE), `cmake/NanoRosReadConfig.
  cmake` (DELETE if exists), `nros-cli/packages/nros-cli-core/src/
  cmd/check.rs` (M.11 lints), regression test under
  `packages/testing/nros-tests/tests/`.

## Acceptance

Two-step Rust (codegen + build) is the canonical user surface;
one-step C++ (cmake configure runs codegen as a side effect of the
cmake fn) is the canonical C++ user surface. See §Goal for the
asymmetry rationale.

- [ ] **Single-node Rust = `nros generate-rust && cargo build && cargo
      run` for ALL three RMWs** (zenoh, xrce, cyclonedds). No CMake step
      required. (212.K Option B)
- [ ] **Single-node C++ = `cmake -B build && cmake --build build`.**
      RMW selected via `-DNANO_ROS_RMW=…`. `nano_ros_generate_interfaces()`
      runs codegen at configure. (existing path; cmake-side codegen)
- [ ] **Multi-node Rust = `nros generate-rust && cargo build && nros
      plan && nros launch <bringup>`** — explicit codegen step + cargo
      builds + nros owns plan + launch. (212.B + 212.J + 212.L Bringup)
- [ ] **Multi-node C++ = `cmake -B build && cmake --build build && nros
      launch <bringup>`** — `nano_ros_workspace_metadata()` does the
      plan stage at configure time. (212.D + 212.J)
- [ ] **Mixed Rust+C++ workspace = `cmake -B build && cmake --build
      build`** with `corrosion_import_crate` bridging Rust components
      into cmake's superbuild. (212.D + cross-language acceptance)
- [ ] **Three pkg shapes work for both langs** — Component pkg
      (lib only), Application pkg (user main), Bringup pkg (Path A
      code-free). (212.L)
- [ ] **Per-pkg metadata in vendor manifest** — Rust uses Cargo.toml
      `[package.metadata.nros.{component,application,deploy.<target>,
      embedded}]`; C++ uses cmake fns (`nano_ros_component_register`,
      `nano_ros_application`, `nano_ros_deploy`). No sidecar TOML for
      C++. `system.toml` lives ONLY in Path A bringup pkgs. (212.L)
- [ ] **Component class follows `<pkg>::<UserClass>`** — pkg dir name
      MUST match the prefix. `nros check` enforces. (212.L.4)
- [ ] **Launch file synthesis works for single-pkg** — Component pkg
      w/o launch file gets an implicit one synthesised in-memory by
      `nros plan` / `nros codegen-system` / `nros launch`. (212.L.6)
- [ ] **Multi-launch resolution works** — `<pkg>/launch/<pkg>.launch.
      xml` > `<pkg>/launch/system.launch.xml` > single file > synth.
      `--file <path>` override. (212.L.6)
- [ ] **Every existing fixture migrated to the new shape** via the
      §212.I.3 sweep (fixtures) + §212.M sweep (examples). No mixed-
      shape tree allowed. (212.I + 212.M)
- [ ] **All 7 RTOS adapters ship a working bringup fixture under the
      new shape** (Zephyr, NuttX, FreeRTOS, ThreadX, ESP-IDF, PlatformIO,
      PX4). (212.H + 212.M)
- [ ] **Each adapter shim ≤200 LoC; cmake `nano_ros_workspace_metadata
      ()` ≤150 LoC.** CI gate via `tokei`. (`nros-build` budget bullet
      retired with 212.C.)
- [ ] **No `nros build` / `nros test` / `nros flash` / `nros monitor`
      / `nros sign` / `nros emit` verbs.** Phase-doc grep checked in CI
      via `phase212_non_goals_grep.rs`. (Non-Goals)
- [ ] **A failing rustc / cmake / clang diagnostic in any test fixture
      reaches the user's terminal verbatim** — no aggregation, no
      truncation. CI test injects a synthetic compile error and greps for
      the original message.
- [ ] **Pre-212 files forbidden in the tree** — `nros.toml`,
      `component_nros.toml`, `gen-app-config.py`, `app_config.h.in`
      per-example bakers, committed `metadata/*.json`. Regression test
      grep-asserts. (212.M.10 + M.11)

## Test infrastructure

- [ ] Fixture directory restructure under
      `packages/testing/nros-tests/fixtures/`:
  - `single_pkg_rust/` (per RMW × {zenoh, xrce, cyclonedds})
  - `single_pkg_cpp/` (per RMW)
  - `multi_pkg_workspace_rust/` (canonical 2-component bringup)
  - `multi_pkg_workspace_cpp/`
  - `multi_pkg_workspace_mixed/`
  - `codegen_system_{zephyr,nuttx,freertos,threadx,esp_idf,platformio,px4}/`
- [ ] Every fixture has a corresponding integration test under
      `packages/testing/nros-tests/tests/phase212_*.rs`.
- [ ] CI matrix gates: SDK-available rows run, unavailable rows skip
      cleanly (mirrors existing `require_*` helpers).
- [ ] `tokei` budget tests for every glue piece in the §Acceptance
      LoC table.
- [ ] `nros migrate workspace` golden-fixture tests for every pre-212
      fixture shape.

## Execution order

S = small (≤1d), M = medium (1–3d), L = large (≥1w).

1. **212.A `cargo-nros` binary shell** — RETRACTED, see §212.A
2. **212.B schema + loader** (M) — partial; final shape locked in §212.L.8
3. **212.C `nros-build` crate** — RETRACTED, see §212.C
4. **212.D `nano_ros_workspace_metadata()` cmake fn** (M) — shipped
5. **212.E `nros codegen-system`** (M) — shipped; launch synthesis per §212.L.6 still to land
6. **212.F bringup pkg shape + `nros new system`** (S) — shipped
7. **212.G `nros check` exec-depend drift detector** (S) — shipped; emit verb retracted
8. **212.H RTOS adapter audit + 7 fixtures** (L) — H.1-H.7 shipped; M.7 ESP-IDF cross-compile blocked
9. **212.I migration tooling (INTERNAL, hidden CLI)** (M) — shipped + I.3 fixture sweep done
10. **212.J `nros launch`** (M) — shipped
11. **212.K cyclonedds-sys + wrapper** (L) — shipped + K.4 Option B (codegen-driven descriptors) shipped
12. **212.L Pkg shape + unified launch model** (L) — NEW; lock canonical shapes + lints + launch synth
13. **212.M Example migration sweep + pre-212 cleanup** (L) — NEW; tree-wide sweep + lint enforcement
14. **Acceptance verification + CI gates** (M)

## Non-Goals

The following user surfaces are explicitly rejected. Adding any of them
re-creates the colcon-shaped anti-pattern (orchestrator owns stdout,
swallows root-cause errors, drags every contributor into a parallel
build system to learn):

- `nros build`
- `nros test`
- `nros flash`
- `nros monitor`
- `nros sign`
- `nros emit package-xml` (retracted in 212.G — users hand-write
  `package.xml`; the bringup `<exec_depend>` drift is caught by
  `nros check --bringup`, not auto-regenerated)
- `cargo-nros` cargo subcommand shell (retracted in 212.A — every
  `nros <verb>` works directly; cargo prefix added no functional value)
- `nros-build` Rust build-dependency crate (retracted in 212.C —
  `nros generate-rust` is the explicit codegen step; cargo owns build)
- A workspace-root `nros.toml` (per-pkg metadata lives in
  `Cargo.toml` `[package.metadata.nros.*]` / cmake fns; bringup uses
  `<bringup>/system.toml`)
- Per-component `component_nros.toml` (`[package.metadata.nros.component]`
  in `Cargo.toml` is the replacement)
- Per-pkg `system.toml` OUTSIDE bringup role (deploy data lives in
  `Cargo.toml` `[package.metadata.nros.deploy.<target>]` / cmake fns;
  see §212.L.8)
- Sidecar `nros-pkg.toml` for C++ pkgs (cmake fns are the C++ metadata
  surface; see §212.L.9)
- Committed `metadata/*.json` (build artifact only, lives in
  `$OUT_DIR/nros-gen/` or `target/nros-metadata/`)
- Per-example `gen-app-config.py` / `app_config.h.in` / per-target
  `<nros/app_config.h>` Kconfig-synthesis (retired by §212.M sweep;
  embedded codegen routes through `nros codegen-system`)
- Per-pkg `.cargo/config.toml` `[patch.crates-io]` block (auto-managed
  in workspace-root `Cargo.toml`; see §212.L.11)

Phase-doc CI grep checks that none of these appears in user-facing
surface area (CLI help text, fixture trees, book docs).

## Notes

- Phase 212 is a CLEAN BREAK from the pre-212 shape. No fallback
  loaders, no transitional state in the tree. The migration tool
  (212.I) is the only bridge — and only because the in-tree fixture
  sweep needs it. Hidden from the public CLI; retires entirely once
  212.I.3 ships and the regression test is demoted to historical.
- The codegen-vs-build split is asymmetric for Rust by design. C++
  cmake fns make codegen implicit at configure; cargo has no
  equivalent integration point without a build-dep crate (the
  retracted §212.C path). Two-step Rust is the honest answer; see
  §Goal.
- Three pkg shapes (Component / Application / Bringup) replace the
  pre-212 "every pkg has a Cargo.toml + nros.toml" model. The mental
  cost of picking shape pays off in: lib-only components compose into
  multi-component bringups + ship to RTOS; explicit-spin applications
  keep rclcpp/rclpy-style control on native; Path A bringups stay
  code-free orchestration declarations. See §212.L for the full
  taxonomy.
- Live design documents in `docs/design/` continue to iterate after the
  phase lands. Treat the phase doc as the work breakdown; treat the
  design docs as the source of truth on shape decisions.
- The `nros launch` work item (212.J) is one of two paths to resolve
  the "does the bringup pkg need `ament_cmake`?" design-doc open
  question. The other is to always require a colcon outer install
  before `ros2 launch`. Phase 212 commits to 212.J as the canonical
  path; colcon outer integration becomes an opt-in alternative.
- 212.K (Cyclone-Rust pure cargo) shipped via Option B: descriptor
  codegen lives inside `nros generate-rust`'s emit pipeline, not a
  separate per-example build.rs. The retired §212.C `nros-build` path
  is NOT the canonical user surface for any RMW.
- §212.M is the only work item that mutates `examples/` tree-wide.
  Once it lands, the §212.L canonical shape is the only shape in
  the tree; §212.M.11 lints prevent re-introduction.
- Companion design docs (kept up-to-date as work proceeds):
  - `docs/design/multi-node-workspace-layout.md` (LIVE)
  - `docs/design/workspace-layout-by-case.md` (LIVE)
  - `docs/design/rtos-integration-pattern.md` (LIVE)
