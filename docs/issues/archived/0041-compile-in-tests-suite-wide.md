---
id: 41
title: Suite-wide "compile inside tests" antipattern — convert to build-stage fixtures
status: resolved
type: tech-debt
area: testing
related: [issue-0034]
resolved_in: "2026-06-13 (native O.3/O.4/O.5 + threadx Wave B + zephyr/esp Wave C/D + zephyr_self_pkg)"
---

**RESOLVED (2026-06-13).** Every compile-in-test offender is now a build-stage
fixture (native `compile-check-fixtures.sh`, cross-build, cmake, west, idf
mechanisms). The slow-compile `nextest.toml` override holds ONLY the two
NEGATIVE cases (`native_orchestration_misuse`, `native_main_macro_misuse` —
compile-FAIL / rebuild-tracking, can't be prebuilt → permanent documented
exceptions). The heavy SDK-gated lanes (zephyr west, esp-idf) build in their
`just zephyr/esp32 build-fixtures` recipes and `[SKIPPED]`/deselect when the SDK
is absent. See the per-wave notes below for the conversion record.

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
Removed `binary(pkg_index)` from the slow-compile override. **`nav2_compat` (O.5)
CONVERTED (2026-06-13):** the `o5_nav2_compat` BUILD_FIXTURE builds the Entry pkg's
`build.rs` (drives `generate_run_plan` via `play_launch_parser`) → `run_plan.rs` +
`nros-plan.json`; `nav2_compat.rs` inspects the prebuilt codegen (register calls +
`<arg>`/`<remap>`/`<include>` evidence), Placeholder-stub → skip. Required two
`stage_and_build` upgrades: an optional **manifest-subdir** 3rd `id:src:dir` field
(`demo_entry` is excluded from the fixture root workspace) + a **`@NROS_CLI_ROOT@`**
placeholder rewrite (→ `packages/cli`, the in-tree nros-build crate). Removed
`binary(nav2_compat)` from the override.
**`board_agnostic_run_plan` (O.3) CONVERTED (2026-06-13):** the `o3_board_agnostic`
BUILD_FIXTURE does `cargo build -p posix_entry` (the host Board leg) in the build
stage; `board_agnostic_run_plan.rs` now (1) reads the committed fixture and asserts
`posix_entry/build.rs` == `freertos_entry/build.rs` byte-identical (the codegen-driver
identity — no build needed), then (2) `require_compile_check`s the stamp + inspects
the prebuilt `out/run_plan.rs` (`pub fn run_plan` + `shared_node_pkg::register`,
Placeholder → skip). Runs in ~0.02s, no runtime cargo. Required a 4th
`id:src:dir:pkg` **pkg-name** field on `stage_and_build` (O.3 builds `posix_entry`,
not the default `demo_entry`). The strongest leg — byte-identical `run_plan.rs`
across the POSIX *and* the `thumbv7m-none-eabi` freertos emit — needs the freertos
Entry pkg cross-built as a build-stage fixture; that is **Wave B** (cross-build
mechanism, gated on the arm toolchain) and is reported (not silently dropped) by
the test. Removed `binary(board_agnostic_run_plan)` from the override.
(`threadx_corrosion_bringup` H.4 remains STOPGAP.)
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
**`threadx_corrosion_bringup` (3 fns) CONVERTED (2026-06-13):** provisioned the
prereqs via `nros setup` (`--tool corrosion`, `--source threadx`,
`--source threadx-netxduo`) and moved the cmake configure + build + the
Corrosion-imported Rust crate compiles to the build stage. Two cmake fixtures:
`threadx_bringup` (host `threadx_app` — `build_cmake_fixture` now puts the
provisioned `~/.nros/sdk/corrosion` on `CMAKE_PREFIX_PATH`, so the codegen helper
imports the Rust components and the REAL non-stub entries link) and
`threadx_bringup_rv64` (CONFIGURE-ONLY rv64 codegen sibling — gated on
`riscv64-unknown-elf-gcc` + the ThreadX/NetX trees). The fixture
`threadx_app/CMakeLists.txt` lost its `@NANO_ROS_ROOT@` placeholder for a
relative `_NROS_ROOT` (6-up), mirroring the `l9_register_*` fixtures, so the
cmake-fixture mechanism points `-S` straight at it. `threadx_corrosion_bringup.rs`
now inspects the prebuilt codegen artifacts + runs the prebuilt host binary
(~0.003s/fn, no runtime cmake). Removed `binary(threadx_corrosion_bringup)` from
the override.

**Native + cross-build POSITIVE conversions COMPLETE.** The slow-compile
`nextest.toml` override now holds ONLY the two NEGATIVE cases
(`native_orchestration_misuse`, `native_main_macro_misuse` — compile-FAIL /
rebuild-tracking, can't be prebuilt → permanent documented exceptions). Every
positive compile-in-test is a build-stage fixture.

**Wave C/D host-provision wiring FIXED (2026-06-13):** the zephyr west /
esp-idf fixtures were already converted to the `west-`/`idf-fixture`
mechanisms but had skipped on hosts that *had* the SDKs, because the build
scripts couldn't discover them. `west-fixtures.sh` now resolves `ZEPHYR_BASE`
via the same workspace ladder the `just zephyr` recipes use (in-repo →
sibling `../nano-ros-workspace`), `zephyr-ci.just` passes its resolved
workspace through, and `idf-fixtures.sh` auto-discovers the in-repo
`esp-idf-workspace/esp-idf`. Verified on a provisioned host: west builds
`west_bringup_zephyr` (native_sim) + `west_board_import` (FVP); idf builds the
3 esp32c3 ELFs (issue 0044 resolved → clean compile). All Wave C/D
fixture-consuming tests pass (`cli_bringup_zephyr` boots native_sim,
`board_import`, `cli_bringup_esp_idf`, `esp32_idf_{talker,listener}_builds`).
The west/idf builds are heavy + SDK-gated, so they live in the
`just zephyr build-fixtures` / `just esp32 build-fixtures` lanes (not the
native `compile-check-fixtures.sh` sweep); the tests `[SKIPPED]`/deselect when
the SDK is absent.

**`zephyr_self_pkg` (2 fns) CONVERTED (2026-06-13) — last residual cleared.**
The M-F.3 self-pkg shim test generated its bringup app in-test (`fs::write` of
Cargo.toml/lib.rs/prj.conf/CMakeLists, then `west build`). Promoted both app
layouts to committed templates (`fixtures/zephyr_self_pkg/{self,sibling}/`):
the self form (`nros_system_generate(.)`) and the sibling form
(`caller/`'s `nros_system_generate(alpha_pkg)` resolving the sibling self-pkg).
`west-fixtures.sh` gained a BAKE-ONLY block — the shim contract is the
configure-time bake (`nros-system/system_{config.h,main.c}`), NOT a full ELF
link (out of scope, as in the original), so it stamps on bake-exists after the
(expected-to-fail-at-link) `west build`. The test inspects the prebuilt bake;
runs ~0.002s, no runtime west. The `justfile` test-all west-gate now uses the
same workspace-discovery ladder so a sibling-layout host RUNS these instead of
deselecting them. **Every compile-in-test offender is now converted** — only
the two NEGATIVE `*_misuse` cases remain on the nextest override (permanent).

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
