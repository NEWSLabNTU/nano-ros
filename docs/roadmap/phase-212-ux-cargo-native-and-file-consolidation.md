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
  fn `nros_find_interfaces()` (reads `package.xml` `<depend>` rows) runs
  codegen at configure time.
- **Embedded:** vendor tool drives. Adapter shim shells
  `nros codegen-system` at configure time (Zephyr cmake fn, NuttX
  Makefile rule, ESP-IDF component, PIO pre-script, …).

Component packages declare themselves in their own native manifest
(Cargo.toml `[package.metadata.nros.*]` tables for Rust; cmake fns
`nano_ros_component_register` / `nano_ros_entry` for C++).
Multi-node systems compose nodes via a per-board **Entry package**
that carries the launch file + a user-authored `main` calling into a
codegen-emitted `run_plan()` fn (one Entry pkg per board target;
launch file shared across boards via symlink / `<include>`).

**Major design shift (locked 2026-06-02 after M.5.a + survey of
Zephyr / ThreadX board shapes):** Phase 212 originally proposed
auto-generating `main` per board inside a BSP crate's `build.rs`
(M.5.a.1 - M.5.a.4 shipped this for FreeRTOS). Surveying Zephyr +
ThreadX board reality showed the BSP main-codegen is wrong for two
reasons: (1) Zephyr's `main()` is C, owned by Zephyr — Rust is a
staticlib, no room for our generated entry; (2) every new board
adding RTOS support would require an upstream contribution of a
board-specific main-codegen template to nano-ros. The fix is **move
codegen from BSP to Entry pkg**: nano-ros ships codegen + board
library + family driver crates; user writes an Entry pkg (with
`main.rs` + `build.rs` + launch file) per board they want to support.
The result is a ROS 2-aligned model (rclcpp_components + executable
split) with smaller maintainer surface and user-side board porting.

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
2. **Two pkg shapes** (§212.L; was three before the 2026-06-02 redesign):
   - **Component pkg** — board-agnostic node library. `nros::component!()`
     (Rust) or `NROS_COMPONENT_REGISTER()` (C++). No `main`, no spin, no
     board knowledge. Compiles into any binary that wants the node.
   - **Entry pkg** — per-board binary. User-authored `main.rs` (or
     `main.cpp`) calls `Board::run(closure)` where the closure invokes
     codegen-emitted `run_plan(runtime)`. Build.rs reads launch file
     + emits the run_plan via `nros-build` library (§212.N). One Entry
     pkg per board target; launch file shared across boards via
     symlink / `<include>`.
   - **Bringup pkg — RETIRED.** Entry pkg subsumes its role
     (deploy/domain/bridge config moves to Entry pkg's Cargo.toml;
     launch file lives next to Entry pkg). Users wanting ROS 2 colcon-
     convention `<system>_bringup/` pkg can author it as a launch-files
     convenience dir, but nano-ros doesn't mandate it.
3. **Per-pkg metadata location** (Option α, locked):
   - Component pkg (Rust): `Cargo.toml`
     `[package.metadata.nros.component]` w/ `class` + `name` (Phase
     212.M.5.a.1 mangling reads these).
   - Component pkg (C++): cmake fn `nano_ros_component_register(NAME …
     CLASS … SOURCES … DEPLOY …)` writes JSON to build dir.
   - Entry pkg (Rust): `Cargo.toml` `[package.metadata.nros.entry]` +
     `[package.metadata.nros.deploy.<target>]` (board / rmw / domain_id
     / locator) + optional `[[package.metadata.nros.domain]]` /
     `[[package.metadata.nros.bridge]]`.
   - Entry pkg (C++): cmake fn `nano_ros_entry(NAME … SOURCES … DEPLOY
     … BOARD …)` + `nano_ros_deploy(...)`.
   - `system.toml` — RETIRED (was bringup pkg only; bringup pkg
     itself is retired).
4. **Component class name** = `<pkg-dir-name>::<UserClass>` MANDATORY.
   The pkg dir name is the cargo `[package].name` (Rust) or top
   `project()` name (C++). Enforced by `nros check`.
5. **Launch file policy**:
   - REQUIRED in Entry pkg (`<entry-pkg>/launch/system.launch.xml`
     by default; multi-launch resolution per §212.L.6).
   - OPTIONAL in single Component pkg case; `nros plan` /
     `nros codegen-system` / `nros launch` / `nros-build`
     synthesise an implicit launch (`<launch><node pkg=… exec=…/>
     </launch>`) when absent.
   - Multiple files per Entry pkg supported. Resolution: positional
     arg → `<pkg-name>.launch.xml` → `system.launch.xml` → single
     file → synth.
6. **Board trait family** (`packages/boards/nros-board-common`):
   - `Board: BoardInit + BoardPrint + BoardExit` core
   - `BoardEntry: Board { fn run<F, E>(cfg, closure) -> ! }` (or `->
     ()` for FSP-managed boards like orin-spe)
   - `TransportBringup: Board { fn init_transports(cfg) }` (opt-in,
     for boards where Rust drives transport bring-up — bare-metal
     smoltcp, esp-hal WiFi)
   - `NetworkWait: Board { fn wait_network(timeout_ms) }` (opt-in,
     for boards w/ RTOS-owned network stack — FreeRTOS lwIP, ThreadX
     NetX Duo, Zephyr conn_mgr)
   - Family driver crates (`nros-board-{posix,freertos,threadx,
     zephyr,nuttx,esp-idf,bare-metal}`) shipped by nano-ros.
   - Per-board crates: tier-1 boards shipped by nano-ros (`nros-
     board-{native,qemu-mps2-an385-freertos,threadx-linux,esp32-c3,
     …}`). User-authored boards live in user workspace; no upstream
     contribution required.
