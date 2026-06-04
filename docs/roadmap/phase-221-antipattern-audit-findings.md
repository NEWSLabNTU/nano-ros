# Phase 221 — Build System + Test Antipattern Audit Findings

**Goal**: Record antipatterns surfaced by a 5-slice read-only audit of the
nano-ros build system + test scripts. Pre-group findings into **tracks**
with non-overlapping file ownership so future remediation waves can run
in parallel without rebase conflict.

**Status**: CLOSED 2026-06-04. Created 2026-06-04 from 5 parallel
`Explore`-mode audit agents on `main` @ `60a8ac052`; Tracks A+B+C+D+E
landed on `main` @ `09dcd2620`.

**Priority**: HIGH for Track A (large build-script refactor) + Track B
(test name behavior-description rename). MED for Track C (template
extraction). LOW for Tracks D + E.

**Depends on**: Phase 218 closure (DONE — CLI in-tree).

> **Notes**: The audit checked five distinct antipattern axes per a
> contributor-driven request:
> 1. Embedded source code in scripts/build files (prefer templates)
> 2. Large/complex `cmake` / `build.rs` / `*.just` needing redesign
> 3. Very long names
> 4. Phase names leaked into code identifiers (temp contributor names)
> 5. Other antipatterns (banned tests, hardcoded paths, magic numbers, …)

---

## Overview — track summary

| track | what | scope | severity | est. effort |
|---|---|---|---|---|
| **A** — Large build-script refactor | `zpico-sys/build.rs` 1978 LOC; `nros-c`/`nros-cpp` build.rs duplication; `zephyr.just` 1474 LOC | 4–6 files | HIGH | days |
| **B** — Test FN rename to describe-behavior | 28 test fn names > 60 chars across `cli/` + `testing/nros-tests/` | ~30 files | HIGH | hours |
| **C** — Extract embedded source to templates | 3 large `format!()`-walls in `nros-c/build.rs` (C/Rust header generation) | 1 file | MED | hours |
| **D** — Phase-name leakage cleanup | 1 FFI fn + 1 test mod + 1 test fn + 1 node-name string | 4 sites | LOW | minutes |
| **E** — Misc hygiene | 3 board crates missing `.gitignore`; small platform inconsistencies | scattered | LOW | minutes |

**Acceptance — Tracks A+B+C+D+E all closed** = build/test system audited clean against the 5 antipattern categories.

---

## Track A — Large build-script refactor

**Status**: DONE 2026-06-04. Large build scripts were moved behind helper
crates/modules/scripts, preserving the public recipe/build surfaces. Final
verification passed with `cargo check -p zpico-sys -p nros-c -p nros-cpp` and
`source ./activate.sh && XDG_RUNTIME_DIR=/tmp just build`.

**HIGH priority. Files exceed clean-decomposition thresholds.**

### A.1 — `packages/zpico/zpico-sys/build.rs` (1978 LOC) — CRITICAL

- **Smell**: Mixed concerns (env probing + CMake invocation + cc compilation + cbindgen + config generation). 27+ helper fns but monolithic `main()`.
- **Refactor**: Extract `ShimConfig`, `ZenohBufferConfig`, `LinkFeatures` probing + `generate_*` fns into a new `nros-zpico-build` helper crate (sibling to `nros-board-common`, `nros-build-paths`, `nros-sizes-build`). `build.rs` collapses to a thin wrapper invoking helper fns.
- **Acceptance**: `build.rs` < 200 LOC; all helpers covered by `nros-zpico-build` unit tests; `cargo check -p zpico-sys` clean.

### A.2 — `packages/core/nros-c/build.rs` (628 LOC) + `packages/core/nros-cpp/build.rs` (565 LOC) — ~95% duplication

- **Smell**: Identical structure: env probing → `cc::Build` for weak stubs → `cc::Build` for `log_fmt.c`. `apply_baremetal_libc()`, `picolibc_include()` repeated verbatim across both.
- **Refactor**: New `nros-build-helpers` crate carrying `apply_baremetal_libc()`, `picolibc_include()`, `compile_weak_stubs()` etc. Both `nros-c`/`nros-cpp` `build.rs` shrink to ~100 LOC thin wrappers.
- **Acceptance**: Both build.rs < 150 LOC; cross-build clean on every platform target.

