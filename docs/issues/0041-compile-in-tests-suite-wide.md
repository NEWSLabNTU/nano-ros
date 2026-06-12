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
(Wave A native cmake/codegen DONE.)

**Reclassified (scan refined):** `phase212_o3/o4/o5`,
`phase212_n_freertos_run_plan_runtime`, and all of `phase212_h4_threadx` WERE
`#[ignore]`'d gates (inert). **As of 2026-06-12 (M-F.17 landed)** the first four
are un-`#[ignore]`d + renamed to behavioural names and now run live:
`board_agnostic_run_plan` (O.3), `pkg_index` (O.4), `nav2_compat` (O.5),
`threadx_corrosion_bringup` (H.4, 3 fns). They compile cargo/cmake at run time, so
they're added to the slow-compile `nextest.toml` override as STOPGAP exceptions
(alongside the `*_misuse` cases) pending build-stage-fixture conversion.
**`pkg_index` (O.4) CONVERTED (2026-06-13):** the `o4_pkg_index` BUILD_FIXTURE
(`compile-check-fixtures.sh` `stage_and_build` → `cargo build -p demo_entry`)
moves the compile to the build stage; `pkg_index.rs` now `require_compile_check`s
the stamp (build succeeded = package.xml pkg-index resolved `demo_bringup`) +
inspects the prebuilt `node_{a,b,c}` rlibs — runs in ~0s, no runtime compile.
Removed `binary(pkg_index)` from the slow-compile override. (`board_agnostic_run_plan`
O.3 + `nav2_compat` O.5 + `threadx_corrosion_bringup` H.4 remain STOPGAP.)
`freertos_run_plan_runtime` (O.1) stays `#[ignore]`d on issue 0045 (FreeRTOS
Entry-pkg `staticlib` panic-handler link path — NOT a compile-in-test concern).
`phase212_diagnostic_verbatim` (rustc + cmake verbatim-error checks) and
`cmake_platform_matrix` are NEGATIVE — the compile/configure MUST fail with exact
text → documented exceptions (fast-fail, can't be prebuilt).

**Wave A COMPLETE.** Also: `platform` is a FALSE POSITIVE — it spawns
`west --version`/`rustup target list`/`qemu --version` (availability probes), NOT
compilation. `phase215_e_board_import` is a `west build` (Zephyr/FVP) →
toolchain-gated, reclassified to Wave C (skips when ZEPHYR_BASE absent).

**Wave A also converted:** `phase212_d_workspace_metadata`'s lone cmake test →
`metadata_cpp` cmake fixture. `phase212_l9_cmake_fns` → `cmake_node_register_metadata`
(3 positives → `l9_register_cpp`/`l9_register_c`/`l9_deploy` cmake fixtures) +
`cmake_node_register_misuse` (2 negative configure-fail cases, exception).

**Wave B — freertos / threadx cross-build** (cross-build mechanism):
`orchestration_tiers_freertos` CONVERTED (→ `orch_tiers_freertos` cross-build
fixture, tests boot the prebuilt thumbv7m firmware in QEMU). Wave B update
(2026-06-12): `threadx_corrosion_bringup` (was `phase212_h4_threadx`, 3 fns) is
un-`#[ignore]`d + live (M-F.17 `nros plan` reads `[package.metadata.nros.component]`)
and joins the slow-compile override as a STOPGAP compile-in-test exception.
`freertos_run_plan_runtime` (was `phase212_n_freertos_run_plan_runtime`) stays
`#[ignore]`d on issue 0045.

**Wave C — zephyr** (west; heavy, gate on SDK): `phase212_h1_zephyr`,
`phase212_mf3_zephyr_self_pkg`, `integration_zephyr`.

**Wave D — esp-idf** (`idf.py`; heavy, gate on IDF): `phase212_m7_esp32_talker`,
`phase212_m7_esp32_listener`, `phase212_h5_esp_idf`, `integration_esp_idf`.

**Wave E — zpico / misc**: `zpico_drift_gate`, `zpico_build_matrix`, `platform`,
`orchestration_shared_state_xlang`.

Each conversion also: renames off any `phaseNNN_` tag, removes the binary from
the nextest timeout-override (a `binary()` matching nothing aborts the run), and
updates this table.

## Rename sweep COMPLETE (37/37)

All phase-numbered `nros-tests` test files are renamed to behavioral names
(AGENTS.md convention), with every Cargo.toml `[[test]]`, `.config/nextest.toml`
`binary()`, and justfile test-all env-gate ref updated; `cargo nextest list` is
clean. A latent dangling `[[test]]` (phase210_f4_shadowing) was fixed en route.

## Remaining compile-in-test conversions (Wave C/D — gated west/idf builds)

The 6 still-live offenders are heavy cross-toolchain builds, now renamed but not
yet fixture-converted (they `skip!` cleanly when the SDK is absent, so they don't
break lighter tiers):
- zephyr (`west build`): `cli_bringup_zephyr` + `board_import` **CONVERTED**
  (west-fixture mechanism `scripts/build/west-fixtures.sh` + `require_west_fixture`;
  built by `just zephyr build-fixtures`, gated on west/ZEPHYR_BASE, board_import
  also on the FVP SDK). The tests inspect the prebuilt build dir (baked artifacts /
  CMakeCache / boot zephyr.exe). `zephyr_self_pkg` is **deferred** — it generates
  its zephyr app in-test (fs::write Cargo.toml/lib.rs/prj.conf/CMakeLists), so its
  conversion needs the generated app promoted to a fixture template first.
- esp-idf (`idf.py build`): `cli_bringup_esp_idf`, `esp32_idf_talker_builds`,
  `esp32_idf_listener_builds` — **CONVERTED** (idf-fixture mechanism
  `scripts/build/idf-fixtures.sh` + `require_idf_fixture`; built by `just esp32
  build-fixtures`, gated on idf.py). BLOCKED at build time on
  [issue 0044](0044-esp-idf-platform-c-heap-symbols-undeclared.md) — a pre-existing
  esp-idf `_heap_start/_heap_end` compile failure the conversion exposed. Tests
  resolve the ELF + skip/deselect cleanly until 0044 lands.

**Conversion approach (validated, deferred — cost/value tradeoff):**

- esp-idf: stage the example, `source $IDF_PATH/export.sh`, then
  `NANO_ROS_ROOT=<repo-root> idf.py -B build -DNANO_ROS_SKIP_BOOTSTRAP=ON
  set-target esp32c3 && idf.py -B build build` → `<name>.elf`. (`-DNANO_ROS_SKIP_BOOTSTRAP=ON`
  is required: the bootstrap re-runs `tools/setup.sh` which fails offline even
  though submodules are populated. `NANO_ROS_ROOT` must be the repo root so the
  staged copy finds `integrations/nano-ros`.) **Verified** set-target +
  build-start succeed; a full esp32 build is ~7 min.
- zephyr: analogous `west build` of the bringup fixture (needs ZEPHYR_BASE).

**Why deferred:** each build is a multi-minute cross-toolchain compile, so wiring
all 6 into `build-test-fixtures` (esp32 / zephyr lanes, gated on idf.py /
ZEPHYR_BASE) adds ~30–45 min to the build stage — for tests that already `skip!`
cleanly when the SDK is absent (no lighter-tier breakage). The principled fix is
clear and the env path is proven; it is sequenced last, behind the (completed)
native conversions, because of the build-stage cost vs the gated-skip safety net.
The 6 are renamed off their phase numbers and remain in-test compiles, gated.
