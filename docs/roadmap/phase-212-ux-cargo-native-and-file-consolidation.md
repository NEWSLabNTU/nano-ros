# Phase 212 — Build-system-native UX + workspace layout

**Status:** OPEN
**Priority:** P1
**Depends on:** Phase 211 (orchestration foundation)
**Supersedes / breaks:** every `component_nros.toml`, every committed
`metadata/*.json`, every root `nros.toml`, the existing `nros build` /
`nros generate-rust` user surface, every Phase 211 fixture's workspace shape.
**No backward compatibility.** Clean break.

## Goal

Make nano-ros's developer surface idf.py-shaped: vendor build tool stays
user-facing (cargo for Rust, cmake for C/C++, vendor SDK for embedded);
`nros` is a provisioner + codegen + metadata + deploy back-end, never a
build verb. Component packages declare themselves in their own native
manifest (`Cargo.toml` or `CMakeLists.txt`); multi-node systems live in a
dedicated `<system>_bringup` package following standard ROS layout.

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
2. **Single-node** = one package, one `Cargo.toml` (or one
   `CMakeLists.txt`). No bringup pkg. `cargo build` / `cmake --build`
   does codegen automatically via `nros-build` (Rust) /
   `nano_ros_generate_interfaces()` (C/C++).
3. **Multi-node** = cargo workspace (or cmake superbuild) +
   `<system>_bringup` package. Bringup pkg is pure declarative — no
   `Cargo.toml`, no `CMakeLists.txt`, no `src/`. Contains `package.xml`
   + `system.toml` + `launch/system.launch.xml` + optional `config/`.
4. **Mixed Rust + C/C++** = cmake top-level via Corrosion bridge. Pure
   Rust = cargo top-level. Pure C/C++ = cmake top-level.
5. **Embedded RTOS** = vendor SDK retains its native build tool (west /
   make+Kconfig / cmake / idf.py / pio). nano-ros plugs into vendor's
   external-module hook + bakes `system.toml` into compile-time C config.
   Bringup pkg never reaches device.
6. **Diagnostics passthrough.** Rustc errors stay rustc errors. cmake
   errors stay cmake errors. `nros` errors only when `nros` owns the
   action. No colcon-style `Failed <<<` aggregation.
7. **No colcon as primary orchestrator.** Colcon stays AVAILABLE for
   Autoware-style outer integration via two-graph seam at `nros plan`.

**Five irreducible per-component user-authored items:**
- `Cargo.toml.[package].name` (cargo requires)
- `Cargo.toml.[package.metadata.nros.component].{default_namespace, parameters, remaps}` (pure deployment intent)
- `Cargo.toml.[package.metadata.ament].{build_depend, exec_depend}` (non-cargo ROS deps)
- `src/lib.rs` w/ `#[nros::component]` attribute macros
- Per-RTOS: nothing extra (adapter shim handles it)

Everything else derives from these or becomes a build artifact.

## Work Items

### 212.A — `cargo-nros` binary shell

Cargo subcommand convention: cargo auto-discovers `cargo-<verb>` binaries
on PATH. A `cargo-nros` binary makes every nros verb invokable as
`cargo nros <verb>` for Rust workspaces.

- [ ] **A.1** — Binary crate `packages/cargo-nros/` in `nros-cli` repo
      (NOT this tree). ≤100 LoC. Clap dispatcher strips cargo-injected
      `nros` argv[1] and re-exports `nros_cli_core::cmd::Cmd::run`.
- [ ] **A.2** — Every verb supports `--explain` decomposing to the
      underlying `nros` invocation. Mandatory for every verb.
- [ ] **A.3** — `scripts/install-nros.sh` installs both `nros` and
      `cargo-nros` to `~/.nros/bin/` from the same release artifact.
- **Tests:**
  - [ ] `cargo_nros_plan_dispatches_to_nros_plan` — `cargo nros plan demo`
        produces the same `nros-plan.json` as `nros plan demo`.
  - [ ] `cargo_nros_explain_decomposes` — `cargo nros plan --explain`
        prints the underlying `nros plan …` invocation.
- **Files:** `nros-cli/packages/cargo-nros/{Cargo.toml,src/main.rs}`,
  `nano-ros/scripts/install-nros.sh`.

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

### 212.C — `nros-build` build-dependency crate

Rust build-script helper that runs `nros codegen` from `build.rs`
automatically. Replaces manual `nros generate-rust` step.

- [ ] **C.1** — Crate at `packages/nros-build/` (this tree). ≤500 LoC
      HARD cap. `Codegen` builder pattern.
- [ ] **C.2** — Resolves `nros` binary via `$NROS_BIN` → PATH →
      `~/.nros/bin/nros` (mirrors `scripts/build/cargo.sh::nros_cli_bin`).
- [ ] **C.3** — Writes outputs to `$OUT_DIR/nros-gen/` ONLY (preserves
      `--target-dir` isolation rule from CLAUDE.md). Never touches
      `target/` directly.