7. **Codegen split:**
   - `nros::component!()` macro emits 4 per-pkg mangled symbols
     (`_register`, `_init`, `_dispatch`, `_tick`) — M.5.a.1
   - `ExecutorComponentRuntime` runs spin + dispatches callbacks —
     M.5.a.2
   - `nros-build::generate_run_plan(launch_file, plan_json) → run_
     plan.rs` (board-agnostic) — emitted into Entry pkg's `OUT_DIR`
   - `nros-build::generate_single_node_main(Board)` — convenience
     helper for single Component pkg case that synthesises both
     `run_plan.rs` AND a thin `main.rs` (out-of-tree under
     `target/`) so `cargo run` Just Works
8. **Mixed Rust + C/C++** = cmake top-level via Corrosion bridge. Pure
   Rust = cargo top-level. Pure C/C++ = cmake top-level.
9. **Embedded RTOS** = vendor SDK retains its native build tool. nano-
   ros plugs into vendor's external-module hook + bakes the system
   spec into compile-time C config via `nros codegen-system`. The
   Entry pkg's build.rs (Rust) or cmake fn (C++) consumes the bake +
   emits `run_plan`.
10. **Diagnostics passthrough.** Rustc errors stay rustc errors. cmake
    errors stay cmake errors. `nros` errors only when `nros` owns the
    action. No colcon-style `Failed <<<` aggregation.
11. **No colcon as primary orchestrator.** Colcon stays AVAILABLE for
    Autoware-style outer integration via two-graph seam at `nros plan`.

**Irreducible per-Component-pkg user-authored items (Rust):**
- `Cargo.toml.[package].name` — pkg dir name
- `Cargo.toml.[lib]` + `crate-type = ["rlib", "staticlib"]`
- `Cargo.toml.[package.metadata.nros.component].class = "<pkg>::<UserClass>"`
- `Cargo.toml.[package.metadata.ament].{build_depend, exec_depend}` (non-cargo ROS deps)
- `package.xml` (colcon parity)
- `src/lib.rs` — `impl Component for UserClass` + `impl ExecutableComponent for UserClass` + `nros::component!(UserClass);`
- NO `main.rs`. NO launch file. NO board awareness.

**Irreducible per-Entry-pkg user-authored items (Rust):**
- `Cargo.toml.[package].name` — pkg dir name
- `Cargo.toml.[[bin]] name = "<pkg>"`
- `Cargo.toml.[dependencies]` listing every Component pkg as a path-dep
- `Cargo.toml.[dependencies]` exactly one `nros-board-*` crate
- `Cargo.toml.[package.metadata.nros.entry] deploy = "<board>"`
- `Cargo.toml.[package.metadata.nros.deploy.<board>]` board / rmw /
  domain_id / locator
- `package.xml`
- `launch/system.launch.xml` — composition (params, remaps, env,
  `<node>` rows)
- `build.rs` — `nros_build::generate_run_plan(...)` (~3 LoC)
- `src/main.rs` — `Board::run(|runtime| { apply_overlay(runtime)?;
  run_plan(runtime)?; Ok(()) })` (~10-30 LoC)

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
- `nros_find_interfaces()` cmake fn as the cmake configure-time
  codegen step (C++) — reads `package.xml` `<depend>` rows as SSoT
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

**Revision (2026-06-02):** L.1-L.3 redefined per the Entry pkg
redesign. The "Application pkg" of the 2026-05 draft (user main +
launch-overlay init) is generalised + renamed **Entry pkg**.
"Bringup pkg" is RETIRED — Entry pkg subsumes its role. The L.4-L.12
sub-items that already shipped stay marked done.

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
- [ ] **L.2 Entry pkg shape** — Rust authors `Cargo.toml` (`[[bin]]
      name = "<pkg>"` + path-deps on Component pkgs + one
      `nros-board-*` crate + `[package.metadata.nros.entry] deploy =
      "<board>"` + `[package.metadata.nros.deploy.<board>]` (board /
      rmw / domain_id / locator) + optional
      `[[package.metadata.nros.{domain,bridge}]]`) + `package.xml` +
      `launch/system.launch.xml` (composition: `<node pkg=… exec=…/>`
      rows + `<param>` + `<set_remap>` + `<set_env>` + optional
      `<include>`) + `build.rs` (~3 LoC: `nros_build::generate_run_
      plan(...)`) + `src/main.rs` (~10-30 LoC: `Board::run(|runtime| {
      apply_overlay(runtime)?; run_plan(runtime)?; Ok(()) })`). One
      Entry pkg per board target the user wants to support; launch
      file shared across boards via symlink / `<include>`.

      C++ analogous: cmake fn `nano_ros_entry(NAME … SOURCES … DEPLOY
      … BOARD …)` + `nano_ros_deploy(...)`.

      Native-only entries (host POSIX, `cargo run` dev loop) use
      `nros-board-posix`. RTOS Entry pkgs use the per-RTOS family
      crate (`nros-board-freertos` etc.) + a per-board crate (or
      user-authored crate for boards outside the tier-1 set).

      Convenience: single Component pkg with `[package.metadata.nros.
      entry] deploy = "native"` + `build.rs` calling
      `nros_build::generate_single_node_main(Board::Native)`
      synthesises both `run_plan.rs` AND a thin `main.rs` under
      `OUT_DIR` so `cargo run` Just Works without a separate Entry
      pkg dir. Embedded single-Component case still requires a
      hand-written `main.rs` (board init non-trivial).

