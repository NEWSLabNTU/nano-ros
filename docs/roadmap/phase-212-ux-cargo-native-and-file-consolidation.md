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

- [x] **D.1** — Landed: feat(212.D) `15de00e3c` — cmake nano_ros_workspace_metadata() + multi-pkg fixtures.
      Original spec: — cmake function `nano_ros_workspace_metadata(SYSTEM <bringup-pkg>
      [WORKSPACE_ROOT <dir>])` in `cmake/nano_ros_workspace_metadata.cmake`.
      ≤150 LoC HARD cap.
- [x] **D.2** — Landed: feat(212.D) `15de00e3c` — workspace_metadata() shells `nros plan`.
      Original spec: — Function shells `nros plan` at cmake configure time
      with the bringup pkg path; emits `${CMAKE_BINARY_DIR}/nros_components.cmake`;
      `include()`s it so component targets are visible to cmake natively.
- [x] **D.3** — Landed: feat(212.D+H.4) `e1b1b3bd6` — Corrosion in setup tier.
      Original spec: — Cross-language interop: `corrosion_import_crate()`
      already supported for Rust components; the function exposes both
      C++ and Rust component targets uniformly. The plan stage decides
      which language each component is in.
- [x] **D.4** — Landed: feat(212.D) `15de00e3c` — multi-pkg fixtures document the incantation.
      Original spec: — Documented user incantation: top-level `CMakeLists.txt`
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

- [x] **H.1 Zephyr** — Landed: feat(212.H) `8278955b9` (sweep) + fix(212.H.1+H.5) `9abb85b28` + fix(212.H) `330450e82`.
      Original spec: — `zephyr/module.yml` + `zephyr/CMakeLists.txt`
      provides `nros_system_generate()` cmake fn that shells `nros codegen
      system`. Today's `app_config.h` baker (per-example) retires.
- [x] **H.2 NuttX** — Landed: feat(212.H) `8278955b9` — RTOS adapter sweep (7 adapters consume codegen-system bake).
      Original spec: — `integrations/nuttx/` provides
      `apps/external/<bringup>/` symlink + `Makefile context::` rule
      that runs `nros codegen system` then `NROS_CARGO_BUILD`.
- [x] **H.3 FreeRTOS** — Landed: feat(212.H) `8278955b9` — RTOS adapter sweep.
      Original spec: — per-board crate `freertos-<board>-bsp` runs
      `nros codegen system` in `build.rs`, emits `nros_config_generated.h`.
      No separate `integrations/freertos/` directory needed (cargo path
      IS the adapter).
- [x] **H.4 ThreadX** — Landed: feat(212.D+H.4) `e1b1b3bd6` — Corrosion in setup tier + mixed-Rust tests.
      Original spec: — `cmake/platform/nano-ros-threadx.cmake` runs
      `nros codegen system` at cmake configure time + uses Corrosion to
      import Rust component crates. No `integrations/threadx/`.
- [x] **H.5 ESP-IDF** — Landed: feat(212.H.5+H.7+H.8) `34db111ad` + fix(212.H.1+H.5) `9abb85b28`.
      Original spec: — `integrations/esp-idf/` ESP-IDF component w/
      `idf_component_register` + `Kconfig.projbuild`; configure-time
      `add_subdirectory(<nano-ros-root>)` triggers `nros codegen system`.
- [x] **H.6 PlatformIO** — Landed: feat(212.H) `8278955b9` — RTOS adapter sweep (PlatformIO covered).
      Original spec: — repo-root `library.json` + pre-build
      `extra_script` that invokes `nros codegen system --ahead-of-vendor`.
- [x] **H.7 PX4** — Landed: feat(212.H.5+H.7+H.8) `34db111ad` + feat(212.H) `8278955b9`.
      Original spec: — `integrations/px4/` template that the codegen
      emits one module dir per component into; user runs PX4's
      `make px4_sitl` after `nros plan`.
- [x] **H.8 LoC audit** — Landed: feat(212.H.5+H.7+H.8) `34db111ad` + test(212.I.3+212.H.8) `7e0e5cbce` (tokei budget gate).
      Original spec: — each adapter shim ≤200 LoC verified by
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
- [x] **I.3** — Landed: test(212.M.11+M.12) `d9dc99787` — canonical-shape walker covers `tests/fixtures/`.
      Original spec: — Every fixture under
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

- [x] **K.1** — Landed: feat(212.K.1+K.2+K.5) `11dc35f38` — pure-cargo cyclonedds for native Rust.
      Original spec: — `cyclonedds-sys` crate at
      `packages/dds/cyclonedds-sys/` vendors Cyclone via the `cmake`
      build-script crate against `third-party/dds/cyclonedds` (pinned
      0.10.5). Forces `ENABLE_LTO=OFF`, `BUILD_IDLC=ON`. Separate host
      `idlc` build target. Exports `links = "ddsc"`, `cargo:idlc`,
      `cargo:include`.
- [x] **K.2** — Landed: feat(212.K.1+K.2+K.5) `11dc35f38` + refactor `233520435` (drop hard-coded std_msgs/Int32).
      Original spec: — `nros-rmw-cyclonedds-sys` wrapper crate at
      `packages/dds/nros-rmw-cyclonedds-sys/` runs `cc::Build::cpp(true)`
      over existing `packages/dds/nros-rmw-cyclonedds/src/*.cpp`. Bakes
      `rmw_dds_common_graph` descriptor via bundled host `idlc`. Emits
      `cargo:rustc-link-lib=static:+whole-archive,-bundle=nros_rmw_cyclonedds`
      + `dylib=stdc++`. Risk: HIGH (semi-internal Cyclone headers).