- [ ] **C.4** — Emits `cargo:rerun-if-changed=` for `package.xml`,
      every `.msg` / `.srv` / `.action` file, and interface-package
      roots discovered via `[package.metadata.ament].build_depend`.
- [ ] **C.5** — SHA-256 input digest stamp at `$OUT_DIR/nros-gen/.stamp`
      for incremental skip. Reuse the Phase 195 `cache` module from
      `cargo_nano_ros`.
- [ ] **C.6** — Degrades to no-op (warn-only) on `cargo check
      --no-default-features` when no RMW feature selected. Same hazard
      as Phase 118.B probe.
- [ ] **C.7** — Missing `nros` binary → hard fail with install pointer.
- **Tests:**
  - [ ] `build_rs_invokes_nros_codegen` — golden trybuild fixture
        `Cargo.toml` + `build.rs` + `src/lib.rs` produces expected
        `$OUT_DIR/nros-gen/` tree.
  - [ ] `stamp_skips_when_inputs_unchanged` — second `cargo build`
        without input changes does not re-invoke `nros codegen`.
  - [ ] `rerun_if_msg_changes` — touching a `.msg` triggers a rebuild.
  - [ ] `nocodegen_no_default_features` — `cargo check --no-default-features`
        on a crate w/ no RMW feature degrades to no-op + warning.
  - [ ] `missing_nros_binary_hard_fails` — clean error message.
  - [ ] `loc_budget_under_500` — script-level test asserts `tokei`
        on `src/` reports ≤500 LoC.
- **Files:** `packages/nros-build/{Cargo.toml,src/lib.rs,tests/}`.

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
- [ ] **F.3** — `cargo nros plan <dir>` discovers bringup pkgs by
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

### 212.G — `nros emit package-xml`

Auto-generate `package.xml` from `Cargo.toml` `[package.metadata.ament]`
for Rust pkgs. For bringup pkgs, regenerate `<exec_depend>` block from
`system.toml.[system].components`. Keep `package.xml` in tree (ament/colcon
interop) but eliminate hand-maintenance.

- [ ] **G.1** — `nros emit package-xml <pkg-dir>` reads either
      `Cargo.toml.[package.metadata.ament]` (component pkg) or
      `system.toml.[system].components` + idiom (bringup pkg) and emits
      `package.xml`.
- [ ] **G.2** — Hand-written `package.xml` carries an `<!-- generated by
      nros emit package-xml -->` header; subsequent runs regenerate it
      idempotently. `nros check` warns on hand-edit drift.
- [ ] **G.3** — `nros check` cross-validates `<exec_depend>` in bringup
      pkg ↔ `[system].components`.
- **Tests:**
  - [ ] `emit_package_xml_from_cargo_ament_metadata` — golden fixture.
  - [ ] `emit_package_xml_from_system_toml_for_bringup` — bringup pkg
        gets correct `<exec_depend>` block.
  - [ ] `nros_check_warns_on_hand_edit_drift` — modifying generated
        `package.xml` between runs surfaces a diagnostic.
  - [ ] `idempotent_round_trip` — running `emit` twice produces
        byte-identical output.
- **Files:**
  `nros-cli/packages/nros-cli-core/src/cmd/emit_package_xml.rs`.

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

### 212.I — Migration tooling

Existing fixtures + any external users need to migrate from `nros.toml`
+ `component_nros.toml` + committed `metadata/*.json` to the new shape.
No backward compat in the live tools; migration tool is the only path.

- [ ] **I.1** — `nros migrate workspace <dir>` walks an existing
      pre-212 workspace and emits the new shape:
      - Reads `nros.toml`, writes `<bringup>/system.toml`.
      - For each component pkg: reads `component_nros.toml` /
        `nros/components/*.toml`, writes `Cargo.toml`
        `[package.metadata.nros.component]` (or `nros/components/<Name>.toml`
        for multi-component).
      - Deletes committed `metadata/*.json` (becomes a build artifact;
        the next `cargo build` regenerates).
      - Regenerates `package.xml` via `nros emit package-xml`.
- [ ] **I.2** — Tool is idempotent (re-runnable on already-migrated
      trees w/o change) and reversible w/ `--dry-run`.
- [ ] **I.3** — Every fixture under
      `packages/testing/nros-tests/fixtures/orchestration_*` gets
      migrated in a single sweep after 212.B/C/F/G land. No mixed-shape
      transitional state in the tree.
- **Tests:**
  - [ ] `migrate_orchestration_e2e_fixture_round_trip` — pre-212
        snapshot → migrate → post-212 → planner output identical.
  - [ ] `migrate_orchestration_composable_fixture_round_trip` —
        multi-component case round-trips.
  - [ ] `migrate_idempotent` — second run is a no-op.
  - [ ] `migrate_dry_run_writes_no_files` — dry-run flag honored.
