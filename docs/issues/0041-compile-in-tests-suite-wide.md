---
id: 41
title: Suite-wide "compile inside tests" antipattern — convert to build-stage fixtures
status: open
type: tech-debt
area: testing
related: [issue-0034]
---

The "No compilation inside tests" convention (AGENTS.md → Testing Guidelines;
CLAUDE.md Practices) is violated by ~23 `nros-tests` binaries beyond the
host-integration lane that [issue 0034](0034-host-integration-31-preexisting-test-failures.md)
catalogued. Each spawns a compiler (`cargo`/`cmake`/`idf.py`/`west`/`c++`) at run
time, so the test wall-clock is dominated by compile time → spurious `timed out`
failures under load, build-lock serialization, and "builds" conflated with
"behaves".

**Fix per the convention:** move the build to the build stage as a fixture
(`scripts/build/compile-check-fixtures.sh` + `examples/fixtures.toml`), and have
the test assert / inspect / run the prebuilt artifact. Negative compile-FAIL +
rebuild-tracking cases (can't be prebuilt) split into a documented `*_misuse`
test kept on the `.config/nextest.toml` timeout-override.

## Build-stage fixture mechanisms available (`compile-check-fixtures.sh`)

| mechanism | use | resolver |
| --- | --- | --- |
| cargo compile-check | stage template + native `cargo check`, stamp | `require_compile_check(id)` |
| cargo build-fixture | stage template + native `cargo build`, binary | `require_compile_check_bin(id, rel)` |
| cross-build | stage + `cargo build --target <t> -p <pkg>` from a subdir | `require_compile_check(id)` → parent dir |
| cmake build-fixture | configure + build a C/C++ template into a persistent dir | `require_cmake_fixture(id, rel)` |
| cxx-syntax | `c++ -fsyntax-only` a snippet | `require_compile_check(id)` |
| cross cargo-check | `cargo check --target <t>` of an existing example (no link) | `require_compile_check(id)` |

## Converted (0034 lane — DONE)

`stm32f4_rtic_main_macro`, `native_entry_poc_boot`, `macro_one_dep_resolves`,
`native_main_macro_forms` (+`_misuse`), `native_orchestration_tiers` (+`_misuse`),
`cpp_multi_node_entry`, `cpp_entry_runtime`, `cpp_api_drift`, `c_mixed_workspace`,
`workspace_shadowing`, `stm32f4_embassy_main_macro`, `freertos_firmware_entry`.

**Wave A converted:** `cmake_add_subdirectory` → cmake build-fixture
`cmake_add_subdir` (asserts prebuilt `smoke`). `cmake_platform_matrix` is a
negative cmake-CONFIGURE-fail test (must fail) → kept as a documented exception
(configure is fast, no build).

## Remaining offenders, by wave

**Wave A — native cmake/codegen smoke** (fastest; cmake/compile-check fixtures):
`phase212_l9_cmake_fns` (cmake-configure cluster), `platform` (cmake/codegen),
`phase215_e_board_import`.

**Reclassified (scan refined):** `phase212_o3/o4/o5` are `#[ignore]`'d gates (inert,
not live). `phase212_diagnostic_verbatim` (rustc + cmake verbatim-error checks) and
`cmake_platform_matrix` are NEGATIVE — the compile/configure MUST fail with exact
text → documented exceptions (fast-fail, can't be prebuilt).

**Wave A also converted:** `phase212_d_workspace_metadata`'s lone cmake test →
`metadata_cpp` cmake fixture. `phase212_l9_cmake_fns` → `cmake_node_register_metadata`
(3 positives → `l9_register_cpp`/`l9_register_c`/`l9_deploy` cmake fixtures) +
`cmake_node_register_misuse` (2 negative configure-fail cases, exception).

**Wave B — freertos / threadx cross-build** (cross-build mechanism):
`orchestration_tiers_freertos` CONVERTED (→ `orch_tiers_freertos` cross-build
fixture, tests boot the prebuilt thumbv7m firmware in QEMU). Remaining:
`phase212_n_freertos_run_plan_runtime`, `phase212_h4_threadx`.

**Wave C — zephyr** (west; heavy, gate on SDK): `phase212_h1_zephyr`,
`phase212_mf3_zephyr_self_pkg`, `integration_zephyr`.

**Wave D — esp-idf** (`idf.py`; heavy, gate on IDF): `phase212_m7_esp32_talker`,
`phase212_m7_esp32_listener`, `phase212_h5_esp_idf`, `integration_esp_idf`.

**Wave E — zpico / misc**: `zpico_drift_gate`, `zpico_build_matrix`, `platform`,
`orchestration_shared_state_xlang`.

Each conversion also: renames off any `phaseNNN_` tag, removes the binary from
the nextest timeout-override (a `binary()` matching nothing aborts the run), and
updates this table.