### A.3 — `packages/boards/nros-board-threadx-qemu-riscv64/build.rs` (486 LOC) + `nros-board-nuttx-qemu-arm/nros-nuttx-ffi/build.rs` (322 LOC) — board pattern replication

- **Smell**: Linear `cc::Build` orchestration with repeated config-passing (`configure_riscv64`, `add_threadx_includes`, `add_netx_includes`, …). Pattern dupes per board.
- **Refactor**: Consolidate into helper fns (`build_threadx_port()`, `build_netx_duo()`, `build_virtio_net()`) under `nros-board-common`. New board crates call helpers instead of re-implementing.
- **Acceptance**: Both build.rs < 200 LOC; new board crates onboard via < 50 LOC of cc helper calls.

### A.4 — `just/zephyr.just` (1474 LOC, 33 recipes) — HIGH

- **Smell**: Mixes setup (workspace init + patch apply), CI (fixture matrix), dev workflows. Some recipes carry 30-line bash bodies.
- **Refactor**: Split into 3 modules — `just/zephyr-setup.just` + `just/zephyr-ci.just` + `just/zephyr-dev.just`. Main `zephyr.just` dispatches.
- **Acceptance**: Each split module < 600 LOC; top-level `just zephyr <recipe>` surface unchanged.

### A.5 — `justfile` (2242 LOC) + 3 long recipes — MED

- **Recipes > 30 LOC**: `regenerate-bindings` (72), `build-all-jobserver` (51), `_check-fixtures-stale` (50).
- **Refactor**: Extract to `scripts/regenerate-bindings.sh`, `scripts/build-all-jobserver.sh`, `scripts/check-fixtures-stale.sh`. justfile recipes become 1–3 line wrappers.
- **Acceptance**: Justfile recipe bodies all < 20 LOC; scripts independently runnable + reviewable.

### A.6 — `zephyr/CMakeLists.txt` (833 LOC) — MED

- **Smell**: RMW dispatch `CONFIG_NROS_RMW_ZENOH` / `_XRCE` / `_CYCLONEDDS` branches with large inline `file(GLOB_RECURSE ...)`. No helper fns.
- **Refactor**: Per-RMW cmake fns under `zephyr/cmake/nros_rmw_<rmw>.cmake`. Inline GLOB → per-backend `.cmake` modules.
- **Acceptance**: `zephyr/CMakeLists.txt` < 400 LOC; per-RMW cmake fns reusable.

### A.7 — Top-level `CMakeLists.txt` (347 LOC) — LOW (already planned)

- **Status**: RMW + platform `if(...)` chains. Per existing comments, refactor planned for a later phase (was Phase 138). No action this phase.

---

## Track B — Test FN names: rename to describe-behavior

**Status**: DONE 2026-06-04. The listed 29 test functions were renamed and
each renamed test carries a `///` behavior summary. Closure scan also renamed
four additional >60-char production test functions in the same scope:
`codegen_cyclonedds_emits_std_msgs`,
`codegen_cyclonedds_rejects_duplicate_stem`,
`codegen_system_resolves_zephyr_alias`, and
`cmake_decls_skip_duplicate_pair`.
Focused verification passed: no remaining >60-char test function names in the
Track B scan scope; `cargo test -p nros -p nros-node --lib`;
`cargo test -p nros-cli-core -p rosidl-codegen --lib` from `packages/cli`;
no-run builds for touched CLI integration tests and touched `nros-tests`
integration tests.

**HIGH priority. 28 production test fns > 60 chars. Filename convention `phase212_*.rs` stays as roadmap cross-ref; FN names lose phase markers + verbose chains.**

### B.1 — nros-cli test renames (16 files)

Renames inside `packages/cli/nros-cli-core/{src,tests}/`. Each entry: `current (chars) → proposed`:

