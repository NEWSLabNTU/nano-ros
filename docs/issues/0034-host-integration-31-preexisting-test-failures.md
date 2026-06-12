---
id: 34
title: host-integration surfaces 31 pre-existing nros-tests failures (lane now runs honestly)
status: open
type: bug
area: testing
related: [issue-0029, issue-0025, issue-0027]
---

With [issue 0025] (all prereqs via `nros setup`) and [issue 0029] (disk ENOSPC +
the compile-failure masking bug) fixed, the `host integration-tests` lane runs
`just test-integration` to completion **honestly for the first time** — and that
exposes **31 real (non-`[SKIPPED]`) nros-tests failures**. They were invisible
before because the lane was always cancelled (main-branch push churn), crashed
on ENOSPC, or *false-greened*: the pre-masking-fix recipe counted only junit
`<failure>` cases, so a test binary that failed to **compile** produced zero
cases and tallied as "0 real failures → pass".

**These are NOT regressions from the 0025/0029 work.** Confirmed by reproducing
on a plain `main` checkout in the dev env, e.g.:

```
cargo nextest run -p nros-tests --test phase212_m12_example_shape
  FAIL every_example_leaf_has_package_xml   # 1 failed, 6 passed
```

`every_example_leaf_has_package_xml` fails because commit `78ac799ee`
("fix(stm32f4): resolve .bss overflow (#24) + rtic defmt timestamp (#28)") added
`examples/stm32f4/rust/{talker,listener,service_server,service_client,action_server,action_client}_pkg`
(+ `listener_pkg_rtic`) **without a `package.xml`**, and the test only allowlists
the older `listener-embassy`. A real repo-state regression, independent of CI.

The lane is intentionally left **honest-red** until these are triaged + fixed by
their phase owners (masking them green would undo the 0029 masking fix).

## The 31 failures (run 27385404078, junit)