- [ ] **L.3 Bringup pkg shape — RETIRED (2026-06-02)**. The Path A
      code-free bringup pkg concept introduced in the 2026-05 draft
      is subsumed by Entry pkg. Deploy / domain / bridge config
      moves into Entry pkg's `Cargo.toml` `[package.metadata.nros.*]`
      tables. The launch file lives next to Entry pkg. `system.toml`
      is RETIRED as a Phase-212 artifact — `nros check` rejects it
      everywhere. Users wanting ROS 2 colcon-convention
      `<system>_bringup` pkg may author a launch-files convenience
      dir / pkg w/o `Cargo.toml` themselves, but nano-ros tooling
      does not produce or consume one.
- [x] **L.4 `<pkg>::<Class>` enforcement** — `nros check` MUST reject a
      component pkg whose `class` field doesn't start with the pkg
      directory name (which equals `Cargo.toml::[package].name` for
      Rust and `project()` for C++). Cross-cuts user docs ("the dir
      name IS the pkg name").
- [x] **L.5 Init API patterns**:
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
- [x] **L.6 Launch file resolution + synthesis** —
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
- [ ] **L.7 `[workspace.metadata.nros]` schema + self-entry
      planner** — single field `default_system = "<entry-pkg-name>"`
      pointing at an Entry pkg (post-redesign — the Path A bringup
      case is RETIRED per revised L.3). **Self-entry planner
      support**: `nros plan <pkg-dir>` accepts a single Component
      pkg dir where the dir has `Cargo.toml` + `[package.metadata.
      nros.component]` + `[package.metadata.nros.entry] deploy =
      "<board>"` — single Component pkg eats its own Entry role,
      mostly for `cargo run` dev loop convenience. Emit a one-
      component plan from Cargo metadata; use the L.6 launch
      resolver (real or synth) for the launch file. Same path for
      `nros codegen-system`. Tracked as M-F.2 below (still valid:
      the planner code change is identical regardless of the
      surface naming).
- [x] **L.8 `[package.metadata.nros.deploy.<target>]` table (Option
      α)** — per-pkg deploy targets live in `Cargo.toml`
      `[package.metadata.nros.deploy.<target>]` (Rust) OR via
      `nano_ros_deploy(TARGET … RMW … DOMAIN_ID …)` cmake fn (C++).
      `system.toml` deploy table exists ONLY in Path A bringup pkgs.
      `nros check` rejects per-pkg `system.toml` outside bringup role.
- [x] **L.9 C++ cmake fn surface** — `nano_ros_component_register(NAME
      <name> CLASS <UserClass> SOURCES … DEPLOY …)`,
      `nano_ros_entry(NAME <name> SOURCES … BOARD <board> DEPLOY …)`
      (renamed from `nano_ros_application` per Entry pkg redesign;
      tracked in N.5 below), `nano_ros_deploy(TARGET <name> RMW <rmw>
      DOMAIN_ID <n> LOCATOR <uri>)`, `nano_ros_bridge(…)`,
      `nano_ros_domain(…)`. All fns write metadata JSON to
      `${BUILD}/nros-metadata.json` so `nros codegen-system` reads it
      at configure time. No sidecar TOML for C++ pkgs.
- [x] **L.10 `nros::component!()` macro + Component / ExecutableComponent
      traits** — already shipped per Phase 172 W.3 (see
      `packages/core/nros-macros/src/lib.rs:156`). Macro emits the
      register trampoline; `Component` trait declares nodes/pubs/
      subs/timers/services/actions; `ExecutableComponent` adds
      `init()` + `on_callback(state, cb_id, ctx)` + optional `tick(
      state, ctx)` bodies. Generated runtime owns the spin loop.
- [x] **L.11 `.cargo/config.toml` lint** — `nros check` warns when a
      per-pkg `.cargo/config.toml` carries `[patch.crates-io]`
      entries; patches live exclusively in workspace-root
      `Cargo.toml` (auto-managed by `nros ws sync`). Phase 212.K wave
      11 hit a real shadow bug here.
- [x] **L.12 Vendor-native platform configs out of scope** — `prj.conf`
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

- [x] **M.1 native/rust sweep** — `examples/native/rust/{talker,
      listener}/` → Application pkg (NOT Component as originally
      drafted; Component pkgs are not `cargo run`-able per user
      direction option ii — Component shape reserved for embedded
      fixtures). `examples/native/rust/{service-*,action-*,parameters,
      logging}/` → Application pkg. Init pattern: talker/listener/
      service-*/action-* use `nros::init_with_launch_auto()`; logging
      kept as pure `nros-log` demo (no executor — flagged as
      Pattern 3 framing follow-up). Phase 170.A `lib.rs::run() +
      main.rs::main(){run()}` split collapsed. Shipped wave 1
      (commit `e90df2b66` + cleanup `3058ec6a8`).
- [x] **M.2 native/cpp sweep** — `examples/native/cpp/*` (8 examples)
      → Application pkg with `nano_ros_application()` + `nros_find_
      interfaces(LANGUAGE CPP SKIP_INSTALL)`. Note: phase doc named
      a fn `nros_find_interfaces(LANGUAGE CPP)` (reads `package.xml`
      `<depend>` rows)
      that doesn't exist in tree; the real cmake fn is
      `nros_find_interfaces(LANGUAGE CPP)` which reads `package.xml`
      `<depend>` rows — same SSoT intent. Shipped wave 1.
- [x] **M.3 Zephyr sweep (Rust)** — `examples/zephyr/rust/{talker,
      listener,service-{client,server},action-{client,server}}/` (6
      examples) → Component pkg w/ `[package.metadata.nros.component]`
      + `[package.metadata.nros.deploy.zephyr]`. `service-client-async`
      DEFERRED (no async-Component trait yet; Embassy). 12 Zephyr
      C+C++ examples DEFERRED — H.1 shim only resolves bringup pkgs
      with `system.toml`; the L.7 self-bringup case (Cargo.toml /
      CMakeLists.txt-driven, no separate `system.toml`) lives in
      H.1 follow-up. `zephyr/cpp/cyclonedds/talker-aemv8r/` carve-out
      preserved per CLAUDE.md. Shipped wave 2 (commit `2fcb07c6c`).
- [x] **M.4 NuttX sweep** — `examples/qemu-arm-nuttx/{rust,c,cpp}/*`
      (18 examples total — 6 per lang). `examples/nuttx/` doesn't
      exist in tree. Pre-212 deletions per example: `nros.toml`,
      `build-zenoh/`, `Kconfig`, `Make.defs`, `Makefile`,
      `src/main.{rs,c,cpp}`. Rust pkg names use snake_case
      (`nuttx_rs_<ex>`, `nuttx_c_<ex>`, `nuttx_cpp_<ex>`) so the L.4
      `<pkg>::<UserClass>` lint reads `[package].name` cleanly.
      Client-side examples (service-client + action-client × 3 langs
      = 4 examples) land declarative-metadata-only with no-op bodies
      — `TickCtx` doesn't yet expose `call()` / `send_goal()`
      seams. Shipped wave 2.
- [ ] **M.5 FreeRTOS sweep — SPLIT into M.5.a + M.5.b**:
      - [ ] **M.5.a FreeRTOS BSP prerequisite** — extend the per-
            board BSP crate's `build.rs` baker (currently emits weak
            no-op stubs per `packages/boards/freertos-qemu-mps2-an385-
            bsp/build.rs:181-204`) to: (i) resolve real component
            register symbols (consume each component crate's
            `__nros_component_register` export); (ii) spawn the
            FreeRTOS `ApplicationTask` + bring up lwIP/zenoh-pico
            from the system layer; (iii) drive `Executor::spin` on
            behalf of the component. Validate via the existing
            `phase212_h3_freertos` fixture exchanging real pub/sub
            traffic (today only asserts the staticlib LINKS, not
            that it runs). Also: drop `nano_ros_read_config(
            nros.toml)` cmake fn calls from any FreeRTOS-side caller
            (deferred to M.10 if non-FreeRTOS callers also exist).
            Expand BSP crates beyond `freertos-qemu-mps2-an385-bsp`
            for any non-mps2 board that gets a wave-M.5.b example.
            HARD prerequisite for M.5.b.
      - [ ] **M.5.b FreeRTOS mechanical sweep** — once M.5.a is
            green, transcribe `examples/qemu-arm-freertos/{rust,cpp,
            c}/*` (18 examples) + `examples/freertos/*` (if any
            land) to canonical L.1 Component pkg shape. Drop the
            working `src/main.rs::_start` + `src/lib.rs::run_app`
            user plumbing in favour of `impl Component +
            ExecutableComponent` + `nros::component!()`. Without
            M.5.a, an M.5.b sweep would either break every working
            FreeRTOS example end-to-end OR be cosmetic-only (Cargo
            metadata table added but the imperative plumbing
            retained — trips a future M.12 canonical-shape lint).