| file:line | current | proposed |
|---|---|---|
| `src/orchestration/planner.rs:4113` | `infer_callback_chains_links_publisher_instance_to_subscriber_callback` (69) | `infer_publisher_to_subscriber_chains` |
| `src/cmd/codegen_system.rs:1270` | `codegen_system_resolves_workspace_default_system_to_self_bringup_pkg` (68) | `codegen_system_resolve_self_bringup_default` |
| `src/orchestration/planner.rs:3193` | `plan_system_keeps_instance_callbacks_remaps_and_parameter_overrides` (67) | `plan_system_keep_callback_remaps` |
| `src/orchestration/build.rs:323` | `generated_cargo_args_allow_cli_target_and_passthrough_overrides` (63) | `generated_cargo_args_allow_overrides` |
| `src/orchestration/build.rs:434` | `freshness_requires_crate_present_matching_stamp_and_not_forced` (62) | `freshness_requires_crate_match_stamp` |
| `src/cmd/check_workspace.rs:889` | `nros_check_workspace_rejects_application_pkg_with_rtos_in_deploy` (64) | `check_workspace_rejects_rtos_in_deploy` |
| `src/cmd/check_workspace.rs:929` | `nros_check_workspace_accepts_application_pkg_without_deploy_list` (64) | `check_workspace_accepts_no_deploy_list` |
| `src/orchestration/planner.rs:4372` | `collect_param_persistence_reads_block_defaults_and_last_wins` (60) | `collect_param_persistence_with_defaults` |
| `src/orchestration/sdk_store.rs:566` | `plan_picks_present_then_prebuilt_then_source_then_unavailable` (61) | `plan_picks_present_prebuilt_source` |
| `src/cmd/emit_package_xml.rs:428` | `emit_package_xml_for_component_pkg_from_cargo_ament_metadata` (60) | `emit_package_xml_from_cargo_metadata` |
| `tests/orchestration_e2e.rs:833` | `fixture_workspace_builds_generated_bare_metal_fibonacci_action_package` (70) | `fixture_builds_fibonacci_action_baremetal` |
| `tests/orchestration_e2e.rs:771` | `fixture_workspace_builds_generated_bare_metal_service_action_package` (68) | `fixture_builds_service_action_baremetal` |
| `tests/orchestration_e2e.rs:249` | `fixture_workspace_builds_and_boots_generated_freertos_package` (61) | `fixture_builds_boots_freertos` |
| `tests/orchestration_e2e.rs:1455` | `metadata_build_discovers_and_produces_missing_source_metadata` (61) | `metadata_build_discovers_missing_sources` |
| `tests/orchestration_e2e.rs:2173` | `deploy_zephyr_vendor_module_dry_run_resolves_and_substitutes` (60) | `deploy_zephyr_vendor_resolves_subst` |
| `tests/orchestration_generate.rs:423` | `multi_domain_nodes_emit_session_per_domain_and_route_by_session_idx` (67) | `multi_domain_emit_session_routes` |
| `tests/orchestration_generate.rs:603` | `generated_service_action_package_is_readable_by_cargo_metadata` (62) | `generated_service_action_readable_by_cargo` |
| `tests/orchestration_cli.rs:89` | `orchestration_plan_binds_callback_group_to_declared_scheduling_tier` (67) | `orchestration_plan_binds_tier` |
| `tests/orchestration_cli.rs:353` | `orchestration_metadata_command_flags_missing_component_export` (61) | `orchestration_metadata_flags_missing_export` |
| `tests/orchestration_self_bringup_cargo_metadata.rs:203` | `plan_system_still_reports_missing_metadata_for_pure_empty_pkg` (61) | `plan_system_reports_missing_metadata` |
| `tests/check_application_native_only.rs:49` | `nros_check_workspace_rejects_application_pkg_with_rtos_deploy` (61) | `check_workspace_rejects_rtos_deploy` |

### B.2 — nros + nros-node test renames (5 fns)