Several are C/C++ cases (`cpp_*`, `phase223_c_mixed_workspace`,
`phase235_a_cpp_entry_runtime`) that may relate to the issue-0027 nros-c posix
clash and/or the C/C++ extras fixtures the lane no longer builds (#29); the
macro/compile + symbol cases reproduce standalone. Each needs its owner's
triage.

| binary | n | tests |
|---|---|---|
| native_main_macro_forms (was phase212_n9, 4 pos) | 4 | CONVERTED — 4 positive forms → compile-check fixtures (#0034) |
| native_main_macro_misuse (was phase212_n9, 4 neg) | 4 | compile-FAIL + rebuild — documented exception (can't prebuild a failing/rebuild check); kept on nextest timeout override |
| native_orchestration_tiers (was orchestration_tiers_native, 4) | 4 | CONVERTED — build-fixtures (orch_tiers_multi/single binaries), tests run prebuilt (#0034) |
| native_orchestration_misuse (1 neg) | 1 | compile-FAIL exception (instance-identity); on nextest timeout override |
| cpp_api_drift (was phase212_n12) | 1+2 | static lint PASS; 2 g++ snippets → cxx-syntax build-fixtures, SKIP pending pre-existing C++ drift fix (#0034) |
| native_entry_poc_boot (was phase212_n_entry_poc_runs) | 2 | CONVERTED to fixture-consuming (#0034) — entry-poc is now a build-fixture |
| c_mixed_workspace (was phase223) | 2 | CONVERTED — cmake build-fixtures (c_mixed/pure_c), tests assert prebuilt robot_entry (#0034) |
| cpp_multi_node_entry | 1 | CONVERTED — cmake build-fixture `cpp_robot_entry`, test inspects prebuilt (#0034) |
| phase212_j_launch | 1 | `nros_launch_spawns_components` — **test removed** (`nros launch` unsupported, RFC-0027) |
| phase212_l9_cmake_fns | 1 | `nano_ros_application_rejects_embedded_deploy` |
| phase210_f4_shadowing | 1 | `workspace_std_msgs_shadows_ament_in_consumer_binary` |
| phase212_m12_example_shape | 1 | `every_example_leaf_has_package_xml` — **cause known: `78ac799ee`** |
| macro_one_dep_resolves (was phase212_macro_one_dep) | 1 | CONVERTED — build-stage compile-check fixture + `.compile-ok` stamp (#0034) |
| stm32f4_rtic_main_macro (was phase216_b) | 1 | `rtic_main_macro_expansion_builds` — CONVERTED to fixture-consuming (#0034 antipattern) |
| phase216_c_embassy_main_macro_expansion | 1 | `embassy_main_macro_expansion_compiles` |
| cpp_entry_runtime (was phase235_a) | 1 | CONVERTED — runs the prebuilt `cpp_robot_entry` fixture (shares cpp_multi_node's build) |
| zenoh_archive_symbols | 1 | `zenoh_archive_wrapper_impl_parity` |
| zenoh_header_parity | 1 | `posix_canonical_header_matches_link_policy` |
| zpico_build_matrix | 1 | `zpico_posix_archive_carries_link_feature_symbols` |

## Triage + progress (2026-06-12)

Local reproduction split the 31 into four categories:

**Timeout class (~22) — in-test compilation (convention violation).**
orchestration_tiers (5), phase212_n9_main_macro_forms (8),
phase212_n_entry_poc_runs (2), phase212_macro_one_dep (1), phase216_b/c (2),
phase210_f4_shadowing (1), and the cpp_* compile tests (cpp_multi_node_entry,
phase212_n12_cpp_api_drift, phase223_c_mixed_workspace, phase235_a_cpp_entry_runtime)
**shell out to cargo/nros to build a generated crate at run time** (e.g.
`phase212_n9` makes 21 build calls, `phase235_a` 11). A COLD build exceeds the
60s nextest default kill (`slow-timeout 30s × terminate-after 2`); measured, an
`orchestration_tiers_native` case takes **72–94 s**.

This is the documented anti-pattern **"No compilation inside tests"** (AGENTS.md
→ Testing Guidelines; CLAUDE.md Practices). Two responses, in order of merit:
- **Stopgap (masks it):** a nextest timeout override (`120s × 4`) for those
  binaries in `.config/nextest.toml` lets them pass (orchestration_tiers 5/5
  once lifted) — but it only hides the wall-clock, keeps the build-lock
  serialization, and conflates "builds" with "behaves".
- **Durable fix (the convention):** move the build to the **build stage** — add
  the project as a row in `examples/fixtures.toml` (or a build-lane target) so
  `build-test-fixtures` compiles it once; rewrite the test to assert the
  prebuilt artifact / inspect it. The "does-it-compile?" signal becomes a
  green/red **build**, not a timeout-prone test. Rename the binary off its phase
  number at the same time (AGENTS.md → Testing Guidelines).

  **Progress:** `phase216_b_rtic_main_macro_expansion` → `stm32f4_rtic_main_macro`
  is the first conversion — it now resolves the prebuilt `stm32f4-rs-rtic-example`
  fixture instead of running `cargo check` (30 s → 0.002 s; also fixed the stale
  `build_rtic_talker()` resolver that pointed at a non-existent binary name).
  Each remaining compile-intent test follows the same shape; negative
  compile-*fail* cases (n9's `*_emits_error`, `unknown_board_emits_compile_error`)
  can't be fixtures (they must fail to build) — relocate those to a dedicated
  compile-fail harness excluded from the timeout-sensitive suite.

**l9 rename drift (1) — FIXED.** `nano_ros_application_rejects_embedded_deploy`
asserted the old "native-only"/L.2 wording; the fn is now a shim →
`nano_ros_entry` with board-centric wording. Updated the drift-guard; l9 5/5 pass.

**FIXED — real failures (2).**
- **m12 `every_example_leaf_has_package_xml`** — genuine gap: `examples/stm32f4/
  rust/*_pkg` (Cargo node-libs) were added without `package.xml`. Fixed by
  adding a minimal `ament_cargo` `package.xml` to the 7 Node-lib pkgs (deps =
  `nros` only, `<name>` matching the crate + node-class prefix) and carving
  `listener-embassy` into `UNMIGRATED_PREFIXES` alongside `talker-embassy` (same
  Embassy variant, `skip_build`/non-linking, known-issue #13 — it had been
  omitted). m12 now 7/7 pass.
- **j_launch (whole file removed).** `nros launch` is not a supported verb:
  RFC-0027 (CLI-verb note, Phase 222) records that `nros build`/`nros run` were
  **removed** — `nros` is now provisioner + codegen + metadata only, and a host
  `launch` verb was never part of that surface (runtime is native: `cargo run`,
  `west run`, `probe-rs run`). So `phase212_j_launch.rs` (both
  `nros_launch_spawns_components` + `nros_launch_detach_returns_pid_file`) tests
  a non-existent feature and was deleted rather than skip-gated. Audit confirmed
  it was the ONLY test invoking `nros launch`; all other test `build`/`run`
  calls are native `cargo`/`west`/`pio`, which the RFC sanctions.

**FIXED — CI-env (3).** `zenoh_archive_symbols`, `zenoh_header_parity`,
`zpico_build_matrix` PASS in the dev env but failed in CI. Root cause: all three
resolve the **zenoh-posix fixture** (`target-zenoh-fixture-posix/`, built by
`just build-zenoh-posix-fixture` / `build-test-fixtures`) and **panic when
absent**. The light host-integration lane builds only the core rust + workspace
fixtures, not the zenoh-posix one — so they failed in CI (no fixture) but passed
locally (fixture present from a prior `build-test-fixtures`). Fixed by gating the
missing-fixture path on `NROS_FIXTURES_OPTIONAL`: `skip!` in the light lane (it
already sets the flag), hard-fail in the full `test-all` tier so a real
header/symbol regression still surfaces. Verified both paths.

**Progress: 31 → 7.** Timeout-class conversions + override (team), j_launch
removal, m12 + l9 fixes, and this CI-env skip-gate clear 24 of the 31. Remaining
**7**: the cpp compile-in-tests (`phase212_n12_cpp_api_drift` ×2,
`phase223_c_mixed_workspace` ×2, `cpp_multi_node_entry`) + `phase216_c` embassy
(blocked on the non-linking example, #13) + `phase210_f4_shadowing` (slow
consumer-binary build). All are the remaining compile-in-test conversions /
blocked example — team/owner domain.

### Sibling lane — NuttX cpp talker `div_t` clash (platform-ci e2e, run 27393704883)

Separate from the host-integration nextest classes above (this is the
*platform-ci* nuttx e2e cell building real fixtures, not a nextest timeout): the
honest e2e run surfaces a genuine **cpp** compile clash. arm-none-eabi-g++
building `examples/qemu-arm-nuttx/cpp/talker/.../nros-entry/main.cpp` fails with
`conflicting declaration 'typedef struct div_t div_t'` — `arm-none-eabi/include/stdlib.h`
(newlib) and NuttX's own `stdlib.h` both on the cpp entry's include path.
issue-0027 made the NuttX sysroot win for the *C* message-lib path; the **cpp**
entry's cc-rs `-I third-party/nuttx/nuttx/include` still also pulls newlib's libc
headers (no SYSTEM-include precedence as the C path got). Fix is the cpp-entry
analogue of 0027 #1 (NuttX sysroot precedence for the C++ FFI compile), owner =
nros-cpp / NuttX C++ header.