- [x] **M.6 ThreadX sweep** — `examples/threadx-linux/{rust,cpp}/*`
      (12 examples) → Component pkg shape + `nano_ros_component_
      register()` cmake fn + `nros_threadx_codegen_system(SYSTEM .)`
      (self-pkg case). C examples NOT in M.6 scope per phase doc.
      `threadx-riscv64/` dir does NOT exist in tree. Single-pkg
      self-bringup configure-clean acceptance on cpp examples
      currently blocked at the `nros plan` "missing-source-metadata"
      step — upstream CLI work, fixed by the schema gap + L.7
      planner work below. Shipped wave 2.
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
- [x] **M.12 Regression test** —
      `packages/testing/nros-tests/tests/phase212_m12_example_shape.rs`.
      Walks `examples/` + asserts (all 7 sub-tests green 2026-06-02):
      - Every migrated example dir has a `package.xml`.
      - Rust example crates declare exactly one of
        `[package.metadata.nros.{component,application}]` (XOR).
      - Component pkgs' `class` string starts with the
        kebab→snake-cased `[package].name` (L.4 lint surface).
      - All deploy targets in `[package.metadata.nros.deploy.<target>]`
        match the platform path (subtable name === target name; native
        uses the `deploy = ["native"]` array shape and is covered by
        Test 2 instead).
      - Path A bringup dirs (those holding `system.toml`) carry no
        `Cargo.toml` / `CMakeLists.txt` / `src/` (L.8 lint complement).
      - Pre-212 files (`nros.toml`, `component_nros.toml`,
        `gen-app-config.py`, `app_config.h.in`, NuttX `Kconfig` /
        `Make.defs`) do NOT survive in any migrated example dir
        (M.10 cleanup gate).
      Skip set keyed by Phase 212.M migration status: ESP32 + ESP32
      bare-metal (M.7 BLOCKED), Cortex-M3 bare-metal + qemu-riscv64-
      threadx + threadx-linux/c + native/c (not in M sweep table),
      stm32f4 RTIC / Embassy variants, native/rust *-rtic + *-async
      + custom-* + lifecycle-node + serial-* (M.1 explicit deferrals),
      native/rust/bridge (Phase 110.G TT demo), templates/. A
      `unmigrated_trees_status_surface` test prints the skip set so
      CI logs document each skip's rationale.

