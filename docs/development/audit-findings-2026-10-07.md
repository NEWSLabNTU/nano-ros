# Audit findings — 2026-10-07 — gate-reach re-audit (phase-472's method, re-run)

The periodic re-run [phase-472](../roadmap/archived/phase-472-gate-reach-sweep.md)
asks for ("should be re-run whenever a class fix lands"). Prior records:
[2026-09-28](audit-findings-2026-09-28.md) and
[2026-10-01 re-run](audit-findings-2026-10-01-rerun.md). Tree: `origin/main` at
`b928293215`, one worktree, every submodule initialised (non-recursive), the
in-tree CLI and `nros-launch-resolve` built fresh.

## Method

One question per gate — *can it fail on the case it names?* — answered by
mutation:

- A scratch harness (not committed) applied each mutation, **refused to run the
  gate unless `git status` showed it**, ran the gate the way its lane does
  (`just check <recipe>`), restored, and **refused to continue unless the tree
  was back to the baseline**. New files were `git add -N`'d so index-reading
  gates see them (`force_add` for gitignored paths a gate is about); planted
  build artefacts used gitignored paths.
- rc≠0 → **CLEAN**. rc=0 → a **control**: the same defect where the gate
  demonstrably reads. Control fails → **HOLE** (confirmed). Both pass → the
  mutation was wrong and was rewritten.
- Submodule-side mutations (zenoh-pico, play_launch) were applied and restored
  by hand with `git -C <submodule>`. Mutations that the CLI embeds (codegen
  packs, the entity census) were measured with the CLI **rebuilt with the
  mutation**, then rebuilt clean (`cli-fresh` OK afterwards).
- Baseline: `just check fast` green on the untouched tree (398 gates, 3
  ledger SKIPs), and green again after this PR's fixes.