- [x] **K.3 (PRE-REQ)** — Landed: feat(212.K.3) `802e13021` — msg_to_cyclone_idl.py ported to Rust crate.
      Original spec: — Port `scripts/cyclonedds/msg_to_cyclone_idl.py`
      to Rust as a `nros-msg-to-idl` library + build-dep helper. Python
      build-dep is a regression for the pure-cargo promise.
- [x] **K.4** — Landed: feat(212.K.4) `f5bf74901` + refactor `233520435` (drop hard-coded std_msgs/Int32).
      Original spec: — Per-example descriptor codegen: extend `nros codegen`
      with `nros codegen cyclonedds-descriptors`. Emits a small Rust
      crate w/ the idlc C output + register TU; consumed via build-dep.
- [x] **K.5** — Landed: feat(212.K.1+K.2+K.5) `11dc35f38` — pure-cargo cyclonedds for native Rust.
      Original spec: — `examples/native/rust/{talker,listener}/Cargo.toml` get
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
      (was: DEFERRED; dropped 2026-06-02 per M-F.5 — no async-Component
      trait yet; Embassy variant retired, native tokio sibling kept). 12 Zephyr
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
- [x] **M.5 FreeRTOS sweep — SPLIT into M.5.a + M.5.b**:
      - [x] **M.5.a FreeRTOS BSP prerequisite** — extend the per-
            board BSP crate's `build.rs` baker (previously emitted
            weak no-op stubs at `packages/boards/freertos-qemu-mps2-
            an385-bsp/build.rs:181-204`) to: (i) resolve real
            component register symbols (consume each component
            crate's `__nros_component_register` export); (ii) spawn
            the FreeRTOS `ApplicationTask` + bring up lwIP/zenoh-pico
            from the system layer; (iii) drive `Executor::spin` on
            behalf of the component. Shipped in 4 commits — `5f271ff9f`
            (per-pkg mangled register symbol via `nros::component!()`)
            + `0aaef01d2` (Executor-backed ComponentRuntime in nros
            crate) + `7087e8114` (BSP baker emits real
            `system_main.rs`) + `ce88408af` (callback dispatch through
            BSP path). `phase212_h3_freertos` fixture remains the
            runtime gate. Dropping `nano_ros_read_config(nros.toml)`
            from FreeRTOS-side callers + expanding BSP crates to
            non-mps2 boards roll into M.10 / future M-wave work.
      - [x] **M.5.b FreeRTOS mechanical sweep** — `examples/qemu-arm-
            freertos/{rust,cpp,c}/*` transcribed to canonical L.1
            Component pkg shape. Imperative `src/main.rs::_start` +
            `src/lib.rs::run_app` plumbing replaced by
            `impl Component + ExecutableComponent` +
            `nros::component!()`. Shipped in `8bd016d66`. M.12
            canonical-shape regression test (`phase212_m12_example_
            shape`) confirms the FreeRTOS tree carries no pre-212
            files + every example pkg classifies cleanly.
- [x] **M.13 native/c sweep (informal — landed 2026-06-02)** — surfaced
      by the M.12 walker as a gap (M.1+M.2 covered native/{rust,cpp}
      only; native/c was unaddressed). Migrated all 10 examples
      (`talker, listener, logging, service-{client,server},
      action-{client,server}, custom-msg, custom-platform,
      custom-transport-loopback`) to the §212.M.2 Application pkg shape,
      C variant: added `package.xml` with `<build_type>cmake</build_type>`,
      replaced `add_executable(...) + nros_generate_interfaces(...)`
      chain with `nros_find_interfaces(LANGUAGE C SKIP_INSTALL)` +
      `nano_ros_application(NAME ... SOURCES ... DEPLOY native)`,
      enabled `LANGUAGES C CXX` (the codegen FFI glue needs CXX even
      for C apps), kept `nros_platform_link_app(<target>)` for the
      RMW register stub. Fixed two pre-existing source bugs surfaced
      by the rebuild: `custom-platform/src/main.c` had `g_logger`
      declared after its callback use site (moved to before the
      callback); `custom-transport-loopback/src/main.c` called the
      never-defined `nros_publisher_publish` (replaced with the typed
      helper `std_msgs_msg_int32_publish`) and shipped a bare
      `int main()` (replaced with `NROS_APP_MAIN_REGISTER_POSIX()`
      macro emit). The `phase212_m12_example_shape` walker dropped
      its `examples/native/c/` carve-out; `phase212_examples_
      canonical_shape` reports zero `native/c/` violations. All 10
      examples build clean via `cmake -B build && cmake --build build`.
- [x] **M.13.b stm32f4/rust application metadata (landed 2026-06-02)** —
      strict canonical-shape lint flagged 7 `stm32f4/rust/` examples
      (talker + 6 RTIC variants) missing
      `[package.metadata.nros.application]`. Added the table with
      `deploy = ["stm32f4"]` to each, mirroring the M.11 native/rust
      sweep + the e6f4cb346 esp32-baremetal pattern. The Embassy
      variant (`stm32f4/rust/talker-embassy`) stays carved out
      pending M-F.5 async-Component work — it has no `package.xml`
      yet. `phase212_examples_canonical_shape` reports zero
      `stm32f4/` violations after this sweep. The
      `phase212_m12_example_shape` walker promoted
      `examples/stm32f4/rust/` into `MIGRATED_PREFIXES` and narrowed
      the `UNMIGRATED_PREFIXES` carve-out from the entire `stm32f4/`
      tree to just `stm32f4/rust/talker-embassy/`. Also fixed a
      trailing-slash mismatch bug in the walker's `is_migrated()`
      that prevented per-leaf un-migration carve-outs from being
      recognised; now uses a normalised prefix-with-trailing-slash
      match.
- [x] **M.6 ThreadX sweep** — `examples/threadx-linux/{rust,cpp}/*`
      (12 examples) → Component pkg shape + `nano_ros_component_
      register()` cmake fn + `nros_threadx_codegen_system(SYSTEM .)`
      (self-pkg case). C examples NOT in M.6 scope per phase doc.
      `threadx-riscv64/` dir does NOT exist in tree. Single-pkg
      self-bringup configure-clean acceptance on cpp examples
      currently blocked at the `nros plan` "missing-source-metadata"
      step — upstream CLI work, fixed by the schema gap + L.7
      planner work below. Shipped wave 2.
- [x] **M.7 ESP-IDF / ESP32 sweep** — `examples/esp32/rust/{talker,
      listener}/` migrated to ESP-IDF idf.py workflow via
      `integrations/nano-ros` ESP-IDF component (`d94371fa2`). The
      M-F.7 Arc / portable-atomic gate closed in `15a5e1717` (Arc
      swap + `portable_atomic_unsafe_assume_single_core` cfg +
      ESP-IDF FreeRTOS critical-section wrap + xrce-cffi-staticlib
      panic-halt feature). `phase212_m7_esp32_{talker,listener}`
      tests added; `phase212_h5_esp_idf` green. No `c`/`cpp` esp32
      examples in tree (CLAUDE.md carve-out preserved).
- [ ] **M.8 PlatformIO sweep** — `examples/platformio/*` (when any
      land). H.6 extra_script handles framework-agnostic codegen via
      `nros codegen-system --ahead-of-vendor --framework <f>`.
- [ ] **M.9 PX4 sweep** — `examples/px4/cpp/uorb/nros-register-check/`
      stays as-is (the canonical PX4 surface per Phase 115.K.4).
      Multi-node PX4 case (H.7-shipped emit) operates on bringup pkgs
      writing into `$PX4_AUTOPILOT_DIR/src/modules/`.
- [~] **M.10 Pre-212 file cleanup** — partial close 2026-06-02 audit.
      Per-file disposition (verified by `git ls-files` + the
      `phase212_m12_example_shape` + `phase212_examples_canonical_
      shape` lints):
      - [x] `component_nros.toml` per-pkg — zero tracked instances.
      - [x] `gen-app-config.py` per-example baker — zero tracked.
      - [x] `app_config.h.in` / per-example `<nros/app_config.h>`
        Kconfig-synthesis — zero tracked.
      - [x] Per-example committed `metadata/*.json` — zero tracked.
        Both M.12 walkers now ban regression
        (`phase212_examples_canonical_shape` already enforced;
        `phase212_m12_example_shape` extended in this audit to match).
      - [x] Phase 170.A `lib.rs::run()` + `main.rs::main(){run()}`
        split files in `examples/native/rust/*` — collapsed by M.1
        (Application pkg shape uses `src/main.rs` only; no `lib.rs`
        survivor).
      - [x] Legacy `examples/native/rust/{talker,listener}/CMakeLists.
        txt` (Phase 175.A Cyclone CMake fallback — superseded by
        Option B pure-cargo path) — zero tracked.
      - [x] Stale `examples/native/rust/{talker,listener}/generated/`
        dirs from pre-Option-B codegen runs — never tracked
        (per-dir `.gitignore` covers them).
      - [ ] `nros.toml` (any location) — 39 tracked files remain in
        UNMIGRATED trees: `qemu-arm-baremetal/rust/` (13),
        `qemu-esp32-baremetal/rust/` (2), `qemu-riscv64-threadx/`
        {c, cpp, rust} (6+6+6 = 18), `threadx-linux/c/` (6). These
        are still active — the bare-metal board crates parse
        them at runtime via `Config::from_toml`; threadx-linux/c
        CMakeLists call `nano_ros_read_config`. Per-tree deletion
        rolls into the corresponding future sweep (not in any
        named M.x slot yet; candidate new waves M.13+).
      - [ ] `nano_ros_read_config(nros.toml)` cmake fn (delete the
        fn + every caller) — covered by M-F.10. 24 callers
        remain (4 in `cmake/platform/*` + `cmake/NanoRosConfig.
        cmake` + the defn at `packages/core/nros-c/cmake/
        NanoRosReadConfig.cmake` + 19 in unmigrated example trees
        threadx-linux/c + qemu-riscv64-threadx). Final pass after
        the qemu-riscv64-threadx + threadx-linux/c sweeps retire
        their callers.
      Tools refresh: the sibling `phase212_examples_canonical_
      shape.rs` lint test had a `toml::Value::FromStr`-shape bug
      (rejected full Cargo.toml documents); fixed to use
      `toml::from_str` in this audit so the lint reports honest
      violations going forward.
- [x] **M.11 `nros check` lints (defensive)** — in-tree tree-walker
      slice landed via `d9dc99787`: `phase212_examples_canonical_
      shape` (76 violations at landing time — sweep-side gap) +
      `phase212_pre_212_files_forbidden` (41 examples + 18 fixtures
      hits). Both grep-assert the §212.L pkg taxonomy + pre-212
      file ban; both fail loud with a deduped punch list, no
      `#[ignore]`. The CLI-side `nros check` slot for L.4 / L.8 /
      L.11 lints is the matching nros-cli follow-up.
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
      bridges/ (Phase 110.G TT demo — relocated 2026-06-02 from
      native/rust/bridge/ per §212.L sibling-category rule), templates/. A
      `unmigrated_trees_status_surface` test prints the skip set so
      CI logs document each skip's rationale.

### §212.M follow-ups (gates / blockers surfaced by waves 1+2)

The mechanical sweeps deliberately land ahead of some downstream
infrastructure work. These follow-ups must close before the §212.M.12
canonical-shape regression test can run green tree-wide:

- [x] **M-F.1 `ComponentMetadata` schema gap** (nros-cli) — shipped
      pre-redesign in nros-cli `5e810c0` (2026-06-02 03:50). Schema
      adds `class` / `name` to `ComponentMetadata`, new
      `ApplicationMetadata` (`deploy: Vec<String>` allow-list, native-
      only enforced), `DeployTargetMetadata` per-target table.
      `validate()` mutex on `component XOR components XOR
      application`. `+12` unit tests (229 → 241). Post-redesign
      Entry-pkg shape (`[package.metadata.nros.entry] deploy =
      "<single-board>"`) is a DIFFERENT taxonomy not covered here —
      folded into §212.N.4 (run_plan codegen lib reads Entry pkg
      metadata) + §212.N.5 (single_node_main triggered by
      `[package.metadata.nros.entry]` on a Component pkg).
- [x] **M-F.2 L.7 self-bringup planner support** (nros-cli) — co-
      shipped with M-F.1 in `5e810c0`. `nros plan` / `nros codegen-
      system` accept the third resolution case (single Component or
      Application pkg w/ `[deploy.*]` + no sibling bringup pkg eats
      its own bringup role); `is_self_bringup_eligible` predicate +
      `resolve_bringup` path-shaped hint mapping basename → bringup
      key. New nano-ros integration test `phase212_l7_self_bringup`
      3/3 green. Same caveat as M-F.1: the pre-redesign
      `application`-shape pathway is the actual surface; the
      Entry-pkg shape that supersedes it (2026-06-02 redesign) is
      §212.N.4/N.5 territory.
- [x] **M-F.3 Zephyr H.1 shim self-pkg case** (nano-ros) —
      Shipped in `843ffea74` (`feat(212.M-F.3+M-F.6): Zephyr shim
      self-pkg + FreeRTOS BSP probe fix`). The H.1 adapter
      `zephyr/cmake/nros_system_generate.cmake`'s
      `_nros_system_resolve_bringup()` helper now accepts a self-pkg
      dir (Cargo.toml / CMakeLists.txt-driven) in addition to the
      Path A bringup case. Unblocks Zephyr single-pkg Rust examples;
      M.3 C+C++ Zephyr sweep remains its own wave.
- [~] **M-F.4 `TickCtx` client API gap** — substrate landed
      2026-06-02. `nros::component::ClientDispatch` trait (sibling
      to `ActionExecutor`) defines `call_raw` (service-client) +
      `send_goal_raw` (action-client). `TickCtx` carries
      `&'a mut dyn ClientDispatch` and exposes typed wrappers:
      `call<Req, Resp, REQ_N, RESP_N>` + `call_raw` +
      `send_goal<G, N>` + `send_goal_raw`. `UnsupportedClients`
      stub in the in-tree `ExecutorComponentRuntime` returns
      `ComponentError::Runtime` for all client ops; replaced by
      the codegen-emitted `GenClientDispatch` impl in nros-cli.
      Remaining sub-items:
      - [x] **M-F.4.a** — `GenClientDispatch` shipped in nros-cli
        branch `phase-212-m-f-4-a-genclientdispatch` (commit
        `0b2b206`). Mirrors `GenActionExec`: collects per-instance
        service-client + action-client handles by stable entity id,
        registers them inline before `emit_tick_entry`, emits a
        `GenClientDispatch` impl alongside the existing
        `GenActionExec`. Both backends share a `*mut Executor` to
        sit on the two `&mut dyn` arms of the new 3-arg
        `TickCtx::new`. Bounded 200-iter `spin_once(10ms)` poll in
        `call_raw` (~2s cap); condvar/waker integration is a
        future refinement. `nros-cli` 241/241 unit tests pass + new
        `generated_service_client_emits_gen_client_dispatch`
        integration test. Pending maintainer merge of the
        `phase-212-m-f-4-a-genclientdispatch` branch.
      - [x] **M-F.4.b** — Per-lang client-side example transcription
        shipped 2026-06-02. All 6 wave-2 Component pkgs now carry a
        real `tick` body invoking `ctx.call` / `ctx.send_goal`:
        - service-client × {`qemu-arm-nuttx/rust`,
          `threadx-linux/rust`, `zephyr/rust`} — timer fires →
          `on_callback` flips `pending`/`counter`; `tick` drains the
          flag, builds `AddTwoIntsRequest`, calls
          `ctx.call::<Req, Resp, 64, 64>(...)`. Two of the three
          (ThreadX, Zephyr) had no timer in registration — they use
          a one-shot `sent` flag pattern on first tick.
        - action-client × same three platforms — one-shot
          `send_goal::<FibonacciGoal, 32>(...)` on first tick;
          `sent` flag flipped only on `Ok`, so the next tick retries
          until the M-F.4.a-shipped `GenClientDispatch` reaches the
          installed CLI (today the in-tree `UnsupportedClients`
          stub returns `ComponentError::Runtime` and the flag stays
          false — honest behavior + stable source). Verified
          `cargo check` on 4/6 (NuttX × 2 + ThreadX-Linux × 2);
          Zephyr × 2 share the same template + are blocked on the
          pre-existing `zephyr-build` crate-not-on-crates.io gap
          (out of M-F.4.b scope).
      - [x] **M-F.4.c** — `nros-cpp` mirror shipped in `71ce0a7ba`.
        New FFI symbols `nros_cpp_tick_ctx_call_raw` +
        `nros_cpp_tick_ctx_send_goal_raw` (Rust impl at
        `packages/core/nros-cpp/src/tick_ctx.rs`); C++ wrappers
        `nros::TickCtx::{call_raw, call<Req,Resp>, send_goal_raw,
        send_goal<G>}` in `<nros/tick_ctx.hpp>`; cbindgen header
        regen'd. Stub contract matches `UnsupportedClients`
        (returns `RET_ERROR` until M-F.4.a-equivalent C++ codegen
        wires the impl). 8 unit tests + C++ link smoke green.
      Wave-2 client examples (nuttx_rs_*, threadx_*, zephyr_rs_*
      service-/action-client) keep no-op bodies + the
      `_state, _ctx` ABI; M-F.4.b transcription unblocks once
      M-F.4.a reaches the installed nros-cli.
- [x] **M-F.5 Async-Component trait** (nros). `examples/zephyr/
      rust/service-client-async/` used Embassy; no async-Component
      shape exists today. Decision 2026-06-02: **drop the
      async-client demo from the example matrix** (the simpler
      resolution). Embassy variant deleted; native tokio sibling
      (`examples/native/rust/service-client-async/`) retained as
      the async-client reference. The async-`Component` /
      `ExecutableComponent` trait extension itself remains
      deferred until L-Wave / runtime authors pick the path; if
      revived, re-introduce the Zephyr example then.
- [x] **M-F.6 FreeRTOS BSP runtime gate (M.5.a)** — Closed by the
      M.5.a chain (`5f271ff9f` + `0aaef01d2` + `7087e8114` +
      `ce88408af`) plus the BSP probe fix in `843ffea74`. Per-board
      BSP `build.rs` baker now spawns `ApplicationTask` + drives
      `Executor::spin`. Sound prerequisite for the M.5.b sweep that
      subsequently landed in `8bd016d66`.
- [x] **M-F.7 H.5 ESP-IDF cross-compile gate** — Closed in
      `e4204459a` (`fix(212.M.7): swap alloc::sync::Arc →
      portable_atomic_util::Arc on no_std RTOS wake path`). nros-node
      `executor/spin.rs` now routes through
      `portable_atomic_util::Arc` on `riscv32imc` (and any target
      missing ptr atomics). Unblocked the M.7 ESP-IDF sweep that
      then shipped in `15a5e1717`.
- [ ] **M-F.8 PX4 H.7 SITL board overlay** — see §212.H.7. Codegen
      emits `nros_<name>/` module dirs but PX4's `make px4_sitl_
      default --dry-run` doesn't pick them up without an enable
      fragment in `boards/px4/sitl/*.px4board`. Either a
      `--board-overlay <path>` codegen flag writing outside the
      vendored PX4 tree, or operator-supplied overlay file.
- [x] **M-F.9 `nros generate-rust` default output path mismatch** —
      tree-side reconciliation landed in `964914870`: 72 example
      `Cargo.toml`s + 1 just-recipe comment rewritten so every
      `[patch.crates-io]` block points at `generated/<pkg>` to match
      the `nros generate-rust` no-`-o`-flag default. Examples now
      build with a plain `nros generate-rust && cargo build`.
      `examples/esp32/` was skipped by the parallel agent (the
      sibling M.7 sweep had owned-rewrite scope); `docs/roadmap/
      phase-210-ros-convention-codegen.md` documents the existing
      `nros ws sync` output — aligning that emit is a follow-up in
      the nros-cli repo (`github.com/NEWSLabNTU/nros-cli`).
- [~] **M-F.10 Retire cmake codegen of `nros/app_config.h`** (nano-ros) —
      **partial close 2026-06-02**, **design locked 2026-06-02 as
      Path C** (below).

      ### Status

      - [x] **M-F.10.0 — orphan deletion.** Deleted
        `packages/core/nros-c/cmake/NanoRosReadConfig.cmake` (260
        LoC duplicate, never `include()`-ed anywhere). Cleaned a
        leftover stale-`nano_ros_read_config(...)` caller in
        `examples/qemu-riscv64-threadx/rust/talker/CMakeLists.txt`
        (M.10 sweep deleted its `nros.toml` but missed the cmake
        side; the cyclonedds C path doesn't reference
        `NROS_APP_CONFIG` so its per-binary header was dead weight).

      ### Open — Path C (locked design)

      Why not "refactor each board to take its own Config struct
      across FFI" (the earlier Path A proposal)? It would break the
      **universal `NROS_APP_CONFIG` user-facing read promise**:
      today every example reads `NROS_APP_CONFIG.zenoh.locator` /
      `.network.ip` / etc. with the same paths regardless of
      board. Passing per-board Config structs across FFI = per-board
      access types = porting between boards rewrites every read
      line.

      **Path C — preserve universal `nros_app_config_t` API; move
      population from cmake codegen to source-side `extern`
      definition.** Each example/board still exposes a uniform
      `const nros_app_config_t NROS_APP_CONFIG = { ... };` symbol
      — but the symbol is **author-emitted in source** rather
      than cmake-emitted from `nros.toml`.

      Board-specific fields outside the universal shape
      (`uart_base`, interface name, scheduling knobs) stay in each
      board crate's own Rust `Config` struct, NOT in
      `nros_app_config_t`. Two-tier separation: universal contract
      (`nros_app_config_t`) ↔ board-local extension (`<board>::Config`).

      ### Path C work items

      - [ ] **M-F.10.1 Header surface flip.** Modify
        `packages/core/nros-c/include/nros/zephyr/app_config.h`'s
        non-Zephyr branch: replace the `#error` stub with
        `extern const nros_app_config_t NROS_APP_CONFIG;` (forward
        declaration only). Header now only defines the struct
        type + declares the symbol; no `static const` initialiser
        baked in. Zephyr `__ZEPHYR__` Kconfig branch unchanged.

      - [ ] **M-F.10.2 Board startup.c — no source changes.**
        The 3 board crate startup.c files
        (`nros-board-mps2-an385-freertos`, `nros-board-threadx-linux`,
        `nros-board-threadx-qemu-riscv64`) ALREADY read
        `NROS_APP_CONFIG.network.*` via `<nros/app_config.h>`. After
        M-F.10.1 lands, that include resolves to the `extern`
        declaration. The reads continue to work via linker symbol
        resolution. Confirm with a single-example smoke build per
        board.

      - [ ] **M-F.10.3 Per-board NROS_APP_CONFIG emission.** Each
        board crate exposes a helper that emits
        `const nros_app_config_t NROS_APP_CONFIG = { ... };` from
        its Rust `Config`. Two options (pick during impl):
        - `build.rs` writes the symbol into a generated `.c` baked
          into the board's staticlib.
        - Rust side passes the Config into an `nros_set_app_config(
          const nros_app_config_t*)` setter installed before
          `nros_support_init`; startup.c reads through the setter.
        Whichever lands, the user-facing read pattern
        (`NROS_APP_CONFIG.network.ip`) stays unchanged.

      - [ ] **M-F.10.4 Example sweep.** For C / C++ examples that
        rely on the universal struct (today: every embedded
        example that includes `<nros/app_config.h>` indirectly via
        the board startup), confirm the symbol resolves. Native
        examples already moved away from `NROS_APP_CONFIG` in the
        M.13 native/c sweep (replaced with literal locator strings
        at the call site) — no change needed.

      - [ ] **M-F.10.5 cmake codegen retirement.** Once M-F.10.1–4
        are green, delete:
        - `cmake/NanoRosConfig.cmake` — the 2 fns
          (`nano_ros_read_config()` +
          `nano_ros_generate_config_header()`) plus the 3 internal
          helpers (`_nros_ip_to_c` / `_nros_mac_to_c` /
          `_nros_prefix_to_netmask`). 231 LoC.
        - `cmake/templates/nros_app_config.h.in` — the codegen
          template.
        - Include directives for `NanoRosConfig.cmake` in
          `cmake/platform/nano-ros-{freertos,nuttx,threadx,esp_idf}.cmake`
          (4 files). The `Pulls in NanoRosReadConfig.cmake` doc
          comment lines retire alongside.

      - [ ] **M-F.10.6 Verification matrix.** Build-smoke per
        board:
        - FreeRTOS-MPS2 (any `examples/qemu-arm-freertos/c/talker`
          variant).
        - ThreadX-linux (`examples/threadx-linux/rust/talker` +
          one cpp example).
        - ThreadX-QEMU-RV64 (`examples/qemu-riscv64-threadx/c/talker`).
        - Zephyr (unaffected — Kconfig branch handles its own
          population).
        Each must `cmake configure + build` clean without
        `nano_ros_read_config` / `nano_ros_generate_config_header`
        in the cmake fn surface.

      ### Trade-offs

      Pro: universal `NROS_APP_CONFIG.*` read promise preserved
      across platforms; no cmake codegen / `nros.toml`
      indirection; aligns with the M.10 sweep that already moved
      Rust-side `Config` literals into source. Board-specific
      fields stay in board crates (no over-sharing into the
      universal struct).

      Con: every embedded example/board now needs ONE explicit
      `NROS_APP_CONFIG` definition site (today cmake codegen
      wrote it for them). Mitigated by per-board build.rs / setter
      pattern. Slight duplication (mitigation: per-board emission
      helper). Path C is intentionally NOT changing the universal
      contract — long-term we can replace `nros_app_config_t` with
      board-extension presets, but that's beyond M-F.10's scope.

      ### Effort estimate

      ~ 1 day total — 30 min header surface + 1 hr per board (×3) +
      mechanical 2 hr example sweep + 1 hr cmake retire + smoke
      builds.
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

- [x] **N.1 `Board` trait family in `nros-platform`** — landed via
      `1e4e0df92`. Spec: `Board: BoardInit + BoardPrint + BoardExit`.
      Compose mixins: `TransportBringup: Board` (Ethernet / WiFi /
      CAN / serial / USB CDC / IVC — board picks one or several at
      type-system level), `NetworkWait: Board` (carrier / DHCP /
      link-up gate), `BoardEntry: Board { fn run<F, E>(setup: F) ->
      Result<(), E> where F: FnOnce(&mut RuntimeCtx) -> Result<(),
      E>; }`. The `run` method owns board init + transport bringup +
      executor lifecycle + clean exit. `setup` callback receives a
      `RuntimeCtx` for overlay (params / remaps / env) plus the
      generated `run_plan(runtime)` codegen call. Trait surface in
      `packages/core/nros-platform`. N.2 family driver crates + N.3
      tier-1 per-board crates + N.4 codegen library extraction are
      the open follow-ups (other agents own the `nros-cli` codegen-
      side work).
- [x] **N.2 Family driver crates** — `nros-board-{posix,freertos,
      threadx,zephyr,nuttx,esp-idf,bare-metal}`. Each implements the
      `Board` traits over its RTOS surface. Drives `nros::init` +
      `Executor::spin` + transport bringup via the matching
      `packages/drivers/` crates (`nros-smoltcp`, `cmsdk-uart`,
      `virtio-net-netx`, `stm32f4-usart`, …). Zephyr is the carve-
      out: Kconfig + DTS own BSP, the family crate implements only
      `NetworkWait` over `<zephyr/net/net_if.h>` (Rust staticlib
      can't take over `main`).
- [x] **N.3 Tier-1 per-board crates** — `nros-board-{native,qemu-
      mps2-an385-freertos,qemu-arm-nuttx,threadx-linux,esp32-c3,
      qemu-riscv64-threadx,orin-spe}`. Each thin shim plugs the
      family crate plus the board's clock / pinmux / transport
      choice. Users for boards outside this set author their own
      `BoardEntry` impl in their Entry pkg (or a side crate) —
      the family crate is the porting surface.
- [x] **N.4 `nros-build::generate_run_plan(launch_file)` codegen
      library** — shipped in nros-cli `84988e8` (2026-06-02). New
      `packages/nros-build/` crate exposes:
      - `pub fn generate_run_plan(launch_file: impl Into<PathBuf>)
        -> Result<PathBuf>` — Entry pkg `build.rs` 3-LoC entry.
      - `pub fn generate_run_plan_with(opts: &Options) -> Result
        <PathBuf>` — explicit-options form.
      - `pub struct Options { pub fn from_env(launch_file) }` —
        sources `OUT_DIR` etc. from cargo env.
      - `pub mod emit` — internal emit primitives.
      Follow-up `1011127` drops the in-crate `RuntimeError` def
      (matches nano-ros `1c3f31080` no_std move): emit references
      `::nros_platform::RuntimeError`. Already consumed by every
      Entry pkg `build.rs` in `examples/*/rust/*_entry/` via
      `nros-build = { git = "github.com/NEWSLabNTU/nros-cli",
      branch = "main" }`. Board-agnostic emit (board choice lives
      in user `main.rs`'s `Board::run` closure).
- [ ] **N.5 `nros-build::generate_single_node_main(Board)`
      convenience** — for the L.7 single-Component-pkg case, emit
      a thin `$OUT_DIR/main.rs` skeleton in addition to
      `run_plan.rs` so `cargo run` Just Works without a separate
      Entry pkg dir. Triggered when Cargo metadata declares
      `[package.metadata.nros.entry] deploy = "<board>"` directly
      on a Component pkg. Embedded boards still require a hand-
      written Entry pkg (board init is non-trivial; convenience is
      native-host only at first).

      **Status:** non-blocking UX polish; defer post-Phase 212.
      The explicit Entry pkg pattern (N.4 already shipped) handles
      every single-node case today — users write a sibling
      `<pkg>_entry/` dir (~3-LoC build.rs + ~10-LoC main.rs). N.5
      would collapse that single dir for native-only single-
      Component-pkg cases (getting-started demos, one-shot
      iteration). Trade-offs:
      - Pro: simpler "hello world" — single pkg + `cargo run`.
      - Con: the same Cargo.toml must declare BOTH `[lib]` (for
        the Component impl) AND `[[bin]]` pointing at the codegen-
        emitted `$OUT_DIR/main.rs` — awkward + risks colliding
        with user-supplied `src/main.rs`.
      - Con: native-only asymmetry (embedded still requires
        explicit Entry pkg) creates a two-pattern UX that learners
        must reason about.
      - Con: codegen-emitted `main.rs` paths in `[[bin]]` need
        platform support that's still settling.
      Revisit once the wave-4 Entry pkg pattern has soaked for a
      release + a clean "single-pkg cargo run" UX story emerges
      from real usage feedback.
- [x] **N.6 Rename `nano_ros_application` → `nano_ros_entry`** —
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
  - [x] **step-1 entry-poc** (commit `82407c40c` family) — native
        Entry pkg with empty launch.xml validates the Phase 212.N
        shape end-to-end. `cargo build && ./target/debug/entry-poc`
        prints `nros: application complete` exits 0.
  - [x] **step-2 wave-4 Component pkg `register(runtime)` wrappers
        + 18 sibling Entry pkgs** (commit `f9ae826a4`) — every
        FreeRTOS / NuttX / threadx-linux Rust example gains a
        sibling `*_entry/` Entry pkg crate, and every M.5.b Component
        pkg exposes a `pub fn register<R>(runtime: &mut R)` no-op
        wrapper. The codegen-emitted `run_plan(runtime)` body invokes
        these wrappers; the wrappers are stubs because `RuntimeCtx`
        doesn't yet expose a `ComponentRuntime` sink.
  - [x] **step-2.5 `RuntimeError` moved to `nros-platform`
        (no_std)** (commits `ee4a545cf` + nros-cli `1011127`) — was
        a step-3 prerequisite: embedded Entry pkgs can't depend on
        `nros-build` at runtime (its `cargo_metadata` /
        `serde_json` / `thiserror` graph is std-only). Now lives in
        `packages/core/nros-platform/src/board/runtime.rs` and is
        re-exported at the crate root. `nros-build` stays a
        build-dep only; its emit template references
        `::nros_platform::RuntimeError`.
  - [~] **step-3 RuntimeCtx ↔ ComponentRuntime bridge** — extend
        `nros_platform::RuntimeCtx` with a `&mut dyn ComponentRuntime`
        slot populated by each `BoardEntry::run` impl before the setup
        closure fires. Update every Component pkg's `register(runtime)`
        wrapper to materialise its component via the runtime instead
        of the no-op stub (call into `<Component as Component>::register`
        through the sink). Cross-cuts seven `BoardEntry` impls (native,
        freertos, threadx-{linux,riscv64}, nuttx, zephyr, esp32) plus
        every wave-4 Component pkg. **Design locked 2026-06-02 as
        Path A (BoardEntry owns executor + runtime slot)**.

      ### Design — Path A (locked)

      `BoardEntry::run` opens the `Executor`, builds an
      `ExecutorComponentRuntime`, installs it into `RuntimeCtx`,
      calls the user `setup` closure (= codegen-emitted
      `run_plan(runtime)`), then spins until halt. User `main.rs`
      stays 5 lines. Spin loop hidden per platform.

      Rejected: Path B (setup callback owns executor + spin) —
      defeats the trait abstraction, forces every user `main.rs`
      to know executor lifecycle.

      **Customisation escape hatches** (Path A's answer to per-user
      orchestration):
      - **Override `OUT_DIR`** — point `nros-build` at a checked-in
        dir, edit the emitted `run_plan.rs` by hand.
      - **Skip `build.rs` entirely** — hand-write `main.rs` that
        calls `runtime.runtime.register_dispatch_slot_dyn(...)` per
        component directly. The `BoardEntry::run` setup closure is
        an arbitrary `FnOnce`; the codegen path is one shape among
        many.

      ### Path A sub-items

      - [ ] **step-3.1 `ComponentRuntime` trait in
            `nros-platform::board::runtime`** — object-safe, no_std.
            Two methods covering the registration + spin surfaces:
            ```rust
            pub trait ComponentRuntime {
                fn register_dispatch_slot_dyn(
                    &mut self,
                    register: ComponentRegisterFn,
                    init: ComponentInitFn,
                    dispatch: ComponentDispatchFn,
                    tick: ComponentTickFn,
                    name: &'static str,
                ) -> Result<(), ()>;
                fn spin_once(&mut self, timeout_ms: u32) -> Result<(), ()>;
            }
            ```
            Fn-pointer signatures (`ComponentRegisterFn` etc.) stay
            in `nros-platform` (already no_std, ABI-stable).

      - [ ] **step-3.2 `RuntimeCtx` widens** — add
            `runtime: &'a mut dyn ComponentRuntime` field +
            `RuntimeCtx::with_runtime(runtime, overlay)` ctor.
            Existing overlay accessors unchanged. Update entry-poc
            + 18 wave-4 Entry pkg main.rs files for the new ctor —
            mechanical sweep.

      - [ ] **step-3.3 `ExecutorComponentRuntime` impls
            `nros_platform::ComponentRuntime`** — one impl block in
            `packages/core/nros/src/component_runtime.rs`. Forwards
            to existing `register_dispatch_slot` + `spin_once`.

      - [ ] **step-3.4 `nros::component!()` macro emits
            `pub fn register(runtime: &mut RuntimeCtx<'_>)`
            wrapper** — replaces the hand-written generic-`R` stub
            on 24 Component pkgs. Body:
            ```rust
            runtime.runtime.register_dispatch_slot_dyn(
                __nros_component_<pkg>_register,
                __nros_component_<pkg>_init,
                __nros_component_<pkg>_dispatch,
                __nros_component_<pkg>_tick,
                "<pkg>",
            ).map_err(|_| RuntimeError::ComponentRegister("<pkg>"))
            ```
            Legacy `__nros_component_*` externs stay (step-4 / step-6
            retire them). Macro expansion site needs
            `nros_platform` ident in scope — add to every Component
            pkg `Cargo.toml` (the wave-4 step-2 sweep deliberately
            omitted it).

      - [ ] **step-3.5 Per-board `BoardEntry::run` impls** — seven
            boards: native, mps2-an385-freertos, threadx-linux,
            threadx-qemu-riscv64, nuttx, zephyr, esp32. Body shape:
            ```rust
            Self::init_hardware(&cfg);
            /* kernel-specific app task spawn */
            let exec = Executor::open(&exec_cfg)?;
            let mut crt = ExecutorComponentRuntime::from_executor(exec);
            let mut rt = RuntimeCtx::with_runtime(&mut crt, overlay);
            setup(&mut rt)?;
            loop {
                crt.spin_once(10)?;
            }
            ```
            Spin policy: BoardEntry always spins until error / halt
            flag. Native uses ctrl-C to set halt flag; embedded
            spins forever. Zephyr stays a NetworkWait-only carve-out
            (Kconfig drives its own entry).

      - [ ] **step-3.6 Component pkg cleanup** — delete the
            hand-written `pub fn register<R>` stub from 24 Component
            pkg `src/lib.rs` files. The macro-emitted wrapper from
            step-3.4 replaces them.
  - [ ] **step-4 retire `bake_system_main_rs` + symbol-walking** —
        `packages/boards/freertos-qemu-mps2-an385-bsp/build.rs` drops
        the baker fn; `src/lib.rs::nros_run` is rewritten to depend
        only on board init (clock + lwIP + `ApplicationTask`). The
        `__nros_component_*_register` / `_init` / `_dispatch` /
        `_tick` extern decls and the `NROS_*_FNS` slices are
        deleted. Same deletion pattern in the other M.5.a board
        BSPs once the FreeRTOS path lands.
  - [ ] **step-5 firmware bin migration** — the
        `multi_pkg_workspace_freertos/firmware` test fixture (+
        the threadx, nuttx siblings) switches from BSP `nros_run()`
        to a real Entry pkg `main.rs` (`<Board as BoardEntry>::run`
        + `include!(...run_plan.rs)`).
  - [ ] **step-6 `nros::component!()` macro cleanup** — drop the
        `__nros_component_*` symbol emission once no consumer walks
        them. The macro keeps the typed `Component` impl and the
        `register(runtime)` wrapper.
- [x] **N.8 Board family + porting docs (book chapter)** —
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