### §212.M follow-ups (gates / blockers surfaced by waves 1+2)

The mechanical sweeps deliberately land ahead of some downstream
infrastructure work. These follow-ups must close before the §212.M.12
canonical-shape regression test can run green tree-wide:

- [ ] **M-F.1 `ComponentMetadata` schema gap** (nros-cli). The
      installed CLI's `[package.metadata.nros.component]` reader
      rejects `class` / `name` fields and the `[package.metadata.
      nros.deploy.<target>]` table (see §212.L.4 + L.8). Extend the
      `ComponentMetadata` struct + add new `ApplicationMetadata` +
      `DeployTargetMetadata` structs. Mutex `component` XOR
      `application`. Strict `deny_unknown_fields`.
- [ ] **M-F.2 L.7 self-bringup planner support** (nros-cli). Today
      `nros plan` / `nros codegen-system` require either a Path A
      bringup pkg (has `system.toml`, no `Cargo.toml`) or a
      workspace pointer. Add a third resolution case: single-pkg
      self-bringup — Component or Application pkg w/
      `[package.metadata.nros.deploy.<target>]` AND no sibling
      bringup pkg. Pkg eats its own bringup role. Plan emits a
      one-component view from Cargo metadata directly. The L.6
      launch resolver (`launch_synth::resolve_launch`) covers the
      launch-file-absent case from §212.L.6 trigger condition 5.
      Co-shipped with M-F.1.
- [ ] **M-F.3 Zephyr H.1 shim self-pkg case** (nano-ros). The H.1
      adapter `zephyr/cmake/nros_system_generate.cmake` only resolves
      sibling Path A bringup pkgs (`IS_DIRECTORY … AND EXISTS
      "${_abs}/system.toml"`). After M-F.1+M-F.2 land, extend the
      shim's `_nros_system_resolve_bringup()` helper to also accept
      a self-pkg dir (Cargo.toml / CMakeLists.txt-driven). Unblocks
      M.3 C+C++ Zephyr sweep + any single-pkg Zephyr Rust example.
- [ ] **M-F.4 `TickCtx` client API gap** (nros / nros-cpp). The
      Phase 172 W.3 `ExecutableComponent::tick()` Context exposes
      publish + action-server ops but no service-client `call()` /
      action-client `send_goal()` seams. Wave-2 NuttX + ThreadX +
      Zephyr Rust sweeps landed the 4 client-side examples per lang
      as declarative-metadata-only with no-op bodies. Real
      runtime-side client dispatch landed in W.5.6 (separate). Once
      shipped, transcribe client examples to use the real ops.
- [ ] **M-F.5 Async-Component trait** (nros). `examples/zephyr/
      rust/service-client-async/` uses Embassy. No async-Component
      shape exists today. Decide: extend `Component`/`Executable
      Component` w/ an async variant, or formally drop the
      async-client demo from the example matrix. Deferred until
      L-Wave / runtime authors pick the path.
- [ ] **M-F.6 FreeRTOS BSP runtime gate (M.5.a)** — see M.5
      split above. The per-board BSP `build.rs` baker currently
      emits weak no-op stubs; needs to spawn ApplicationTask + run
      Executor::spin before M.5.b sweep is sound.
- [ ] **M-F.7 H.5 ESP-IDF cross-compile gate** — see M.7. nros-node
      `executor/spin.rs` `alloc::sync::Arc` ptr-atomics gap on
      `riscv32imc`. Swap to `portable_atomic_util::Arc` (or
      critical-section + portable-atomic single-core flags). Unblocks
      M.7 ESP-IDF sweep.