- **Files:**
  `nros-cli/packages/nros-cli-core/src/cmd/migrate.rs`.

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

## Acceptance

- [ ] **Single-node Rust = `cargo build && cargo run` for ALL three
      RMWs** (zenoh, xrce, cyclonedds). No CMake step required. (212.C + 212.K)
- [ ] **Single-node C++ = `cmake -B build && cmake --build build`.**
      RMW selected via `-DNANO_ROS_RMW=…`. (existing path, 212.D adds
      multi-node sibling)
- [ ] **Multi-node Rust = `cargo build && cargo nros plan && nros
      launch <bringup>`** — no separate codegen step. (212.B + 212.C + 212.J)
- [ ] **Multi-node C++ = `cmake -B build && cmake --build build && nros
      launch <bringup>`** — `nano_ros_workspace_metadata()` does the
      plan stage at configure time. (212.D + 212.J)
- [ ] **Mixed Rust+C++ workspace = `cmake -B build && cmake --build
      build`** with `corrosion_import_crate` bridging Rust components
      into cmake's superbuild. (212.D + cross-language acceptance)
- [ ] **One file per component for the user** — `Cargo.toml` (Rust) or
      `CMakeLists.txt` (C/C++) carries the `[package.metadata.nros]` /
      `nano_ros_component()` declaration; `metadata/*.json` is a build
      artifact; `component_nros.toml` retired. (212.B + 212.C)
- [ ] **Every existing fixture migrates to the new shape via one
      `nros migrate workspace` invocation per fixture.** No mixed-shape
      tree allowed. (212.I)
- [ ] **All 7 RTOS adapters (Zephyr, NuttX, FreeRTOS, ThreadX, ESP-IDF,
      PlatformIO, PX4) ship a working 2-component bringup fixture under
      the new shape.** (212.E + 212.H)
- [ ] **Each adapter shim ≤200 LoC; `nros-build` ≤500 LoC; cmake
      `nano_ros_workspace_metadata()` ≤150 LoC.** CI gate via `tokei`.
- [ ] **No `nros build` / `nros test` / `nros flash` / `nros monitor`
      verbs.** Phase-doc grep checked in CI.
- [ ] **A failing rustc / cmake / clang diagnostic in any test fixture
      reaches the user's terminal verbatim** — no aggregation, no
      truncation. CI test injects a synthetic compile error and greps for
      the original message.

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

1. **212.A `cargo-nros` binary shell** (S, in nros-cli repo)
2. **212.B schema + loader** (M)
3. **212.C `nros-build` crate** (M)
4. **212.D `nano_ros_workspace_metadata()` cmake fn** (M)
5. **212.F bringup pkg shape + `nros new system`** (S)
6. **212.G `nros emit package-xml`** (S)
7. **212.I migration tooling** (M)
8. **Apply 212.I sweep to every existing fixture** (S)
9. **212.J `nros launch`** (M)
10. **212.E `nros codegen system`** (M)
11. **212.H RTOS adapter audit + 7 fixtures** (L, can parallelize)
12. **212.K cyclonedds-sys + wrapper** (L, HIGH risk, deferrable)
13. **Acceptance verification + CI gates** (M)

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
- A workspace-root `nros.toml` (system definition lives in
  `<bringup>/system.toml`)
- Per-component `component_nros.toml` (`[package.metadata.nros.component]`
  in `Cargo.toml` is the replacement)
- Committed `metadata/*.json` (build artifact only, lives in
  `$OUT_DIR/nros-gen/` or `target/nros-metadata/`)

Phase-doc CI grep checks that none of these appears in user-facing
surface area (CLI help text, fixture trees, book docs).

## Notes

- Phase 212 is a CLEAN BREAK from the pre-212 shape. No fallback
  loaders, no transitional state in the tree. The migration tool
  (212.I) is the only bridge.
- Live design documents in `docs/design/` continue to iterate after the
  phase lands. Treat the phase doc as the work breakdown; treat the
  design docs as the source of truth on shape decisions.
- The `nros launch` work item (212.J) is one of two paths to resolve
  the "does the bringup pkg need `ament_cmake`?" design-doc open
  question. The other is to always require a colcon outer install
  before `ros2 launch`. Phase 212 commits to 212.J as the canonical
  path; colcon outer integration becomes an opt-in alternative.
- 212.K (Cyclone-Rust pure cargo) is the highest-risk work item.
  Fallback acceptance allows reverting to CMake for cyclonedds if
  upstream Cyclone churn proves the sys-crate wrapper unsustainable.
  Every other work item must land regardless.
- Companion design docs (kept up-to-date as work proceeds):
  - `docs/design/multi-node-workspace-layout.md` (LIVE)
  - `docs/design/workspace-layout-by-case.md` (LIVE)
  - `docs/design/rtos-integration-pattern.md` (LIVE)
