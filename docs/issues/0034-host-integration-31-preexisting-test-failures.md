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
| phase212_n9_main_macro_forms | 8 | form1/2/3/4 `*_compiles`, `unknown_board_emits_compile_error`, `custom_tasks_*` (2), `rebuilds_on_launch_xml_touch` |
| orchestration_tiers_native | 5 | `instance_identity_mismatch_is_a_compile_error`, `multi_tier_binary_boots_into_run_tiers`, `…runs_both_tiers_with_router`, `multi_tier_main_macro_emits_run_tiers_and_compiles`, `single_tier_system_takes_the_legacy_boardentry_run_path` |
| phase212_n12_cpp_api_drift | 2 | `declared_node_typed_helpers_compile`, `rclcpp_node_options_and_component_factory_compile` |
| native_entry_poc_boot (was phase212_n_entry_poc_runs) | 2 | CONVERTED to fixture-consuming (#0034) — entry-poc is now a build-fixture |
| phase223_c_mixed_workspace | 2 | `c_node_pkg_links_into_cpp_entry_template`, `c_node_pkgs_link_into_c_entry_template` |
| cpp_multi_node_entry | 1 | `multi_node_workspace_cpp_configures_and_builds` |
| phase212_j_launch | 1 | `nros_launch_spawns_components` |
| phase212_l9_cmake_fns | 1 | `nano_ros_application_rejects_embedded_deploy` |
| phase210_f4_shadowing | 1 | `workspace_std_msgs_shadows_ament_in_consumer_binary` |
| phase212_m12_example_shape | 1 | `every_example_leaf_has_package_xml` — **cause known: `78ac799ee`** |
| phase212_macro_one_dep | 1 | `one_dep_pkg_compiles_implicit_platform` |
| stm32f4_rtic_main_macro (was phase216_b) | 1 | `rtic_main_macro_expansion_builds` — CONVERTED to fixture-consuming (#0034 antipattern) |
| phase216_c_embassy_main_macro_expansion | 1 | `embassy_main_macro_expansion_compiles` |
| phase235_a_cpp_entry_runtime | 1 | `cpp_entry_runtime_publishes_live_samples` |
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

**Real — owner triage (2).**
- **m12 `every_example_leaf_has_package_xml`** — genuine gap: `examples/stm32f4/
  rust/*_pkg` (Cargo node-libs) were added without `package.xml`. The test
  correctly flags it; fix is adding the right `package.xml` (stm32f4 owner).
- **j_launch `nros_launch_spawns_components`** — `nros launch` hits the top-level
  CLI usage (`Usage: nros <COMMAND>`), i.e. launch-subcommand CLI drift.

**CI-ENV-ONLY — pass locally (3).** `zenoh_archive_symbols`, `zenoh_header_parity`,
`zpico_build_matrix` PASS in the dev env but failed in CI run 27385404078. They
inspect built zpico/zenoh archive symbols + header parity, so the CI build
context (feature resolution / link policy) differs. Re-check on the next CI run
now that the env leak (#29) is gone — they may already be green.

A verify run after the timeout + l9 fixes is expected to drop the count from 31
to roughly the m12 + j_launch (+ possibly the 3 CI-env) failures.

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