- [ ] **M-F.8 PX4 H.7 SITL board overlay** — see §212.H.7. Codegen
      emits `nros_<name>/` module dirs but PX4's `make px4_sitl_
      default --dry-run` doesn't pick them up without an enable
      fragment in `boards/px4/sitl/*.px4board`. Either a
      `--board-overlay <path>` codegen flag writing outside the
      vendored PX4 tree, or operator-supplied overlay file.
- [ ] **M-F.9 `nros generate-rust` default output path mismatch**
      (nros-cli). Auto-managed `[patch.crates-io]` blocks in every
      example's Cargo.toml point at `build/nros_generator_rs/<pkg>/`
      but `nros generate-rust` defaults output to `generated/`.
      Either rename the default OR teach `nros ws sync` to write
      the patch-table at `generated/`. Cosmetic mismatch — both
      paths work today as long as you pass `-o build/nros_generator
      _rs` explicitly. Surfaced repeatedly across wave 1+2 sweeps.
- [ ] **M-F.10 `cmake/NanoRosReadConfig.cmake` deletion** (nano-ros).
      Lives at `packages/core/nros-c/cmake/NanoRosReadConfig.cmake`
      (NOT `cmake/` as M.10 phase doc says — correct the path).
      12+ live callers outside FreeRTOS (Zephyr / NuttX / native /
      ThreadX-RV64 trees). Deferred to M.10 final-pass after every
      wave retires its callers.
- [x] **M-F.11 nano_ros_generate_interfaces vs nros_find_interfaces
      naming reconciliation** (phase doc + sibling design docs + book).
      Resolved by renaming references in the phase doc / design docs /
      book to the actual shipping fn `nros_find_interfaces(LANGUAGE
      CPP)` (reads `package.xml` `<depend>` rows). Doc-only fix —
      the cmake function name stays as it is (honest descriptor of
      behaviour: find from package.xml, not generation).
- **Tests** (per-wave, gated on SDK availability):
  - [ ] `native_rust_talker_listener_e2e_<rmw>` per RMW
  - [ ] `native_cpp_talker_listener_e2e_<rmw>` per RMW
  - [ ] `zephyr_<example>_builds` per migrated Zephyr example
  - [ ] Same for nuttx / freertos / threadx / platformio / px4
  - [x] `pre_212_files_forbidden_in_migrated_examples` (M.12) —
        shipped as a sub-test of `phase212_m12_example_shape`.
  - [x] `component_class_strings_match_package_name` (M.12) — shipped
        as a sub-test of `phase212_m12_example_shape` (L.4 lint
        surface; the M.11 lint side lands in nros-cli).
- **Files:** `examples/` (tree-wide sweep), `packages/core/nros-c/
  include/nros/zephyr/app_config.h` (DELETE), `cmake/NanoRosReadConfig.
  cmake` (DELETE if exists), `nros-cli/packages/nros-cli-core/src/
  cmd/check.rs` (M.11 lints), regression test under
  `packages/testing/nros-tests/tests/`.

### 212.N — Component + Entry pkg taxonomy (platform-agnostic Board family)

Lock the two-pkg user model (Component pkg + Entry pkg) introduced
2026-06-02. Goal: codegen stays board-agnostic; user-authored Entry
pkg (~30 LoC `main.rs`) owns board choice via the `Board` trait
family — no per-board `system_main.rs` baker for the codegen path.
Replaces the M.5.a FreeRTOS BSP baker as the long-term shape.

- [ ] **N.1 `Board` trait family in `nros-platform`** — define
      `Board: BoardInit + BoardPrint + BoardExit`. Compose mixins:
      `TransportBringup: Board` (Ethernet / WiFi / CAN / serial /
      USB CDC / IVC — board picks one or several at type-system
      level), `NetworkWait: Board` (carrier / DHCP / link-up gate),
      `BoardEntry: Board { fn run<F, E>(setup: F) -> Result<(), E>
      where F: FnOnce(&mut RuntimeCtx) -> Result<(), E>; }`. The
      `run` method owns board init + transport bringup + executor
      lifecycle + clean exit. `setup` callback receives a
      `RuntimeCtx` for overlay (params / remaps / env) plus the
      generated `run_plan(runtime)` codegen call.
- [ ] **N.2 Family driver crates** — `nros-board-{posix,freertos,
      threadx,zephyr,nuttx,esp-idf,bare-metal}`. Each implements the
      `Board` traits over its RTOS surface. Drives `nros::init` +
      `Executor::spin` + transport bringup via the matching
      `packages/drivers/` crates (`nros-smoltcp`, `cmsdk-uart`,
      `virtio-net-netx`, `stm32f4-usart`, …). Zephyr is the carve-
      out: Kconfig + DTS own BSP, the family crate implements only
      `NetworkWait` over `<zephyr/net/net_if.h>` (Rust staticlib
      can't take over `main`).
- [ ] **N.3 Tier-1 per-board crates** — `nros-board-{native,qemu-
      mps2-an385-freertos,qemu-arm-nuttx,threadx-linux,esp32-c3,
      qemu-riscv64-threadx,orin-spe}`. Each thin shim plugs the
      family crate plus the board's clock / pinmux / transport
      choice. Users for boards outside this set author their own
      `BoardEntry` impl in their Entry pkg (or a side crate) —
      the family crate is the porting surface.
- [ ] **N.4 `nros-build::generate_run_plan(launch_file)` codegen
      library** — extract the launch → plan → `run_plan(runtime: &
      mut RuntimeCtx) -> Result<(), Error>` Rust-fn emitter from
      the per-board BSP baker into a standalone codegen library
      consumed from Entry pkg `build.rs`. Reads the launch XML (or
      `--launch <path>` arg), resolves component pkg metadata via
      cargo-metadata, writes `$OUT_DIR/run_plan.rs` that the Entry
      pkg `main.rs` `include!`s. The emitted fn is BOARD-AGNOSTIC
      (board choice lives in user `main.rs`'s `Board::run` call).
- [ ] **N.5 `nros-build::generate_single_node_main(Board)`
      convenience** — for the L.7 single-Component-pkg case, emit
      a thin `$OUT_DIR/main.rs` skeleton in addition to
      `run_plan.rs` so `cargo run` Just Works without a separate
      Entry pkg dir. Triggered when Cargo metadata declares
      `[package.metadata.nros.entry] deploy = "<board>"` directly
      on a Component pkg. Embedded boards still require a hand-
      written Entry pkg (board init is non-trivial; convenience is
      native-host only at first).
- [ ] **N.6 Rename `nano_ros_application` → `nano_ros_entry`** —
      cmake fn rename per L.9. Add `BOARD <board>` arg. Update every
      existing caller (after wave-1 native/cpp sweep) — single
      backward-compat shim emits a `MESSAGE(DEPRECATION …)` then
      forwards.
- [ ] **N.7 Migrate FreeRTOS BSP baker back to pure board init** —
      retire the M.5.a baker's `__nros_component_*` symbol-walking
      + `system_main.rs` synthesis. Once N.1–N.4 ship, FreeRTOS
      Entry pkg user-authors `main.rs` w/ `Board::run`; BSP crate
      shrinks to clock / lwIP init / one `extern "C" fn
      ApplicationTask`. Same migration applies to every M.5.b
      Component pkg: it sheds `nros::component!()` register-only
      duties and gains a sibling Entry pkg per board target.
- [ ] **N.8 Board family + porting docs (book chapter)** —
      `book/src/porting/board-trait.md`: trait surface, lifecycle,
      transport-mixin selection, worked example for a new board
      (clock + UART + smoltcp). Add the Component + Entry pkg
      cookbook to `book/src/user-guide/`. Update
      `docs/design/multi-node-workspace-layout.md` to reflect the
      Entry pkg as composition root (replacing Bringup pkg).
- **Tests:**
  - [ ] `posix_board_run_executes_run_plan` — host POSIX Entry pkg
        from a 2-component launch XML reaches `run_plan` body +
        spins.
  - [ ] `freertos_board_run_executes_run_plan` — same fixture under
        `nros-board-qemu-mps2-an385-freertos` reaches `run_plan` +
        spins under QEMU.
  - [ ] `single_node_native_convenience_generates_main` —
        `generate_single_node_main` emits both files; `cargo run`
        prints expected output.
  - [ ] `entry_pkg_metadata_required_board` — Entry pkg without
        `[package.metadata.nros.entry] deploy = "<board>"` →
        `nros check` hard error.
  - [ ] `board_agnostic_run_plan_links_against_any_board` — same
        compiled `run_plan` rlib links under at least 2 distinct
        Board impls (posix + freertos) in the test fixture.
- **Files:** `packages/core/nros-platform/src/board/{mod,init,
  print,exit,transport,network,entry}.rs` (NEW),
  `packages/boards/nros-board-{posix,freertos,threadx,zephyr,nuttx,
  esp-idf,bare-metal}/` (NEW family crates), `packages/boards/
  nros-board-{native,qemu-mps2-an385-freertos,…}/` (NEW per-board
  shims), `packages/codegen/nros-build/src/{run_plan,single_node}.
  rs` (NEW codegen library; lives in standalone nros-cli repo per
  CLAUDE.md `nros setup` provisioner), `cmake/NanoRosEntry.cmake`
  (RENAMED), `book/src/porting/board-trait.md` (NEW),
  `docs/design/multi-node-workspace-layout.md` (UPDATED for
  Component + Entry pkg taxonomy).

## Acceptance

Two-step Rust (codegen + build) is the canonical user surface;
one-step C++ (cmake configure runs codegen as a side effect of the
cmake fn) is the canonical C++ user surface. See §Goal for the
asymmetry rationale.

- [ ] **Single-node Rust = `nros generate-rust && cargo build && cargo
      run` for ALL three RMWs** (zenoh, xrce, cyclonedds). No CMake step
      required. (212.K Option B)
- [ ] **Single-node C++ = `cmake -B build && cmake --build build`.**
      RMW selected via `-DNANO_ROS_RMW=…`. `nros_find_interfaces()`
      (package.xml-SSoT) runs codegen at configure. (existing path;
      cmake-side codegen)
- [ ] **Multi-node Rust = `nros generate-rust && cargo build && cargo
      run -p <entry-pkg>`** — explicit codegen step + cargo builds +
      Entry pkg `build.rs` calls `nros-build::generate_run_plan` +
      user `main.rs` runs `Board::run`. No separate `nros plan` step
      for native; embedded Entry pkg still routes through
      `nros codegen-system` for vendor-toolchain integration. (212.B +
      212.L Entry + 212.N)
- [ ] **Multi-node C++ = `cmake -B build && cmake --build build &&
      ./build/<entry>`** — `nano_ros_entry()` cmake fn owns Entry-
      pkg-side codegen at configure time. (212.D + 212.N)
- [ ] **Mixed Rust+C++ workspace = `cmake -B build && cmake --build
      build`** with `corrosion_import_crate` bridging Rust components
      into cmake's superbuild. (212.D + cross-language acceptance)
- [ ] **Two pkg shapes work for both langs** — Component pkg
      (lib only — `impl Component` / `NROS_COMPONENT_REGISTER`,
      board-agnostic) + Entry pkg (board-aware `main.rs` /
      `nano_ros_entry()` w/ `Board::run`). Bringup pkg RETIRED.
      Single-Component-pkg convenience covered via L.7 self-entry
      planner + N.5 `generate_single_node_main`. (212.L + 212.N)
- [ ] **Per-pkg metadata in vendor manifest** — Rust uses Cargo.toml
      `[package.metadata.nros.{component,entry,deploy.<target>,
      domain,bridge,embedded}]`; C++ uses cmake fns
      (`nano_ros_component_register`, `nano_ros_entry`,
      `nano_ros_deploy`). No sidecar TOML for any pkg. `system.toml`
      RETIRED tree-wide. (212.L + 212.N)
- [ ] **`Board` trait family ships tier-1 board crates** — posix +
      qemu-mps2-an385-freertos + qemu-arm-nuttx + threadx-linux +
      esp32-c3 + qemu-riscv64-threadx + orin-spe. Each entries-pkg
      `main.rs` `Board::run` call links a working board impl. (212.N)
- [ ] **`nros-build::generate_run_plan` codegen library exists** —
      Entry pkg `build.rs` ~3 LoC, `main.rs` ~10-30 LoC, both
      board-agnostic. Same `run_plan` rlib links under ≥2 distinct
      Board impls. (212.N.4 + N.5)
- [ ] **Component class follows `<pkg>::<UserClass>`** — pkg dir name
      MUST match the prefix. `nros check` enforces. (212.L.4)
- [ ] **Launch file synthesis works for single Component pkg** —
      Component pkg w/ `[package.metadata.nros.entry]` self-entry
      shape but no launch file gets an implicit one synthesised in-
      memory by `nros plan` / `nros codegen-system` /
      `generate_single_node_main`. (212.L.6 + L.7 + N.5)
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
12. **212.L Pkg shape + unified launch model** (L) — IN PROGRESS; lock canonical shapes + lints + launch synth (Bringup pkg RETIRED 2026-06-02 per N redesign)
13. **212.M Example migration sweep + pre-212 cleanup** (L) — IN PROGRESS; tree-wide sweep + lint enforcement
14. **212.N Component + Entry pkg taxonomy (Board family)** (L) — NEW 2026-06-02; platform-agnostic Board trait + family + codegen lib split; N.7 retires M.5.a baker
15. **Acceptance verification + CI gates** (M)

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
  `Cargo.toml` `[package.metadata.nros.*]` / cmake fns)
- Per-component `component_nros.toml` (`[package.metadata.nros.component]`
  in `Cargo.toml` is the replacement)
- ANY `system.toml` (RETIRED tree-wide 2026-06-02 with the Bringup pkg
  retirement — deploy / domain / bridge data lives in Entry pkg
  `Cargo.toml` `[package.metadata.nros.{deploy.<target>,domain,
  bridge}]` / cmake fns; see §212.L.3 + N)
- Bringup pkg (Path A code-free orchestration declaration) — RETIRED
  2026-06-02; Entry pkg subsumes the role. Users wanting a ROS 2
  colcon-convention `<system>_bringup` pkg may author one themselves
  but nano-ros tooling does not produce or consume one.
- Per-board `system_main.rs` baker codegen (retired by §212.N.7 once
  the Board trait family ships; the M.5.a FreeRTOS baker is the
  interim shape, not the destination)
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
- Two pkg shapes (Component pkg + Entry pkg) replace the pre-212
  "every pkg has a Cargo.toml + nros.toml" model AND replace the
  2026-05 draft's three-pkg taxonomy. The mental cost of picking
  shape pays off in: lib-only Component pkgs compose into multi-
  component bringups + are BOARD-AGNOSTIC (ship native + every RTOS
  unchanged); Entry pkgs own board choice via `Board::run` and the
  composition root via the launch file. See §212.L + §212.N for the
  full taxonomy + Board trait family. The earlier Bringup pkg
  concept is retired; Path A code-free orchestration declarations
  were folded into Entry pkg.
- The Board trait family (§212.N.1) is the porting surface. Per-
  board crates (~tier-1 list in §212.N.3) ship in-tree; out-of-tree
  boards author their own `BoardEntry` impl in their Entry pkg
  without contributing back. Codegen stays board-agnostic by
  design — no per-board `system_main.rs` baker survives.
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