Rewrites, not counted (recorded so the method is auditable): `ci-doc-workflow-refs`
(first citation landed under the doc's own "Historical workflow names"),
`decoupling` (named a C-only crate), `deploy-board-resolves` (legacy `nros.toml`
outside the population), `workflow-indexed-apt` (package not indexed),
`rust-targets-covered`/`-installed` (triple already listed / missing column 2),
`knob-ends`/`knob-single-reader` (knob outside the stated scope),
`lane-contracts` (test in a crate `test-unit` excludes), `dds-isolation-symmetry`
(first site had no ros2 peer), `zenoh-feature-off-compile` (first edit hit the
gate's own selftest anchor — invalid), `template-copy-out` (the template is a
CLI stamp input; re-run with `NROS_SKIP_STALE_CHECK=1`), `rmw-descriptors`
(a retired field). `default-gates-run-somewhere`'s first control named a recipe
that IS on a schedule.

## Scope and harvest

1. **Changed since 2026-09-28**: the prescribed `git log --since=2026-09-28 …
   'scripts/check-*' 'scripts/build/check-*' just/check/` **plus
   `scripts/check/check-*`**, a gate directory the prescribed pathspec does not
   match (5 of its 16 gates changed in the window). 250 scripts (30 added, 4
   since deleted). `scripts/build/check-skip.sh` and
   `check-store-corrosion.sh` are helpers and are audited in scope 2.
2. **Shared helpers** in `scripts/lib` + `check-skip.sh` +
   `check-store-corrosion.sh`, each weakened and run against the whole
   `check fast` lane.
3. **Random sample of 20 gates outside scope 1**: `random.seed(20261007)`,
   `random.sample(pool, 20)` over the 107 tracked gate scripts not in scope 1.
4. **Lane reach** for every scope-1 gate, and a mutation of the meta-gate.

## Counts

| scope | rows | CLEAN | HOLE | N/A | other |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 — changed gates (248 gates; facets get their own row) | 255 | 224 | 24 (23 gates) | 7 | — |
| 2 — helpers | 17 | 12 | — | — | 5 UNNOTICED |
| 3 — random sample (seed 20261007) | 20 | 16 | 0 | 4 | — |
| 4 — meta-gate | 1 | — | 1 | — | + 7 gates in no workflow |

Of the 24 scope-1 holes, 2 are **fixed in this PR** (`issue-index` ratchet,
`skippable-tests-tolerant`); the rest are filed by class as **1735** (population,
W1/W5), **1736** (one-spelling fingerprints, + a live stale-toolchain literal),
**1737** (evidence/grain, W3/W6/W8) and **1738** (shell one-line compounds).
All 5 unnoticed helper mutations are **fixed in this PR** on the helpers
(`exemptions`, `check_skip`, `check-skip.sh`) except `check-store-corrosion.sh`
(no self-test; in **1739**). Lane reach and the meta-gate: **1739**.

## Fixed in this PR (each re-proved by its mutation, now rc≠0)

| fix | on | mutation, before → after |
| --- | --- | --- |
| `check-issue-index`: the frozen digest count is a ratchet forced DOWN via `ratchet.judge` (W9), with normal-path rise/fall/equal rows | `scripts/lib/ratchet.py` | delete one digest: 0 → 1; disarm the verdict: selftest fires (rc 1) |
| `check-skippable-tests-tolerant` reads workflows + composite actions (`workflow_commands.ci_files`, W1), both YAML `run:` spellings, with a selftest row | `scripts/lib/workflow_commands.py` | bare `cargo nextest run --test xrce_ros2_interop` in gate.yml: 0 → 1; `_ci_sources` → `[]`: selftest fires |
| `Exemptions.check()` runs the helper's own `self_test()` once per process | `scripts/lib/exemptions.py` | neighbour test disabled: `check fast` 0 → 1 (`one-producer-per-tool` alone: 1) |
| the gate runner (`check fast` / `check build`) runs the ledger self-tests before reporting; `check_skip.self_test` drives `unverified()` too | `scripts/build/run-gates-parallel.sh`, `scripts/lib/check_skip.py` | `unverified()` short-circuited / ledger write removed / strict branch removed: `check fast` 0 → 2 each |

## Scope 1 — every gate changed since 2026-09-28

Verdicts per row; the hole details and their class are in the issues named above, and the per-gate rule/mutation text is reproduced here so the run can be repeated.

| gate | rule | mutation | rc | control | ctl rc | verdict |
| --- | --- | --- | ---: | --- | ---: | --- |
| `action-client-arena-budget` | knob==0 with action-client machinery linked fails (image-level) | N/A: needs re-linked images to mutate (2 prebuilt images present, rc=0 OK) | — | — | — | **N/A** |
| `activate-shells` | activate.sh/.fish run to completion in every claimed shell | unmatched glob loop before the final unset in activate.sh (zsh aborts) | 1 | — | — | **CLEAN** |
| `allocator-never-waits` | every RTOS alloc primitive taking a wait option passes NO_WAIT | threadx_hooks.c tx_byte_allocate TX_NO_WAIT -> TX_WAIT_FOREVER | 1 | — | — | **CLEAN** |
| `archive-lang-items` | at most one Rust archive per link line defines the global allocator | N/A: no CMake image link line built here; [SKIPPED] NOT VERIFIED via ledger (designed) | — | — | — | **N/A** |
| `artifact-identity-budget` | one workspace, one artifact identity per crate | N/A: no examples/workspaces/mixed build tree; [SKIPPED] NOT VERIFIED via ledger (designed) | — | — | — | **N/A** |
| `atomic-sync-writes` | sync-owned files are written via atomic_write, never fs::write | metadata_build.rs atomic_write(path, contents) -> std::fs::write | 1 | — | — | **CLEAN** |
| `backend-dispatch-declared` | every RMW backend crate enables exactly its dispatch feature | xrce-cffi drops in-place-dispatch | 1 | — | — | **CLEAN** |
| `board-build-wiring` | a board build.rs using cc::Build also uses nros_board_common | append a cc::Build use to nros-board-nuttx/build.rs (no common use) | 1 | — | — | **CLEAN** |
| `board-cargo-config-shape` | every board cargo_config blob is cargo-shaped TOML | mps2-an385 blob `runner =` -> `runnerr =` | 1 | — | — | **CLEAN** |
| `board-facts-delivery` | every Corrosion consumer delivers board facts (nros_board_facts_env) | delete nros_board_facts_env(nros_ws_runtime-static) in NanoRosRuntimeCrate.cmake | 1 | — | — | **CLEAN** |
| `board-name-reach` | a TARGET-pinned board's name names a part/machine | freertos-posix overlay gains a linker-script pin | 1 | — | — | **CLEAN** |
| `board-vocabulary` | every board=/deploy= value a workspace states resolves to a known board | esp32-c3 listener system.toml board -> esp32-c3-baremetalx | 1 | — | — | **CLEAN** |
| `book-identifiers` | every code identifier the book quotes exists in the tree | book quotes `nros_audit_never_existed_fn` | 1 | — | — | **CLEAN** |
| `book-links` | every relative link in the book resolves | dead relative link in book/src/introduction.md | 1 | — | — | **CLEAN** |
| `build-profile-literals` | no build site names a cargo profile literal | `cargo build --release` in a just/check recipe body | 1 | — | — | **CLEAN** |
| `build-script-path-resolution` | a build.rs resolving a path-valued SDK var uses nros_build_paths | bare std::env::var("THREADX_DIR") in nros-board-threadx/build.rs | 1 | — | — | **CLEAN** |
| `build-tool-verbs-exempt` | every nros verb cmake invokes is exempt from the workspace check | drop RmwDispatch from ws_cmd_name's ws-build arm | 1 | — | — | **CLEAN** |
| `build-type-spelling` | entries/boards/providers use nros_cargo/nros_cmake, interfaces ament_cmake | esp32-c3 listener entry build_type nros_cargo -> ament_cargo | 1 | — | — | **CLEAN** |
| `c-executor-remove-coverage` | every C executor add verb has a remove verb (or a reason) | rename nros_executor_remove_timer | 1 | — | — | **CLEAN** |
| `c-knob-guard-order` | a `#if KNOB < 1` guard comes after the knob's default define | copy the ZPICO_MAX_QUERYABLES guard above its #ifndef default | 1 | — | — | **CLEAN** |
| `capability-conditionals` | a when.capability conditional names a declared capability and partitions | FreeRTOS ZENOH_ORIN_SPE arm -> ip_stack = true (arms no longer partition) | 1 | — | — | **CLEAN** |
| `capability-flavour-guards` | every capability-gated std use is declared | nros-core: a std:: use gated on a capability feature | 1 | — | — | **CLEAN** |
| `capability-slot-counts` | *ServiceServers server-field count == its slot constant | LifecycleServiceServers gains a 6th server field | 1 | — | — | **CLEAN** |
| `cargo-custom-command-depfile` | every cmake custom command running cargo carries DEPFILE | delete DEPFILE in NanoRosGenerateInterfaces.cmake | 1 | — | — | **CLEAN** |
| `cargo-dir-knob-key` | two images differing only in a derived knob never share a cargo dir (nros_knob_key_fields) | road 1 (the resolver registry NROS_RESOLVED_*) appends the knob NAME without its value | 0 | the same break on road 2 (environment) | 1 | **HOLE** |
| `cargo-dir-knob-key-sites` | every nros_shared_cargo_dir call site appends nros_knob_key_fields | nros-nuttx.cmake: knob fields replaced by empty set | 1 | — | — | **CLEAN** |
| `cbindgen-pin` | cbindgen requirement stays exact (=x.y.z) in one place | root Cargo.toml cbindgen "=0.29.3" -> "0.29" | 1 | — | — | **CLEAN** |
| `cc-build-policy` | every file constructing cc::Build names nros-cc-flags | cc::Build::new() appended to nros-board-mps2-an385/build.rs | 1 | — | — | **CLEAN** |
| `cc-header-deps` | every cc-rs compile emits the compiler's header deps | threadx-linux build.rs drops emit_header_deps | 1 | — | — | **CLEAN** |
| `census-hooks-complete` | every entity entry point calls its census hook | drop on_timer_create from nros_cpp_timer_create (Wall) | 1 | — | — | **CLEAN** |
| `census-no-conditional-api` | no nros-cpp public declaration is conditional on metadata/census modes | #ifdef NROS_METADATA_MODE around a new decl in node.hpp | 1 | — | — | **CLEAN** |
| `ci-cli-from-source` | no workflow acquires the nros CLI as a release asset | gate.yml step `gh release download v1 -p "nros-*"` | 1 | — | — | **CLEAN** |
| `ci-doc-workflow-refs` | a CI doc cites only workflows that exist | ci-conventions.md intro cites `zz-audit-gone.yml` (outside the Historical section) | 1 | — | — | **CLEAN** |
| `ci-no-fixture-tolerance` | CI never sets NROS_FIXTURES_OPTIONAL | gate.yml step env NROS_FIXTURES_OPTIONAL: "1" | 1 | — | — | **CLEAN** |
| `ci-no-verb-fallback` | a CI step never falls back from one just verb to another | gate.yml step `just zephyr build-all \|\| just zephyr build-examples` | 1 | — | — | **CLEAN** |
| `cli-fresh` | the in-tree nros binary matches its sources | edit a CLI source (code line) after the build | 1 | — | — | **CLEAN** |
| `cli-language-literals` | no CLI code decides on a language by comparing a string | fn comparing lang == "cpp" in nros-cli-core lib.rs | 1 | — | — | **CLEAN** |
| `cmake-find-program-shadowed` | find_program(VAR) never follows a set(VAR ...) | set(_NROS_AUDIT_TOOL "") + find_program in NanoRosCodegenCore.cmake | 1 | — | — | **CLEAN** |
| `cmake-generated-source-owners` | a custom-command OUTPUT is a source of only one target | one generated .c listed by two add_library in nros-rmw-cyclonedds CMakeLists | 1 | — | — | **CLEAN** |
| `cmake-image-policy` | a cmake file linking NanoRos into an executable applies the panic policy | delete nros_apply_panic_policy from rclcpp-compat-smoke (umbrella reached via <msg>__nano_ros_cpp) | 0 | same deletion + literal NanoRos::NanoRosCpp in its target_link_libraries | 1 | **HOLE** |
| `cmake-verb-reachable` | a cmake module defining a public nano_ros_* verb is include()-reachable | new cmake/NanoRosAuditVerb.cmake defines nano_ros_audit_verb, included nowhere | 1 | — | — | **CLEAN** |
| `codegen-tool-reconfigure` | a configure-time emitter registers nros_codegen_tool_reconfigure | delete the call in NanoRosNodeRegister.cmake | 1 | — | — | **CLEAN** |
| `codegen-version-surface` | NROS_CODEGEN_VERSION moves when the version surface does | CdrWriter::write_u32 signature u32 -> u64 (no version bump) | 1 | — | — | **CLEAN** |
| `component-entity-bounds` | every nros::node! class declares ENTITY_BOUNDS | remove ENTITY_BOUNDS from esp32-c3 talker | 1 | — | — | **CLEAN** |
| `component-lang-vocabulary` | NROS_COMPONENT_LANG is written only via the shared lowercase write | raw set_property NROS_COMPONENT_LANG "CPP" in NanoRosNodeRegister.cmake | 1 | — | — | **CLEAN** |
| `config-fallback-macros` | a committed fallback config header carries every macro generated artifacts read | delete NROS_CODEGEN_VERSION_MIN from the C++ buildless header | 1 | — | — | **CLEAN** |
| `config-header-producers` | every producer of nros_config_generated.h defines the codegen version range | delete NROS_CODEGEN_VERSION_MIN from the _exact template | 1 | — | — | **CLEAN** |
| `config-header-single-writer` | *config_generated.h has exactly one writer (the mirror script) | a SHELL writer: fixtures-build.sh cp onto nros_cpp_config_generated.h | 0 | a CMake writer: file(COPY_FILE … nros_cpp_config_generated.h) in NanoRosCodegenCore.cmake | 1 | **HOLE** |
| `core-crates-are-no-std` | every core crate declares #![no_std] unconditionally | nros-diagnostics: #![no_std] -> cfg_attr(not(feature=std), no_std) | 1 | — | — | **CLEAN** |
| `cpp-capability-layout` | a capability probe gates a method, never a member (sizeof) | double member inside #ifdef NROS_CPP_HAS_STD_STRING in FixedString | 1 | — | — | **CLEAN** |
| `cpp-destroy-shape` | every nros_cpp_*destroy* FFI has a destroy_shape row | new nros_cpp_audit_destroy FFI in guard_condition.rs, no row | 1 | — | — | **CLEAN** |
| `cpp-ffi-error-mapping` | the C++ FFI never maps Err(_) to TRANSPORT_ERROR | action.rs Err(_) => NROS_CPP_RET_ERROR -> TRANSPORT_ERROR | 1 | — | — | **CLEAN** |
| `cpp-freestanding-mechanisms` | handle/owned/inplace_fn compile freestanding on every shipped toolchain | unguarded #include <functional> in inplace_fn.hpp | 1 | — | — | **CLEAN** |
| `cpp-hosted-family` | every capability conditional carries a hosted-family tag of a member | drop the tag on fixed_string.hpp's #ifdef NROS_CPP_HAS_STD_STRING | 1 | — | — | **CLEAN** |
| `cpp-hosted-minimal-libcpp` | every public nros-cpp header compiles hosted + minimal libcpp | unguarded #include <map> in node.hpp | 1 | — | — | **CLEAN** |
| `cpp-no-std-stdio` | no std::-qualified stdio in a cross-compiled library TU | graph.cpp fprintf( -> std::fprintf( | 1 | — | — | **CLEAN** |
| `cpp-subscription-bound-supplied` | every C++ subscription registration states its rx bound; typed sites never use unknown | subscription.hpp typed site rx_buffer_capacity<M> -> rx_bound_unknown | 1 | — | — | **CLEAN** |
| `cross-toolchain-provenance` | toolchain files set the compiler from a resolved variable, report provenance | arm-freertos-armcm3.cmake CMAKE_C_COMPILER literal arm-none-eabi-gcc | 1 | — | — | **CLEAN** |
| `cxx-compat-shim-coverage` | every std:: C-library name a shim-served source uses is exported by the shim | std::strcoll in examples/workspaces/cpp FibClient.cpp | 1 | — | — | **CLEAN** |
| `cxx-compat-shim-facilities` | each cxx-compat shim supplies the freestanding facilities (correct decay etc.) | threadx shim decay approximated as remove_cv<remove_reference> | 1 | — | — | **CLEAN** |
| `cxx-standard-floor` | no build file declares a C++ standard below nros-cpp's floor | set(CMAKE_CXX_STANDARD 14) in an example CMakeLists | 1 | — | — | **CLEAN** |
| `cyclone-backend-sources` | cmake and build.rs Cyclone source lists agree | drop session.cpp from nros-rmw-cyclonedds-sys build.rs | 1 | — | — | **CLEAN** |
| `dds-isolation-symmetry` | a test pinning its DDS ros2 peer pins our own Command too | drop the pin in spawn_bridge (the Command helper the ros2-peer test fn calls) | 0 | drop the inline pin in advertised_state_interop.rs (Command::new in the peer fn) | 1 | **HOLE** |
| `ddsrt-funnel-producers` | every road compiling Cyclone defines NROS_DDSRT_PLATFORM_FUNNEL | zephyr road's COMPILE_DEFINITIONS renamed | 1 | — | — | **CLEAN** |
| `declared-fact-carriers` | every NROS_DECLARED_* fact is produced, consumed and watched | drop rerun-if-env-changed=NROS_DECLARED_MAX_ARRAY_LEN in nros-params/build.rs | 1 | — | — | **CLEAN** |
| `declared-subscription-shape` | each DeclaredSubscriptionShape variant documents its entry point + lowering | strip the first variant's doc lines naming its lowering/entry point | 1 | — | — | **CLEAN** |
| `decoupling` | nros/nros-node reach no concrete platform crate | nros umbrella gains an optional nros-platform-stm32f4 dependency | 1 | — | — | **CLEAN** |
| `default-gates-run-somewhere` | every ci-gate step / default gate is run by some workflow event | gate.yml test-lane-contracts step runs `echo skipped` (name kept) | 1 | — | — | **CLEAN** |
| `deferred-call-args` | cmake_language(DEFER CALL) never passes an unexpanded ${local} | NanoRosBoardFacts.cmake DEFER CALL passes "${_scheduled}" | 1 | — | — | **CLEAN** |
| `deploy-board-resolves` | every [deploy.*]/[image.*].board in a system.toml resolves to one descriptor | esp32-c3 talker system.toml image board -> esp32-c3-baremetalz | 1 | — | — | **CLEAN** |
| `dist-floors` | every dist row declares a measured floor | drop the qemu linux-x86_64 floor | 1 | — | — | **CLEAN** |
| `dist-runtime-deps` | a dist's system=[..] covers every library its programs need | qemu system = [libselinux1, libpcre2] -> [libpcre2] | 1 | — | — | **CLEAN** |
| `doc-recipe-refs` | every `just <recipe>` in a doc names a recipe that exists | docs/development/ci-conventions.md names a recipe that does not exist (zz-audit-nonexistent) | 1 | — | — | **CLEAN** |
| `emitter-just-spelling` | user-reachable tool messages never prescribe a bare just recipe | message(FATAL_ERROR "run: just setup-cli") in cmake/NanoRosCodegenCore.cmake | 1 | — | — | **CLEAN** |
| `entity-census` | the census runs and refuses by content (stale / missing-in-contract) | CLI freshness_verdict never refuses (`if false && policy.refuses()`), CLI rebuilt | 1 | — | — | **CLEAN** |
| `entity-slot-costs` | the CLI's per-kind callback slot cost matches nros-node's registration | EntityKind::Publisher => 0 -> 1 in entity_inventory.rs | 1 | — | — | **CLEAN** |
| `entry-locator-ssot` | NROS_ENTRY_LOCATOR has one producer | second producer set(NROS_ENTRY_LOCATOR ...) in NanoRosNodeRegister.cmake | 1 | — | — | **CLEAN** |
| `entry-pack-conformance` | a codegen pack's manifest and templates agree | delete packs/c/service.c.jinja | 1 | — | — | **CLEAN** |
| `entry-rmw-vocabulary` | NROS_ENTRY_RMW's bakeable names equal the registry's names | uorb package.xml provides rmw name uorb -> uorbx | 1 | — | — | **CLEAN** |
| `entry-rung-consumers` | a baked NROS_ENTRY_* rung is consumed by every language surface | C surface baked_rmw.h: every NROS_ENTRY_RMW -> NROS_ENTRY_RMWX | 1 | — | — | **CLEAN** |
| `entry-session-name` | every emitted run_components call names a session | C++ boot_wrapper run_components(...) -> run_components(&__nros_entry_setup) (run_tiers keeps the marker) | 1 | the same, plus the marker blanked on run_tiers too | — | **CLEAN** |
| `example-matrix` | no per-RMW example roots (examples/<plat>/<lang>/<rmw>/) | new examples/native/rust/zenoh/talker/Cargo.toml | 1 | — | — | **CLEAN** |
| `executor-backing-arena-pairing` | a stated backing_u64s must reach a port that subtracts it (and arithmetic pairs) | freertos board descriptor states backing_u64s (port does not subtract) | 1 | — | — | **CLEAN** |
| `executor-stack-floor` | every producer emits and guards NROS_EXECUTOR_MAIN_STACK_MIN | first cpp.rs emitter's guard -> #if 0 | 1 | — | — | **CLEAN** |
| `export-f-closure` | every export -f list closes over its call graph | drop nros_cmake_dir_cc from fixtures-build.sh export -f list | 1 | — | — | **CLEAN** |
| `eyre-context-alias` | use WrapErr, never the version-conditional eyre::Context | ament_installer.rs imports eyre::Context | 1 | — | — | **CLEAN** |
| `feature-contract` | clause a: heap gates are cfg(feature=alloc), never any(alloc, std) | nros-core lib.rs fn under cfg(any(feature=alloc, feature=std)) | 1 | — | — | **CLEAN** |
| `feature-gated-modules` | nros-node compiles with each module-gating feature alone | lifecycle_services.rs unconditionally uses parameter_services | 1 | — | — | **CLEAN** |
| `feature-set-ssot` | no ROS edition spelled outside the SSoT (integrations: only the Kconfig map) | integrations/nuttx/CMakeLists.txt hardcodes jazzy | 1 | — | — | **CLEAN** |
| `ffi-struct-mirrors` | hand-mirrored FFI structs match the canonical header | component.h mirror of nros_cpp_integrity_status_t gains a tail field | 1 | — | — | **CLEAN** |
| `fixture-artifact-dir-inputs` | a packer of a lane-built artifact derives the row's args/env | esp32 packer back to nros_fixture_row_artifact_dir <leaf> esp32 "" "" (the issue-1025 regression) | 1 | — | — | **CLEAN** |
| `fixture-binary-names` | a test's fixture binary name is a real CMake target | threadx_riscv64_qemu.rs talker name riscv64_threadx_rust_talker -> _talkr | 1 | — | — | **CLEAN** |
| `fixture-groups` | rows sharing a cargo group never collide on artifact name | group slug ignores the variant (coarse key) | 1 | — | — | **CLEAN** |
| `fixture-id-guard` | a builder narrowed by --id to a non-matching id fails | case 3 (right table, wrong coords, --id) returns 0 | 1 | — | — | **CLEAN** |
| `fixture-require` | a resolver Err is converted only in RequireFixture::require | entry_e2e.rs: match build_*(..) { Ok(b) => b, Err(_) => skip!(..) } | 1 | — | — | **CLEAN** |
| `fixture-row-keep-going` | every builder row goes through nros_fixture_row (E: reach) | workspace-fixtures-build.sh calls build_workspace directly | 1 | — | — | **CLEAN** |
| `fixture-stamp-honesty` | the stamp require refuses a stamp that recorded skipped modules | fixture-lane.sh `if [ -n "$stamp_skipped" ]` -> `if false` | 1 | — | — | **CLEAN** |
| `fixture-variant-features` | a feature-bearing selector names a crate that has [features] | qos-event-probe resolver selects FixtureVariant::rmw (crate has no [features]) | 1 | — | — | **CLEAN** |
| `fixtures-stale` | a stale fixture blocks its lane | N/A: needs built fixtures to stale; with none built it FAILS (rc=1, 3 families stale) — fails closed | — | — | — | **N/A** |
| `foreign-env-path-inputs` | a build script reading a foreign env var classifies it | nros-board-threadx/build.rs reads AUDIT_FOREIGN_SDK_DIR | 1 | — | — | **CLEAN** |
| `freertos-config-single-carrier` | FreeRTOS config* macros reach the compiler only via FreeRTOSConfig.h | target_compile_definitions(freertos_kernel PUBLIC configUSE_TRACE_FACILITY=1) in mps2 board cmake | 1 | — | — | **CLEAN** |
| `gate-cache-keys-agree` | warm-cache writes the same key/path the check job reads | first sccache key in gate.yml gets an extra token | 1 | — | — | **CLEAN** |
| `gate-selftests` | a gate runs its selftest on the normal path | board-build-wiring's normal-path selftest put behind the flag | 1 | — | — | **CLEAN** |
| `generated-cmake-keywords` | every keyword the CLI writes into a generated CMakeLists is parsed | cmake_root.rs emits PANICX instead of PANIC | 1 | — | — | **CLEAN** |
| `generated-leaf-regenerable` | a leaf depending on its own generated/ has a package.xml | new leaf examples/native/rust/zz-audit/Cargo.toml path=generated/std_msgs, no package.xml | 1 | — | — | **CLEAN** |
| `generated-output-collisions` | no two images / west leaves build into one output | realtime-rust derived_bringup [image.zephyr_derived] -> [image.zephyr] (demo_bringup has it) | 1 | — | — | **CLEAN** |
| `generated-schema-coverage` | every committed generated message exposes FIELDS and a DHEADER per serialize | builtin_interfaces Duration: const FIELDS renamed | 1 | — | — | **CLEAN** |
| `generated-schema-coverage-dheader` | a DHEADER per fn serialize | builtin_interfaces Duration: serialize's begin_dheader dropped | 1 | — | — | **CLEAN** |
| `git-dir-layout-assumptions` | nothing models git's layout (.git/index etc.) | build.rs watches root.join(".git/index") | 1 | — | — | **CLEAN** |
| `goal-cdr-stripped` | an FFI taking goal_cdr strips the CDR header | nros-c client.rs send_goal_raw(strip_cdr_header(slice)) -> send_goal_raw(slice) | 1 | — | — | **CLEAN** |
| `grep-q-error-conflation` | no new grep -q conditional conflating error and non-match | `if grep -q foo` in scripts/build/fixtures-build.sh | 1 | — | — | **CLEAN** |
| `hook-repo-side-effects` | a hook-reachable git-init script leaves the invoking repo byte-identical | check-git-dir-layout-assumptions.py: drop the inherited-env clear on its git init runner | 1 | — | — | **CLEAN** |
| `host-platform-vocabulary` | a board never claims both posix and linux | linux board names += posix | 1 | — | — | **CLEAN** |
| `host-triple-literals` | M2: no NO_DEFAULT_PATH find_program with a literal triple in PATHS | find_program(... PATHS .../x86_64-unknown-linux-gnu/bin NO_DEFAULT_PATH) in NanoRosCodegenCore.cmake | 1 | — | — | **CLEAN** |
| `image-locator-bake` | every embedded cargo image row bakes a locator its board reads | rust demo_bringup [image.freertos] loses its locator | 1 | — | — | **CLEAN** |
| `image-paths-apply-policy` | every image-building path (incl. board seams) applies the panic policy | rv-virt-threadx board seam drops nros_apply_panic_policy (it carries the runtime via nros_declare_rust_runtime_carrier, no literal umbrella) | 0 | custom-platform example: nano_ros_add_executable -> raw add_executable (literal umbrella present) | 1 | **HOLE** |
| `infra-queryable-counts` | infra queryable counts match the servers created | LIFECYCLE_SERVICE_QUERYABLES 5 -> 6 | 1 | — | — | **CLEAN** |
| `inherited-checkout-paths` | every path-valued sdk-env export goes through the re-root wrapper | FREERTOS_DIR export bypasses _NROS_REROOT | 1 | — | — | **CLEAN** |
| `interface-glob-configure-depends` | an interface msg/srv/action glob carries CONFIGURE_DEPENDS | drop CONFIGURE_DEPENDS from the local msg glob | 1 | — | — | **CLEAN** |
| `interlock-visibility` | an interlocked CI job has a job reporting that it did not run | nightly coverage-matrix-nightly report step -> echo ok | 1 | — | — | **CLEAN** |
| `interop-cell-runners` | every Runtime interop cell's test binary is invoked by a recipe/workflow | just/xrce.just binary(xrce_ros2_interop) -> binary(xrce_ros2_interopz) (only exclusions remain) | 1 | — | — | **CLEAN** |
| `interop-verdict-ledger` | every live-peer cell has a recorded verdict | rename the bridge-zenoh-to-cyclone verdict's cell | 1 | — | — | **CLEAN** |
| `issue-index` | README 'Recently resolved' digests are frozen (ratchet 327) | append one digest | 1 | — | — | **CLEAN** |
| `issue-index-ratchet-down` | the frozen digest ratchet is forced down (W9) | delete one digest (count 326 < 327) | 0 | the same deletion plus one appended digest (net 327, a regrowth into the freed slot) | 1 | **HOLE** → the COUNT fixed here; the swap itself (net 327) still passed rc 0 until the block CONTENT was frozen by sha256 (issue 1739 follow-up) |
| `just-recipe-paths` | a recipe's literal in-repo paths exist | bash scripts/no-such-audit.sh in just/check/docs.just | 1 | — | — | **CLEAN** |
| `just-recipe-refs` | every just <recipe> in a recipe body exists | just zz-audit-missing in just/check/docs.just | 1 | — | — | **CLEAN** |
| `kconfig-knob-forwarding` | each forwarded knob is read from the RIGHT Kconfig symbol | ZPICO_GET_REPLY_BUF_SIZE resolved from CONFIG_NROS_GET_POLL_INTERVAL_MS | 1 | — | — | **CLEAN** |
| `kconfig-overridden-values` | a leaf Kconfig value a later fragment overrides is refused (ratchet) | zephyr c/talker prj.conf sets CONFIG_HEAP_MEM_POOL_SIZE (mps2-an385.conf overrides) | 1 | — | — | **CLEAN** |
| `knob-delivery` | (lane half) DERIVED_PAIRS names every resolver call site | a new _nros_resolve_derivable_knob(NROS_AUDIT_KNOB …) call site in nros_cargo_build.cmake | 1 | — | — | **CLEAN** |
| `knob-ends` | rule 1: a claimed knob is read somewhere | fixtures.toml row env claims NROS_AUDIT_DEAD_KNOB nothing reads | 1 | — | — | **CLEAN** |
| `knob-resolved-once` | a knob is resolved exactly once | plain _nros_resolve_knob(NROS_RMW_SUBSCRIBER_SLOTS) after its derivable resolution | 1 | — | — | **CLEAN** |
| `knob-single-reader` | a migrated knob has exactly one reader | nros-board-threadx build.rs reads NROS_EXECUTOR_MAX_CBS raw | 1 | — | — | **CLEAN** |
| `lane-contracts` | a merge-gating lane resolves only artifacts it builds | admitted gate-lane test example_shape.rs calls a fixture builder | 1 | — | — | **CLEAN** |
| `lane-coverage-labels` | a job label never claims an event its step does not run on | gate.yml check job renamed `check (fast on push; full on PR/nightly)` | 1 | — | — | **CLEAN** |
| `lane-scope-consumers` | a CELLS-by-platform consumer the lane filter cannot reach narrows itself | entry_e2e.rs drops the narrowing filter (keeps the out-of-lane REPORT call) | 0 | drop both admits calls | 1 | **HOLE** |
| `lane-skip-protocol` | a fixture lane never skips with echo skip; exit 0 | just/freertos.just one-liner `if …; then echo "…skip…"; exit 0; fi` | 0 | the same skip+exit 0 written as a multi-line if block | 1 | **HOLE** |
| `lane-step-duplicates` | a lane never names a gate one of its own steps already runs | ci gate steps gain check::book-links (a fast-lane gate) | 1 | — | — | **CLEAN** |
| `launch-resolve-fresh` | nros-launch-resolve matches the sources it compiled | append a line to play_launch src/ros-launch-resolve/cli/src/check.rs (submodule; restored with git -C) | 1 | — | — | **CLEAN** |
| `ledger-orphan-refs` | a ledger row never cites a file that does not exist | action.json file: action_server.hpp -> action_serverx.hpp | 1 | — | — | **CLEAN** |
| `literal-domain-id` | no entity declared with a literal domain | executor/action.rs .with_domain(self.domain_id) -> .with_domain(0) | 1 | — | — | **CLEAN** |
| `make-stall-watchdog` | the jobserver stall watchdog fires on a stall, and only on a stall | watchdog never reaches the stall branch (`if now - idle_since < stall:` -> `if True:`) | 1 | — | — | **CLEAN** |
| `markdown-links` | a markdown link resolves exactly | docs/development/ci-conventions.md links ../design/9999-audit-gone.md | 1 | — | — | **CLEAN** |
| `message-crate-identity` | a generated message crate's version is the constant 0.0.0 | nros-builtin-interfaces version 0.0.0 -> 0.1.0 | 1 | — | — | **CLEAN** |
| `msg-dep-is-path` | a message crate is a path dep, never registry-named | examples/native/rust/talker gains std_msgs = "*" | 1 | — | — | **CLEAN** |
| `named-lane-fails` | a named platform lane may not skip its way to green (sourced protocol) | just/esp32.just recipe calls nros_lane_skip without sourcing lane-skip.sh | 1 | — | — | **CLEAN** |
| `nested-cargo-lock-discipline` | a nested cargo bypassing the --locked shim states its lock discipline | build.rs: Command::new(std::env::var_os("CARGO").unwrap()).arg("build") (the docstring's own spelling, inline) | 0 | the same with `let cargo = env::var_os("CARGO")…; Command::new(cargo)` | 1 | **HOLE** |
| `nested-workspace-excludes` | every package under a root-less workspace is excluded by the repo root | drop the root exclude for examples/templates | 1 | — | — | **CLEAN** |
| `nextest-binary-filters` | every binary() in nextest.toml names a real test target | override filter binary(zz_audit_gone) | 1 | — | — | **CLEAN** |
| `nextest-test-filters` | every test()/binary() predicate in nextest.toml matches a real test | override filter test(zz_audit_no_such_case) | 1 | — | — | **CLEAN** |
| `no-alloc-image` | every book no-alloc claim is rostered/backed | book introduction claims heap-free on every board | 1 | — | — | **CLEAN** |
| `no-allow-multiple-def` | no --allow-multiple-definition / -z muldefs anywhere in the build | target_link_options -Wl,--allow-multiple-definition in NanoRosCodegenCore.cmake | 1 | — | — | **CLEAN** |
| `no-board-init` | the retired nros_board_common::board_init API is not used | nros-board-threadx build.rs references nros_board_common::board_init | 1 | — | — | **CLEAN** |
| `no-direct-kernel-alloc` | only a platform port calls the kernel allocator directly | zpico.c (RMW shim) calls pvPortMalloc | 1 | — | — | **CLEAN** |
| `no-silent-sample-drop` | an example callback never drops a sample silently | pure-c-workspace Listener.c: drop the DROPPED fprintf before return | 1 | — | — | **CLEAN** |
| `no-std-entry-emission` | no std:: path in Rust entry code a producer emits | rust entry.rs.jinja ::core::result::Result::Ok(()) -> ::std::result::Result::Ok(()) | 1 | — | — | **CLEAN** |
| `no-std-stdio` | no std::-qualified stdio in a no_std crate's src/ | nros-core lib.rs fn calling std::println! | 1 | — | — | **CLEAN** |
| `no-tracked-file-find` | never find-scan for files git tracks | scripts/build/fixtures-build.sh: find examples -name package.xml | 1 | — | — | **CLEAN** |
| `no-tracked-workspace-roots` | an example workspace root Cargo.toml/CMakeLists.txt is never tracked | track examples/workspaces/rust/Cargo.toml (force-add) | 1 | — | — | **CLEAN** |
| `no-unbounded-condvar-wait` | the unbounded condvar wait stays confined to its shim | zpico.c calls nros_platform_condvar_wait | 1 | — | — | **CLEAN** |
| `no-vacuous-tests` | no test body that only prints | entry_e2e.rs gains #[test] fn audit_vacuous() { eprintln!(..) } | 1 | — | — | **CLEAN** |
| `node-ref-fresh-mint` | a node ref minted from the node is never asked whether it is live | node.rs: node_ref_is_live(node_ref_of(n)) | 1 | — | — | **CLEAN** |
| `nros-c-feature-agreement` | the C and C++ roads resolve nros-c to one feature set | xrce descriptor c_cffi_feature rmw-xrce -> rmw-xrce-alt | 1 | — | — | **CLEAN** |
| `nuttx-links-snapshot` | no NuttX consumer links the shared live kernel tree (staging/) | nuttx-qemu build.rs: format!("{}/staging", dir) | 0 | the same consumer via .join("staging") | 1 | **HOLE** |
| `nuttx-links-snapshot-cmake` | no NuttX consumer links the shared live kernel tree (staging/) | nano-ros-nuttx.cmake: target_link_directories(... ${NUTTX_DIR}/staging) | 0 | a Rust .join("staging") consumer | 1 | **HOLE** |
| `nuttx-shared-tree-headers` | no build input takes NuttX headers from the shared tree | nuttx-qemu build.rs include(format!("{}/include", NUTTX_DIR)) | 0 | the same as Path::new(&nuttx_dir).join("include") | 1 | **HOLE** |
| `one-producer-per-tool` | an indexed tool has one producer: nros setup --tool | install-corrosion (which forwards corrosion) also curls corrosion itself | 0 | the same recipe curls ninja instead | 1 | **HOLE** |
| `orphan-generated-stamp` | a per-build generated header never loses its .stamp twin | plant target/audit/nros-cpp-generated/nros/nros_cpp_config_generated.h.stamp (no header) | 1 | — | — | **CLEAN** |
| `path-env-fingerprints` | never rerun-if-env-changed on a PATH variable | nros-board-threadx build.rs: rerun-if-env-changed=THREADX_DIR | 1 | — | — | **CLEAN** |
| `pipefail-sigpipe-assertions` | a matcher predicate under pipefail reads a here-string, never a pipe | fixtures-build.sh one-liner `{ if ! printf … \| grep -q …; then echo n; fi; }` | 0 | the same predicate as a multi-line if | 1 | **HOLE** |
| `platform-provider-features` | every RTOS platform-* feature supplies malloc and panic | nros-c platform-freertos loses global-allocator | 1 | — | — | **CLEAN** |
| `posix-platform-purity` | nros-platform-posix holds to POSIX (Linux-only calls guarded) | net.c: ungated eventfd() | 1 | — | — | **CLEAN** |
| `preconditions-provisioned` | every tier precondition probe is classified and provisioned | new unclassified probe in check-tier-preconditions.sh | 1 | — | — | **CLEAN** |
| `prelude-tiers` | nros::prelude holds no extension-classified (RTOS-only) names | prelude re-exports CdrReader/CdrWriter | 1 | — | — | **CLEAN** |
| `prose-issue-refs` | a prose issue id resolves to a file | ci-conventions.md mentions an issue id that has no file (nine-nine-nine-eight) | 1 | — | — | **CLEAN** |
| `provisioned-root-guard-reach` | every fixture-build front door reaches the ownership guard (R2) | build-test-fixtures-leaves loses its _require-owned-provisioned-roots dependency | 1 | — | — | **CLEAN** |
| `ps-zombie-blind` | a ps process-group scan excludes zombies | subtree-guard.sh keeps `stat=` but drops the `$3 !~ /^Z/` filter | 0 | subtree-guard.sh drops `stat=` from the ps columns | 1 | **HOLE** |
| `px4-archive-header-pairing` | the PX4 module asserts BOTH generated headers pair with the archive | C-header pairing call renamed to message(STATUS …) (its header path string kept) | 0 | the C-header pairing call deleted outright | 1 | **HOLE** |
| `px4-archive-header-pairing-predicate` | the pairing predicate actually rejects a mismatched pair | NanoRosArchivePairing.cmake: `if(NOT _nap_hit)` -> `if(FALSE)` | 1 | — | — | **CLEAN** |
| `qos-profile-ssot` | named QoS presets match upstream field by field | nros-rmw QOS_PROFILE_SENSOR_DATA depth 5 -> 6 | 1 | — | — | **CLEAN** |
| `release-manifest` | R2: the release manifest is stamped by the binary, never composed | release-nros.yml composes manifest.toml with printf | 1 | — | — | **CLEAN** |
| `repo-dir-readers` | every Rust read of $NROS_REPO_DIR says where the 1280 rule applies | nros-cli-core lib.rs raw std::env::var("NROS_REPO_DIR") | 1 | — | — | **CLEAN** |
| `repo-root-walk-scope` | a walk rooted at the checkout root never counts other repositories | scripts/check-ci-doc-workflow-refs.py walks ROOT with os.walk | 1 | — | — | **CLEAN** |
| `repr-memory-agreement` | the C and C++ packs describe one memory layout (incl. container sub-field offsets) | cpp pack nros_cpp_heap_str_t: size/capacity swapped (CLI rebuilt with the mutation) | 0 | same swap on the heap SEQUENCE helper struct (CLI rebuilt) | 1 | **HOLE** |
| `required-contexts-reportable` | a required status check can report on a pull request | HOSTED_CHECKS gains the merge_group-only L3 context | 1 | — | — | **CLEAN** |
| `required-features-reachable` | a required-features target is enabled by some recipe | nros-node gains a [[test]] behind feature zz-audit-unreached | 1 | — | — | **CLEAN** |
| `ret-code-citations` | a doc never names an NROS_RET_* code no header defines | book page names a return code no header defines (AUDIT_BOGUS) | 1 | — | — | **CLEAN** |
| `retired-cmake-keywords` | a retired cmake keyword survives only in its tombstone | mixed rust_heartbeat_pkg nano_ros_node_register(... ENTITIES ...) | 1 | — | — | **CLEAN** |
| `retired-platform-clock-symbols` | no port defines the retired nros_platform_clock_ms/us symbols | posix platform.c defines nros_platform_clock_ms | 1 | — | — | **CLEAN** |
| `retired-submodule-refs` | no live reference to a retired submodule path | bootstrap.sh names packages/cli/third-party/ros-launch-resolve | 1 | — | — | **CLEAN** |
| `rmw-agnostic` | core and API crates name no backend | nros-core lib.rs const naming cyclonedds | 1 | — | — | **CLEAN** |
| `rmw-doc-slot-names` | a backtick in the RMW ABI headers names an identifier that exists | rmw_vtable.h comment cites `zz_audit_missing_slot` | 1 | — | — | **CLEAN** |
| `rmw-force-link-anchor` | a pure-Rust image force-links its RMW backend | zephyr rust/action-client drops force_link_backend! | 1 | — | — | **CLEAN** |
| `rmw-required-slots` | every .expect()ed vtable slot is in the required list | cffi lib.rs .expect("rmw vtable: count_publishers") on an optional slot | 1 | — | — | **CLEAN** |
| `rmw-ret-sign` | nobody tests an RMW status by its sign (ratchet) | zpico.c: rc = vt->take(...); if (rc < 0) | 1 | — | — | **CLEAN** |
| `rmw-slot-producers` | --check: an inert family names no slot that gained a consumer | cffi lib.rs reads vtable.required_rx_bytes (rx-sizing family) | 1 | — | — | **CLEAN** |
| `ros-env-spelling` | a test's ROS 2 env is spelled only via ros_env / ros2 helpers | cpp_multi_node_entry.rs literal `source /opt/ros/humble/setup.bash` | 1 | — | — | **CLEAN** |
| `ros2-daemon-queries` | a graph-reading ros2 command passes --no-daemon | cpp_multi_node_entry.rs builds `ros2 topic list` without --no-daemon | 1 | — | — | **CLEAN** |
| `rtos-target-os` | the hosted question is never re-spelled as target_os != none | a build.rs decides hosted by CARGO_CFG_TARGET_OS != "none" | 0 | the same question as #[cfg(not(target_os = "none"))] | 1 | **HOLE** |
| `runtime-umbrella-link-sites` | a PROPAGATED umbrella link goes through nros_link_runtime_umbrella | NanoRosCodegenCore.cmake: target_link_libraries(x PUBLIC NanoRos::NanoRosCpp) | 1 | — | — | **CLEAN** |
| `rust-stdio-on-zephyr` | no raw Rust std stdio in crates Zephyr links | zephyr rust/action-client app_main.rs calls std::eprintln! | 1 | — | — | **CLEAN** |
| `rust-targets-covered` | every declared Rust cross target has a config/rust-targets.txt row | toolchain file sets Rust_CARGO_TARGET to unlisted powerpc-unknown-linux-gnu | 1 | — | — | **CLEAN** |
| `rust-targets-installed` | every cross Rust target the tree builds for is installed | config/rust-targets.txt gains `mips64-unknown-linux-gnuabi64 rustup` (not installed) | 1 | — | — | **CLEAN** |
| `rustc-wrapper-staticlib` | every RUSTC_WRAPPER producer naming sccache names the shim | gate.yml step env RUSTC_WRAPPER: sccache | 1 | — | — | **CLEAN** |
| `rustc-wrapper-staticlib-cargo-config` | every RUSTC_WRAPPER producer naming sccache names the shim | tracked root .cargo/config.toml [build] rustc-wrapper = "sccache" (cargo's own key) | 0 | gate.yml step env RUSTC_WRAPPER: sccache | 1 | **HOLE** |
| `schema-reader-provenance` | a schema constant module sets _SUPPORTED_FROM | NanoRosMessageBounds.cmake drops NROS_MESSAGE_BOUNDS_SCHEMA_SUPPORTED_FROM | 1 | — | — | **CLEAN** |
| `scoped-target-dirs-ignored` | every scoped cargo target dir a recipe asks for is ignored | codegen.just asks nros_scoped_target_dir zz-audit-scratch | 1 | — | — | **CLEAN** |
| `sdk-guard-can-fire` | no guard on an always-exported SDK var | freertos.just: if [ -z "${FREERTOS_DIR:-}" ] && [ ! -d … ] guard | 1 | — | — | **CLEAN** |
| `sdk-store-not-enumerated` | the SDK store is constructed from the pin, never enumerated | scripts/dev/zenohd.sh: ls $store/ninja \| sort -V \| tail -1 | 1 | — | — | **CLEAN** |
| `sdk-store-not-enumerated-literal` | the SDK store is constructed from the pin (headline) | a script hard-codes a versioned store path ~/.nros/sdk/ninja/1.11.1-nros1/bin/ninja | 0 | the same script enumerates and sorts the store | 1 | **HOLE** |
| `self-pkg-package-xml` | a self-pkg bringup declaring components has a package.xml | delete esp32-c3 listener package.xml | 1 | — | — | **CLEAN** |
| `set-e-bare-assignment` | under set -e, a status to inspect is not captured by a bare assignment | fixtures-build.sh: out="$(false)"; rc=$? | 1 | — | — | **CLEAN** |
| `sidecar-endpoint-keys` | every key the metadata emitter writes is declared by the reader | node_metadata.rs emits an extra `audit_key` | 1 | — | — | **CLEAN** |
| `single-rust-staticlib` | no single branch links more than one umbrella | one target_link_libraries naming both umbrellas in NanoRosCodegenCore.cmake | 1 | — | — | **CLEAN** |
| `skip-marker-matching` | skip markers are recognised with one spelling | entry_e2e.rs: msg.contains("[SKIPPED]") | 1 | — | — | **CLEAN** |
| `skippable-tests-tolerant` | a skip-capable test is run only through the tolerant runner | gate.yml step runs bare cargo nextest run --test xrce_ros2_interop | 0 | the same bare run in a just recipe (just/xrce.just) | 1 | **HOLE** → FIXED here |
| `stack-floor` | ESP32 image stack >= floor | N/A: the recipe runs --selftest + --claims only (prints, rc=0); the image check runs inside fixtures-build.sh per esp32 image, none built here | — | — | — | **N/A** |
| `staleness-probe-exemptions` | the staleness exemption rule is spelled once, in staleness.rs | binaries/mod.rs defines its own is_config_header_stamp_with_header (a predicate staleness.rs has) | 0 | the same copy named is_cargo_out_dir_product | 1 | **HOLE** |
| `std-census` | the std census ratchet only turns one way | nros-core gains a cfg(feature=std) std:: site | 1 | — | — | **CLEAN** |
| `sysdep-remedies` | remedy text derives from the index; no hand-written sudo apt | just/freertos.just echoes `sudo apt install gcc-arm-none-eabi` | 1 | — | — | **CLEAN** |
| `template-copy-out` | every copy-out template builds when copied out | talker_pkg gains an in-repo path dep (run with NROS_SKIP_STALE_CHECK=1: the template is a CLI source-stamp input, so without it the CLI stale guard fired first — first attempt INVALID) | 1 | — | — | **CLEAN** |
| `test-capture-bounded` | a test reader appends child output through capture::append | process.rs raw output.push_str(&String::from_utf8_lossy(&buffer[..n])) | 1 | — | — | **CLEAN** |
| `test-domain-assignment` | a test assigns its ROS domain, never names one | entry_e2e.rs: .env("ROS_DOMAIN_ID", "42") | 1 | — | — | **CLEAN** |
| `test-generated-dir-guards` | a generated directory's existence never decides whether a test runs | entry_e2e.rs test returns early if build/posix-zenoh/native_entry is absent | 1 | — | — | **CLEAN** |
| `test-precondition-guards` | a test-local precondition helper owns its verdict (no Option + return) | entry_e2e.rs: helper prints + returns None; test returns on None | 1 | — | — | **CLEAN** |
| `tests-can-fail` | reject test shapes that report PASS (Err arm only prints) | entry_e2e.rs: match with Err(e) => eprintln!, assertion only in Ok | 1 | — | — | **CLEAN** |
| `third-party-is-submodules` | third-party/ holds tracked submodules and nothing else | track a regular file third-party/zz-audit/README | 1 | — | — | **CLEAN** |
| `tier-has-ci-owner` | every declared CI tier is run by some workflow COMMAND | both `just ci tier1 …` invocations replaced by echo; disk-report.sh/reclaim-disk.sh ARGUMENT strings "before just ci tier1" remain | 0 | the same, with those argument strings reworded too | 1 | **HOLE** |
| `tier-priority-plan` | a tier priority pin respects its port's reserved bands | realtime-c [tiers.high.freertos] priority 3 -> 4 (the transport band) | 1 | — | — | **CLEAN** |
| `tier-priority-plan-image-selftest` | derived Zephyr priority plan vs a built image .config | N/A: recipe is the selftest; the image half needs a built Zephyr .config | — | — | — | **N/A** |
| `tier-spin-gap` | every tier spin loop reaches a scheduling point | FreeRTOS C first tier loop drops nros_tier_spin_gap_step (second loop + prototype kept) | 1 | — | — | **CLEAN** |
| `unsafe-census` | unsafe counts only fall (ratchet) | nros-core gains an unsafe block | 1 | — | — | **CLEAN** |
| `vendor-fetch-pinned` | every FetchContent in a discovered package is pinned by digest | cpp action_client_pkg CMakeLists FetchContent_Declare by tag | 1 | — | — | **CLEAN** |
| `wait-evidence-discarded` | a timed-out wait's evidence is not discarded | entry_e2e.rs: wait_for_output_pattern(..).unwrap_or_default() | 1 | — | — | **CLEAN** |
| `weak-symbols` | no new unaudited weak declaration | posix platform.c gains __attribute__((weak)) | 1 | — | — | **CLEAN** |
| `weak-symbols-image` | each override-default weak symbol is strong in the linked image | N/A: no covered prebuilt image; [SKIPPED] NOT VERIFIED via ledger (designed) | — | — | — | **N/A** |
| `west-leaf-vocabulary` | every west build-dir name the run can produce is modelled | binaries/mod.rs names zephyr-workspace/build-zz-audit/zephyr/zephyr.exe | 1 | — | — | **CLEAN** |
| `workflow-doctor-after-setup` | a job runs runner-doctor after provisioning, never before | nightly matrix-nightly: doctor step inserted before `just setup tier2-nightly` | 1 | — | — | **CLEAN** |
| `workflow-indexed-apt` | a workflow never apt-installs an index-declared package | gate.yml step apt-get install libglib2.0-dev (indexed prereq) | 1 | — | — | **CLEAN** |
| `workflow-repo-env` | a CI step invoking just/nros/west sources activate.sh | gate.yml step `run: just setup tier2` without sourcing | 1 | — | — | **CLEAN** |
| `workflow-runner-isolation` | a self-hosted job is unreachable from a fork pull_request | nightly.yml (self-hosted jobs) gains a pull_request trigger | 1 | — | — | **CLEAN** |
| `workflow-setup-spelling` | provision with `just setup <scope>`, in workflows and prose | book page teaches the module spelling (the zephyr module's own setup recipe) | 1 | — | — | **CLEAN** |
| `workspace-rmw-agreement` | nano_ros_workspace(BACKEND) agrees with the bringup's [system].rmw | force-tracked examples/workspaces/c/CMakeLists.txt: BACKEND xrce vs demo_bringup rmw | 1 | — | — | **CLEAN** |
| `xrce-config-manifest` | the XRCE build.rs owns no configuration value (one manifest) | build.rs hardcodes a UCLIENT define value | 1 | — | — | **CLEAN** |
| `xrce-source-manifest` | both XRCE lanes compile exactly the manifest's sources | build.rs grows a build.file(udp_transport.c) beside the posix platform files | 1 | — | — | **CLEAN** |
| `xrce-vendored-versions` | the vendored XRCE version is derived from the tree, never a literal | build.rs defines MICROCDR_VERSION_STR literal | 1 | — | — | **CLEAN** |
| `zenoh-feature-off-compile` | zenoh-pico portable sources compile in the shipped feature-off set | zenoh-pico filtering.c: an unguarded call to the MATCHING-only _z_write_filter_ctx_remove_callbacks (submodule; restored with git -C). First attempt edited the selftest anchor itself (invalid, not counted) | 1 | — | — | **CLEAN** |
| `zenoh-lane-ownership` | every platform declares its zenoh owner (compiled_by) | zephyr platform toml drops compiled_by = "platform" | 1 | — | — | **CLEAN** |
| `zenoh-platform-macros` | a port declares only its own platform to zenoh-pico | nuttx platform defines += ZENOH_LINUX | 1 | — | — | **CLEAN** |
| `zenoh-source-manifest` | all three zenoh-pico lanes compile one manifest's source list | qemu build-zenoh-pico.sh appends a source outside the manifest | 1 | — | — | **CLEAN** |
| `zenohd-flag-invocations` | no site invokes the router with command-line flags | scripts/dev/zenohd.sh runs the ROS router with a listen flag | 1 | — | — | **CLEAN** |
| `zenohd-router-skips` | a test that cannot reach a router skips (or_skip), never fails | declarative_bridge test: or_skip(ZenohRouter::start_unique()) -> .expect | 1 | — | — | **CLEAN** |
| `zenohd-spawn-sites` | only the shared fixture spawns the router | entry_e2e.rs builds a Command on ros_zenohd_path() | 1 | — | — | **CLEAN** |
| `zephyr-module-binding` | every configuring west build names this checkout's nros module | a tracked python script runs [west, build, -b, native_sim, app] with no module flag | 1 | — | — | **CLEAN** |
| `zephyr-workspace-foreign-checkout` | no Zephyr build cache in this checkout's build root names another checkout | plant build-zz-audit/CMakeCache.txt pointing into the main checkout's absolute path | 1 | — | — | **CLEAN** |
| `zephyr-workspace-resolvers` | no fourth spelling of the Zephyr workspace resolution chain | fixtures-build.sh: ws=${NROS_ZEPHYR_WORKSPACE:-$repo_root/zephyr-workspace} (env -> checkout rung) | 0 | the same spelling with the legacy sibling rung ../nano-ros-workspace | 1 | **HOLE** |

## Scope 2 — the shared class helpers

Each helper was weakened and the whole `check fast` lane run. A helper break no member notices is a finding: the members drive the helper over their own data, which is usually already correct, so only the helper's own controls can tell a broken helper from a clean tree.

| helper mutation | rule | `check fast` | members that caught it | verdict |
| --- | --- | ---: | --- | --- |
| check_just_sources.py: just_sources drops the `check` module (the W2 hole) | just_sources: every justfile just loads, incl. mod check | 1 | `fixture-artifact-dir-inputs`, `just-recipe-paths`, `just-recipe-refs`, `lane-skip-protocol`, `named-lane-fails`, `provisioned-root-guard-reach`, `sdk-guard-can-fire`, `sysdep-remedies` | CLEAN |
| check_skip.py: unverified() returns 0 without calling the ledger | check_skip.unverified records the skip (or fails under strict) | 0 | — | **UNNOTICED** |
| check-skip.sh: the ledger write removed (skip printed, never recorded) | nros_check_skip records the skip in the ledger the lane reports | 0 | — | **UNNOTICED** |
| check-skip.sh: the strict branch removed | nros_check_unverified fails under NROS_CHECK_SKIP_STRICT=1 | 0 | — | **UNNOTICED** |
| comments.py: the `//` branch disabled (C/C++/Rust line comments kept as code) | comments.strip_comments: `//` comments are blanked | 1 | `board-build-wiring`, `capability-slot-counts`, `cli-language-literals`, `component-entity-bounds`, `capability-flavour-guards`, `core-crates-are-no-std`, `cpp-subscription-bound-supplied`, `cyclone-backend-sources` +37 more | CLEAN |
| exemptions.py: neighbour test disabled | Exemptions.check: a table covering a neighbour is refused | 0 | — | **UNNOTICED** |
| file_kinds.py: silently drops everything under zephyr/ | files_of_kind: every tracked file of the kind | 1 | `allocator-never-waits`, `build-profile-literals`, `emitter-just-spelling`, `entry-locator-ssot`, `no-unbounded-condvar-wait` | CLEAN |
| harvest.py: `if not pop:` -> `if False:` | harvest.reconcile: an empty harvest is a problem | 1 | `atomic-sync-writes`, `decoupling`, `fixture-binary-names`, `msg-dep-is-path` | CLEAN |
| harvest.py: `if name not in pop:` -> `if False:` | harvest.reconcile: a stale exemption is a problem | 1 | `atomic-sync-writes`, `decoupling`, `fixture-binary-names`, `msg-dep-is-path` | CLEAN |
| kernel_alloc.py: pvPortMalloc/pvPortCalloc dropped from the alternation | kernel_alloc: the one list of RTOS allocator primitives | 1 | `no-alloc-image`, `no-direct-kernel-alloc` | CLEAN |
| per_item.py: blocks() returns after the FIRST block | per_item.blocks: every head with a body | 1 | `atomic-sync-writes`, `config-fallback-macros`, `config-header-producers`, `dds-isolation-symmetry`, `fixture-binary-names`, `generated-schema-coverage`, `literal-domain-id`, `path-env-fingerprints` +2 more | CLEAN |
| population.py: `if count > 0:` -> `if count >= 0:` (zero accepted) | require_population: zero undeclared FAILS | 1 | `cargo-custom-command-depfile`, `host-triple-literals`, `interop-verdict-ledger`, `nested-workspace-excludes`, `no-std-entry-emission`, `required-contexts-reportable`, `workflow-runner-isolation` | CLEAN |
| population.sh: `-gt 0` -> `-ge 0` (zero accepted) | nros_require_population: zero undeclared FAILS | 1 | `issue-index`, `sysdep-remedies` | CLEAN |
| ratchet.py: `elif n < b:` -> `elif False:` | ratchet.judge: a count below its row fails (forced down) | 1 | `fixture-require`, `grep-q-error-conflation`, `kconfig-overridden-values`, `rmw-ret-sign`, `unsafe-census`, `wait-evidence-discarded` | CLEAN |
| ratchet.py: `if n > b:` -> `if n > b + 1000:` | ratchet.judge: a count above its row is a rise | 1 | `fixture-require`, `grep-q-error-conflation`, `kconfig-overridden-values`, `rmw-ret-sign`, `unsafe-census`, `wait-evidence-discarded` | CLEAN |
| check-store-corrosion.sh: on MISSING, return 0 (run the gate, which then clones) | a probe gate with an empty store SKIPS through the ledger, never fetches | 0 | — | **UNNOTICED** |
| workflow_commands.py: ci_files never includes actions | ci_files: workflows AND composite actions | 1 | `ci-cli-from-source`, `ci-no-fixture-tolerance`, `ci-no-verb-fallback`, `just-recipe-refs`, `workflow-setup-spelling` | CLEAN |

## Scope 3 — random sample (seed 20261007)

| gate | rule | mutation | rc | control | ctl rc | verdict |
| --- | --- | --- | ---: | --- | ---: | --- |
| `arch-profile-resolution` | an [arch.*] profile is reachable from its platform's arch list | freertos platform arch list drops cortex-r52 ([arch.cortex-r52] unreachable) | 1 | — | — | **CLEAN** |
| `book-no-just` | the book's user track teaches no `just` | getting-started/anatomy.md tells the user `just setup-cli` | 1 | — | — | **CLEAN** |
| `build-rs-rerun-paths` | every static rerun-if-changed path exists | threadx build.rs: rerun-if-changed=c/zz_audit_missing.c | 1 | — | — | **CLEAN** |
| `build-wiring-roads` | every Driver variant is a road the wiring map knows | Driver gains a `Make` variant | 1 | — | — | **CLEAN** |
| `cargo-config-tracked` | a tracked leaf .cargo/config.toml never patches an uncommitted generated/ tree | stm32f4-porting/polling config patches generated/std_msgs | 1 | — | — | **CLEAN** |
| `example-leaf-target-dirs` | no cargo build writes an examples/**/target/ dir | just/native.just recipe: cd examples/native/rust/talker && cargo build | 1 | — | — | **CLEAN** |
| `gate-visibility` | a gate in a non-merge-gating lane is on the shrink-only ungated list | build-serial gains book-links (a new non-merge-gating member) | 1 | — | — | **CLEAN** |
| `ivc-fsp-compile` | nvidia-ivc --features fsp compiles when the FSP tree is present | N/A: NV_SPE_FSP_DIR not provisioned; reported SKIP via ledger (designed) | — | — | — | **N/A** |
| `manifests-parse` | every tracked Cargo.toml parses | stm32f4-porting/polling Cargo.toml gets an unclosed table header | 1 | — | — | **CLEAN** |
| `param-inventory-road-parity` | EntityInventory::from_model's emptiness predicate tests node_params | drop the node_params term | 1 | — | — | **CLEAN** |
| `python-deps` | report which Python packages a lane lacks; never provision an interpreter | N/A: a lane-preflight reporter (just/px4.just), not a just-check gate; nothing to mutate in-tree that it judges | — | — | — | **N/A** |
| `rmw-descriptors` | S2: no two backends claim the same name | xrce package.xml announces rmw name rmw-zenoh (zenoh's name) — first mutation (duplicate c_cffi_feature) was outside S1/S2 (that field is derived/retired from this gate), rewritten | 1 | — | — | **CLEAN** |
| `roadmap-commit-refs` | a `PR #N (sha)` in a roadmap doc names a commit main reaches | phase-162 doc cites PR #1234 (deadbee) | 1 | — | — | **CLEAN** |
| `sched-dim-arms-compile` | the SMP accept arms compile | threadx_hooks.c tx_thread_smp_core_exclude called with one argument | 1 | — | — | **CLEAN** |
| `scope-namespace` | every justfile mod is classified (platform or not) | root justfile gains an unclassified mod zzaudit | 1 | — | — | **CLEAN** |
| `sizes-header-mirrors` | each per-build sizes header mirror equals its source | N/A for mutation (needs build trees). W4-shape OBSERVED: with 0 build trees it prints `OK — 0 mirror/source pair(s)` and exits 0 with no nros_check_skip ledger record (its text admits it proved nothing) | — | — | — | **N/A** |
| `source-manifest` | the source-signature helper has no type filter | source-manifest.sh skip globs gain *.conf | 1 | — | — | **CLEAN** |
| `workspace-root-build-files` | an example workspace has no tracked root build file | force-tracked examples/workspaces/c/CMakeLists.txt | 1 | — | — | **CLEAN** |
| `zephyr-fixture-rows` | zephyr west leaves and manifest rows agree both ways | first west row's board native_sim/native/64 -> qemu_x86 | 1 | — | — | **CLEAN** |
| `zephyr-kconfig-symbols` | every symbol zephyr/Kconfig names exists on every supported line | N/A: no Zephyr Kconfig tree here; the gate FAILS closed (rc=1, verified nothing) | — | — | — | **N/A** |

## Scope 4 — lane reach

- 234 of 250 scope-1 scripts are reached through `check fast` (gate.yml: `pull_request`, `merge_group`, `push`, `schedule`).
- 2 only through `check build` (`build-serial`; gate.yml `schedule`/`workflow_dispatch`).
- The remaining 14, one by one:

| script | invoked by | verdict |
| --- | --- | --- |
| `scripts/check-action-client-arena-budget.py` | `build-test-fixtures` (justfile:1949) | reached (fixture builds) |
| `scripts/check-archive-lang-items.sh` | `build-test-fixtures` (justfile:1938) | reached; exempt reason "no caller found" is STALE |
| `scripts/check-book-identifiers.py` | none | **NO LANE** (1739) |
| `scripts/check-cargo-dir-knob-key-sites.py` | called by `check-cargo-dir-knob-key.sh` (fast) | reached |
| `scripts/check-dist-runtime-deps.py` | `just workspace doctor` only | **NO WORKFLOW** (1739); red on this host |
| `scripts/check-executor-stack-floor.py` | none | **NO LANE** (1739) |
| `scripts/check-feature-gated-modules.sh` | `check::compile-smoke` (gate.yml, PR) | reached |
| `scripts/check-fixtures-stale.sh` | `_lane-gate` (run-matrix.yml, nightly.yml) | reached |
| `scripts/check-launch-resolve-fresh.sh` | `ci gate` step only | **NO WORKFLOW** (1739) — and the meta-gate cannot see it |
| `scripts/check-nextest-test-filters.py` | none | **NO LANE** (1739) |
| `scripts/check-rust-targets-installed.sh` | `check-tier-preconditions.sh` (`just ci tier1 run`, host-tests.yml) | reached |
| `scripts/check-stack-floor.py` | recipe: selftest+claims only; image check in `fixtures-build.sh` esp32 rows | image half reached via fixture builds |
| `scripts/check-weak-symbols-image.sh` | none | **NO LANE** (1739) |
| `scripts/check-workspace-rmw-agreement.py` | none | **NO LANE**, dead population (1739) |

### The meta-gate, verified rather than trusted

| gate | rule | mutation | rc | control | ctl rc | verdict |
| --- | --- | --- | ---: | --- | ---: | --- |
| `default-gates-run-somewhere-ci-check-step` | every step of `just ci gate` is reached by some workflow event (R1, second scope) | ci gate steps gain check::book-identifiers (lane-exempt, run by no workflow) — same shape as the LIVE check::launch-resolve-fresh | 0 | ci gate steps gain a non-check:: step no workflow runs (doctor) | 1 | **HOLE** |

Its R1 covers fast + `build-serial` + `default` and the NON-`check::` steps of `ci gate`; a lane-exempt `check::` step of `ci gate` is in neither, and `check::launch-resolve-fresh` is exactly that today (issue 1739).

## What is NOT verified

- N/A rows need artefacts this host does not have (built fixtures/images, a Zephyr Kconfig tree, the NVIDIA FSP); each states its fail-open/fail-closed behaviour without them.
- Severity is not ranked. Holes are counted only when a control proved the gate reads the site; a gate whose stated scope excludes the mutation was rewritten, not counted.
- Gates changed after `b928293215` are not in the set.