| file:line | current | proposed |
|---|---|---|
| `core/nros/src/node.rs:1482` | `runtime_adapter_rejects_duplicate_nodes_and_unknown_effect_entities` (67) | `runtime_adapter_rejects_unknown_entities` |
| `core/nros/src/node.rs:1579` | `component_api_records_multi_node_services_actions_and_defaults` (62) | `component_api_records_multi_node_services` |
| `core/nros-node/src/executor/tests.rs:641` | `test_atomic_sporadic_overrun_recorded_when_callback_exceeds_budget` (66) | `test_atomic_overrun_exceeds_budget` |
| `core/nros-node/src/executor/tests.rs:390` | `test_apply_time_triggered_schedule_dispatches_only_active_window` (64) | `test_time_triggered_dispatch_active_window` |
| `cli/rosidl-codegen/src/generator/common.rs:1257` | `action_goal_result_feedback_type_names_follow_rosidl_convention` (63) | `action_types_follow_rosidl_convention` |

### B.3 — nros-tests integration renames (3 fns)

| file:line | current | proposed |
|---|---|---|
| `testing/nros-tests/tests/phase212_macro_one_dep.rs:102` | `one_dep_component_pkg_compiles_without_explicit_nros_platform_dep` (65) | `one_dep_pkg_compiles_implicit_platform` |
| `testing/nros-tests/tests/phase212_k4_cyclonedds_descriptors.rs:58` | `nros_codegen_cyclonedds_descriptors_emits_c_for_std_msgs_int` (60) | `codegen_cyclonedds_emits_std_msgs` |
| `testing/nros-tests/tests/logging_smoke.rs:163` | `logging_smoke_threadx_linux_harness_captures_nros_log_stderr` (60) | `logging_smoke_harness_captures_stderr` |

### B.4 — Convention going forward

- Test name = `<unit>_<behavior>` (e.g., `fixture_builds_freertos`).
- Behavior details in docstring or test comments.
- Reserve long names for private helper fns only.
- **Filenames** (e.g. `phase212_o3_*.rs`) stay — useful roadmap cross-ref.

---

## Track C — Extract embedded source to templates

**Status**: DONE 2026-06-04. The `nros-c` generated Rust/C header bodies now
live in `packages/core/nros-c/templates/*.template`; generation runs from
`nros-build-helpers`, and `packages/core/nros-c/build.rs` is a thin wrapper.

**MED priority. Concentrated in 1 build.rs (nros-c).**

### C.1 — `packages/core/nros-c/build.rs` C/Rust header walls (3 large `format!()` blocks)

| line range | type | size | content | suggested template |
|---|---|---|---|---|
| `201–225` | `format!()` Rust source | 25 LOC | `NROS_EXECUTOR_MAX_HANDLES` / `LET_BUFFER_SIZE` / `SERVICE_DEFAULT_TIMEOUT_MS` / `MESSAGE_BUFFER_SIZE` / `EXECUTOR_OPAQUE_U64S` consts | `nros_c_config.rs.template` filled w/ `env::var()` lookups |
| `283–339` | `format!()` C header | 57 LOC | `nros_config_generated.h` w/ guard + 10+ `#define` + struct typedef | `nros_config_generated.h.template` (cmake `configure_file()` shape) |
| `362–418` | `format!()` C header (variant-exact) | 56 LOC | Per-variant opaque sizes + 15 `#define` + struct w/ u64 array | Consolidate w/ C.1's `.template` (single template w/ both branches) |

**Refactor**: Land `packages/core/nros-c/templates/` directory carrying the 2 `.template` files. `build.rs` reads + does simple `replace()` for placeholders. **Net**: ~140 LOC moved out of `build.rs` into separate files; `build.rs` becomes more reviewable.

### C.2 — Existing template infra to mirror

- `templates/overlay-board/*.template` (3 items) — overlay-board scaffolding generator.
- `cmake/*.in` (2 items) — C++ FFI bridging via `configure_file()`.
- `integrations/nuttx/*.in` (2 items) — NuttX external module integration.

**Pattern**: Existing template infra is healthy + actively used. nros-c is the drift candidate.

### C.3 — No action (intentional inline)

- `nros-macros/src/main_macro.rs::Framework::{Rtic,Embassy}` `quote! { ... }` blocks (~100 LOC each) — intentional locked design (Phase 212.N.9 + 216.B.3/C.3). Don't extract — framework dispatch evolves.
- `scripts/qemu/build-zenoh-pico.sh` heredocs (15 LOC each) — too small to template.
- User-facing CLI diagnostic heredocs (`check-version-lockstep.sh`, `arm-fvp-installer.sh`) — intentionally inline for maintainability.

---

## Track D — Phase-name leakage cleanup

**Status**: DONE 2026-06-04. The four listed phase-name leakages were renamed;
the acceptance grep returns no matches.

**LOW priority. Only 4 sites — convention holds overall.**

### D.1 — Production FFI export (CRITICAL within the track)

- `packages/core/nros-rmw-cffi/src/lib.rs:109` — `pub fn _phase_115_g4_anchor()` → rename `_c_stub_transport_vtable_anchor()`.
- **Risk**: FFI export name embeds roadmap phase ID. Renaming has zero downstream consumers (test-feature-gated stub).

### D.2 — Test module name

- `packages/core/nros/src/node.rs:1931` — `mod phase_216_a5_macro_emit` → rename `macro_emit_integration_test` or `dispatch_probe_macro_test`.

### D.3 — Test fn name

- `packages/core/nros/src/lib.rs:670` — `fn phase_212_n_12_node_names_resolve()` → rename `fn node_context_types_resolve()` (or similar describe-behavior shape).

### D.4 — Node-name string literal

- `packages/core/nros/src/node.rs:1940` — `const NAME: &'static str = "dispatch_probe_216_a5"` → `"dispatch_probe"`.

### D.5 — Convention going forward

- **OK** to retain in: test filenames (`phase212_*.rs`), doc-comments, roadmap docs themselves.
- **NOT OK** to use in: production fn / type / mod / static / const / C symbol names.

---

## Track E — Misc hygiene

**Status**: DONE 2026-06-04. The three listed board crates now carry local
`.gitignore` files for `/target/`.

**LOW priority. Small drift.**

### E.1 — Board crates missing `.gitignore`

- `packages/boards/nros-board-cffi/`
- `packages/boards/nros-board-common/`
- `packages/boards/nros-board-native/`
- **Action**: Add per-dir `.gitignore` w/ `/target/` (+ `/generated/` if codegen).

### E.2 — Pattern observations from audit (no action items)

- Hardcoded-paths: **clean** (only target/ build artifacts; source uses placeholders).
- Banned test patterns: **clean** (proper `nros_tests::skip!` usage; `eprintln!()` only used for informational logging).
- `todo!()` reachable at runtime: **2 documented Phase 216 skeletons** (intentional, gated).
- Duplicated platform-shim code: **no material duplication**.
- Blanket `#[allow]` on production crates: **5 instances, all FFI/CFFI-justified**.
- Magic numbers: **clean** (`NROS_HEAP_SIZE` etc. consolidated via `option_env!()`).
- Stale TODO/FIXME phase numbers: **clean** (no Phase 110–129 leftovers).
- `build.rs` env-var sprawl: **clean** (no consolidation bottleneck).

---

## Acceptance

- [x] **Track A complete**: 6 build-script files refactored per A.1–A.6. `cargo check -p zpico-sys -p nros-c -p nros-cpp` clean; build orchestration tests green.
- [x] **Track B complete**: 29 listed test fns plus 4 closure-scan test fns renamed; focused verification passed.
- [x] **Track C complete**: 3 `format!()` walls extracted to `packages/core/nros-c/templates/*.template`. `build.rs` < 500 LOC.
- [x] **Track D complete**: 4 phase-name leakages renamed. `grep -nE 'fn [a-z_]*phase\d|mod phase_?\d' packages/` returns 0 production hits.
- [x] **Track E complete**: 3 board `.gitignore`s added.

Closure verification on 2026-06-04:

- `cargo check -p zpico-sys -p nros-c -p nros-cpp`
- `source ./activate.sh && XDG_RUNTIME_DIR=/tmp just build`

## Notes / cross-refs

- Audit conducted 2026-06-04 on `main` @ `60a8ac052`.
- Methodology: 5 parallel `Explore`-mode read-only audits, one per antipattern category.
- Phase 214 (`phase-214-antipattern-audit-findings.md`) is a sibling doc covering an earlier audit wave (silent failures, codegen drift, unsafe doc) — distinct scope, no overlap.
- Track A.7 (top-level CMakeLists.txt RMW dispatch) deferred to an existing phase plan; no action this phase.
