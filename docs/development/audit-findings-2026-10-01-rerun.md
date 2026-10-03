# Audit findings — 2026-10-01 — re-run of the 2026-09-28 gate-reach audit

The phase-472 acceptance run. [phase-472](../roadmap/archived/phase-472-gate-reach-sweep.md)
landed W1–W9 and asked for this audit to be re-run over the same gates;
[audit-findings-2026-09-28](audit-findings-2026-09-28.md) is the per-gate record
it diffs against.

## Method

The 2026-09-28 standard, applied mechanically:

- Each recorded mutation was re-expressed as a concrete patch against
  `origin/main` at `d19d065ae0` (the gate fixes below were measured on
  `e3cb1e009c`). A scratch harness applied it, refused to run the gate unless
  `git status` showed the change, ran the gate the way its `just` lane does
  (`--check` for the two gates that only fail under it), recorded the exit
  status, restored the tree, and refused to continue unless the tree was clean
  again.
- A mutation counts as a HOLE only when the gate exits 0 on it. Every HOLE in
  this record has a positive CONTROL as well: the same defect placed where the
  gate DOES read, which fails. That proves the gate ran and the mutation was not
  vacuous. Six first-draft mutations were vacuous, for example a target outside
  the gate's stated rule or a site after a `#[cfg(test)]` split. Each was
  re-expressed until its control failed. None of them is counted.
- Where the code has moved since 2026-09-28, the mutation was rewritten against
  the current code: the NuttX snapshot header is now
  `nros_config_generated_buildless.h`, the knob pair lives in
  `zephyr/cmake/nros_cargo_build.cmake`, and so on. Where the gate itself is
  gone, the row is N/A.

## Totals

| | count |
| --- | ---: |
| recorded findings re-run | 156 |
| **now FAIL on their recorded hole** (fixed by W1–W9 or earlier) | **78** |
| fixed by this re-run's own PR (proved by the same mutation) | 3 |
| **still PASS: surviving holes** | **70** |
| N/A: gate deleted (`board-manifest-drift`, `nested-workspace-excludes`, `profile-board-mirror`) | 3 |
| N/A: needs artifacts this host lacks (`nextest-test-filters`: compiled nextest binaries; `tier-priority-plan-image`: built Zephyr images) | 2 |

New audits (the W5 "triaged, not audited" `cmake/` gates): 17 gates. 13 are clean,
2 have holes (`no-allow-multiple-def`, `zenoh-feature-off-compile`) and 2 are N/A
(`knob-delivery` needs a configured Zephyr build dir; `scaffold-builds` needs a
CLI rebuild plus every scaffold compiled).

Spot-checks of gates the 2026-09-28 audit recorded as clean: 15 gates, chosen
across the classes. 12 are clean and 3 have holes (`workflow-doctor-after-setup`
W1, `core-crates-are-no-std` W3, `rust-stdio-on-zephyr` W7).

**Phase-472's acceptance is NOT met.** It needs "no confirmed hole in any class
W1–W9", and there are 75: 70 re-run, 2 new and 3 spot. Most are in gates no W
item named. Of the 47 findings whose gate the phase doc never mentions, 43
still held at re-run time; 2 now fail and 2 are N/A. The class fixes did reach
their named members: of the 109 findings whose gate the doc names, 76 now fail,
30 survive, 2 are N/A and 1 gate is deleted. The 30 are mostly a second facet of
a member that was only partly widened, such as `wait-evidence-discarded`'s
`.or_else(..)` chain, `zephyr-workspace-resolvers`' `.txt` suffix skip,
`qos-profile-ssot` rule 6, and `default-gates-run-somewhere`'s `if: false`
credit. These counts are before the three fixes below.

## Fixed in this PR (each proved by its recorded mutation, which now fails)

- `check-skip-marker-matching` (W5): the population is now `file_kinds`
  `rust` + `python`, and the matcher has the Python spellings. Turning it on found
  **two live defects**, both fixed here. `scripts/test/failed-filterset.py` and
  `scripts/test/nextest-slow-tests.py` tested `"[SKIPPED]" in …`, so every
  CLASSED skip (`[SKIPPED:lane]`) was filed as a real failure and re-run. That is
  the issue-0658 shape. Measured on a probe junit: before the fix the classed skip
  was listed as a failed test; after it, only the real failure is.
  Both now use `scripts/test/skip_marker.py`.
- `check-ret-code-citations` (W5): moved onto `file_kinds`, plus a new `jinja`
  kind in that helper. Codegen templates ship into user C, so they are in the
  rule. It also has a normal-path reach control.
- `check-zenohd-flag-invocations` (W6): `\bzenohd` never matched `rmw_zenohd`,
  because `_` is a word character. The fix adds a normal-path self-test (it had
  none) and shrinks the gate-selftest baseline.

## Live defect found outside any gate's reach

- **Issue 1618.** `--allow-multiple-definition` is live in `zephyr/CMakeLists.txt`
  (two uses) and `integrations/nuttx/Make.defs`. `check-no-allow-multiple-def`'s
  allowlist says "ANY `--allow-multiple-definition` fails the gate", but its
  population is `cmake/**` + `scripts/**` + `just/**` + `examples|packages`
  CMake. Widening it to `file_kinds` turns the gate red on `main`, so it is filed
  rather than fixed: each use needs a ruling.

## Surviving holes, by class and issue

| class | issue | gates |
| --- | --- | ---: |
| W5 scan roots / kinds narrower than the rule | 1614 | 18 |
| W6 per-item / spelling-level matching | 1615 | 34 |
| W7 authored lists where the population should be harvested | 1616 | 13 |
| W1 / W3 / W4 / W8 remainder | 1617 | 10 |

## Re-run of every recorded finding

"rc then" is the 2026-09-28 result on the hole (0 = passed). "rc now" is this
run's.

| sev | gate · facet | mutation (as re-expressed) | rc then | rc now | verdict |
| --- | --- | --- | ---: | ---: | --- |
| P1 | `check-cargo-custom-command-depfile.py` | `cmake/NanoRosGenerateInterfaces.cmake`: (delete) DEPFILE "${_ffi_dep_file}"; `zephyr/cmake/nros_generate_interfaces.cmake`: (delete) DEPFILE "${_ffi_dep_file}"; `packages/api/nros-c/cmake/nros-nuttx.cmake`: (delete) DEPFILE "${_output_binary}.d" | 0 | 1 | fails now |
| P2 | `check-cmake-image-policy.py` | `cmake/platform/nano-ros-nuttx.cmake`: (delete) nros_apply_panic_policy(platform "nros_platform_link_app(${ta; `cmake/board/nano-ros-board-rv-virt-threadx.cmake`: (delete) nros_apply_panic_policy(platform | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P2 | `check-atomic-sync-writes.sh` | `packages/cli/cargo-nano-ros/src/provider_scan.rs`: std::fs::write(path, &body)?; | 0 | 1 | fails now |
| P2 | `check-board-facts-delivery.py` | `packages/api/nros-c/cmake/nros-nuttx.cmake`: message(STATUS "x") | 0 | 1 | fails now |
| P2 | `check-build-profile-literals.sh` · flag-in-pkg-cmake | `packages/api/nros-c/cmake/nros-nuttx-depfile.cmake` += execute_process(COMMAND cargo build --release) | 0 | 1 | fails now |
|  | `check-build-profile-literals.sh` · path-in-pkg-cmake | `packages/api/nros-c/cmake/nros-nuttx-depfile.cmake` += set(_rerun_p "${CMAKE_BINARY_DIR}/target/thumbv7m-none-eabi/release/li | 0 | 1 | fails now |
| P2 | `check-cargo-dir-knob-key.sh` | `packages/api/nros-c/cmake/nros-nuttx.cmake`: set(_nnbe_knob_fields "") | 0 | 0 | **HOLE** (W7, #1616); control rc 1 |
| P2 | `check-cmake-verb-reachable.py` | new `cmake/NanoRosRerunDead.cmake`; new `docs/development/rerun-mention.md` | 0 | 0 | **HOLE** (W3, #1617); control rc 1 |
| P2 | `check-book-identifiers.py` | `book/src/introduction.md` += Call `nros_rerun_fiction_ident` to start.; `packages/core/nros-core/src/lib.rs` += // nros_rerun_fiction_ident is mentioned in a comment only | 0 | 1 | fails now |
| P2 | `check-board-cargo-config-shape.py` | `packages/boards/nros-board-nuttx-qemu/nros-board.toml`: runnner = "q" | 0 | 1 | fails now |
| P2 | `check-board-name-reach.py` | `packages/boards/nros-board-nuttx-qemu/nros-board.toml`: names = ["riscv-qemu", "nuttx-riscv", | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P2 | `check-cc-build-policy.sh` | `examples/zephyr/rust/action-client/build.rs` += fn _rerun() { let _ = cc::Build::new(); } | 0 | 1 | fails now |
| P2 | `check-ci-cli-from-source.py` | `.github/actions/setup-nros-cli/action.yml` += gh release download v1 -p "nros-*" | 0 | 1 | fails now |
| P3 | `check-ci-no-fixture-tolerance.py` | `.github/actions/setup-nros-cli/action.yml` += NROS_FIXTURES_OPTIONAL=1 just test-all | 0 | 1 | fails now |
| P3 | `check-ci-no-verb-fallback.py` | `.github/actions/setup-nros-cli/action.yml` += just zephyr build-all \ | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-ci-doc-workflow-refs.py` | `docs/development/ci-conventions.md`: See `.github/workflows/rerun-nonexistent.yml` for the pattern. | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P3 | `check-c-knob-guard-order.py` | `packages/api/nros-cpp/include/nros/node.hpp`: #if NROS_COMPONENT_MAX_TIMERS < 1 | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P3 | `check-capability-slot-counts.sh` | `packages/core/nros-node/src/lifecycle_services.rs`: pub(crate) rerun_extra: LcSrv<GetState>, | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-cbindgen-pin.sh` | `packages/boards/nros-board-freertos/Cargo.toml` += [target.'cfg(any())'.build-dependencies] | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-cmake-find-program-shadowed.py` | `cmake/NanoRosSdkPin.cmake` += set(_NROS_RERUN_PROG "no CACHE here") | 0 | 0 | **HOLE** (W8, #1617); control rc 1 |
| P3 | `check-book-links.py` | `book/src/introduction.md` += See [the guide][rerunref]. | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-build-tool-verbs-exempt.py` · top-verb | `cmake/NanoRosLink.cmake` += execute_process(COMMAND "${_NROS_CLI}" rerun-verb) | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
|  | `check-build-tool-verbs-exempt.py` · zephyr-ws | `zephyr/cmake/nros_generate_interfaces.cmake` += execute_process(COMMAND "${_NROS_CLI}" ws rerun-sub) | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| DEAD | `check-board-manifest-drift.sh` | — (gate deleted since the audit) | 0 | — | N/A |
| P1 | `check-test-precondition-guards.py` | `packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs` += fn rerun_router() -> Option<u16> { | 0 | 1 | fails now |
| P2 | `check-tier-spin-gap.py` | `packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c`: (void)gap_state;; `packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c`: (void)gap_state; | 0 | 1 | fails now |
| P2 | `check-wait-evidence-discarded.py` · or_else-chain | `packages/testing/nros-tests/tests/qos.rs` += fn rerun_discard(p: &mut nros_tests::process::ManagedProcess) -> Strin | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
|  | `check-wait-evidence-discarded.py` · baseline-forced-down | `packages/testing/nros-tests/tests/services.rs`: .expect("x"); | 0 | 1 | fails now |
| P2 | `check-zenohd-router-skips.py` | `packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs` += fn rerun_r() { | 0 | 1 | fails now |
| P2 | `check-zenoh-platform-macros.py` | `zephyr/cmake/nros_rmw_zenoh.cmake`: zephyr_compile_definitions(ZENOH_ZEPHYR ZENOH_LINUX) | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P2 | `check-weak-symbols.sh` | `zephyr/nros_zenoh_zephyr_system.c` += void __attribute__((weak, used)) rerun_weak(void) {} | 0 | 1 | fails now |
| P2 | `check-tier-has-ci-owner.py` | `.github/workflows/run-matrix.yml`: just ci matrix build | 0 | 1 | fails now |
| P2 | `check-workflow-repo-env.py` · action | `.github/actions/setup-nros-cli/action.yml` += # source ./activate.sh | 0 | 1 | fails now |
| P2 | `check-workflow-indexed-apt.py` | `.github/actions/setup-nros-cli/action.yml` += sudo apt-get install -y ninja-build | 0 | 1 | fails now |
| P2 | `check-workflow-runner-isolation.py` | `.github/workflows/gate.yml`: runs-on: ${{ inputs.runner }} | 0 | 1 | fails now |
| P2 | `check-third-party-is-submodules.sh` | `zephyr/CMakeLists.txt` += set(_rerun_tp "${NANO_ROS_ROOT}/third-party/ninja") | 0 | 1 | fails now |
| P2 | `check-test-domain-assignment.sh` | `packages/testing/nros-tests/tests/qos.rs` += const RERUN_DOMAIN: &str = "export ROS_DOMAIN_ID=117 && ros2 topic ech | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-zenohd-flag-invocations.py` | `book/src/introduction.md` += Run `a rmw_zenohd router started with a listen flag` first. | 0 | 0 | **HOLE** → fixed here (rc 1 after fix); control rc 1 |
| P2 | `check-tier-priority-plan-image.py` | — | 0 | — | N/A — needs built Zephyr images (build-*/zephyr/.config) — none on this host |
| P2 | `check-zephyr-module-binding.py` | `justfile` += # rerun probe | 0 | 1 | fails now |
| P3 | `check-workflow-setup-spelling.py` | `.github/actions/setup-nros-cli/action.yml` += the module spelling of the zephyr setup recipe | 0 | 1 | fails now |
| P3 | `check-zenohd-spawn-sites.sh` | `packages/testing/nros-tests/tests/qos.rs` += fn rerun_spawn() { | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-zephyr-workspace-resolvers.py` | `zephyr/CMakeLists.txt` += set(_rerun_ws "${CMAKE_SOURCE_DIR}/../nano-ros-workspace") | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P2 | `check-nested-workspace-excludes.sh` | — | 0 | — | N/A — gate deleted since the audit |
| P2 | `check-profile-board-mirror.sh` | — | 0 | — | N/A — gate deleted since the audit |
| P2 | `check-msg-dep-is-path.sh` · px4_msgs | `examples/native/rust/talker/Cargo.toml`: px4_msgs = "*" | 0 | 1 | fails now |
|  | `check-msg-dep-is-path.sh` · dotted | `examples/native/rust/talker/Cargo.toml`: std_msgs.version = "*" | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-markdown-links.py` · refdef | `docs/design/README.md` += See [the rerun page][rerunref]. | 0 | 1 | fails now |
| P2 | `check-no-vacuous-tests.py` · src-unit | `packages/testing/nros-tests/src/lib.rs` += mod rerun_vacuous { | 0 | 1 | fails now |
|  | `check-no-vacuous-tests.py` · return-as-effect | `packages/testing/nros-tests/tests/qos.rs` += fn rerun_prints_and_returns() { | 0 | 0 | **HOLE** (W3, #1617); control rc 1 |
| P2 | `check-ps-zombie-blind.sh` | `packages/testing/nros-tests/tests/qos.rs` += fn rerun_ps() { | 0 | 1 | fails now |
| P2 | `check-no-unbounded-condvar-wait.sh` | `packages/boards/nros-board-freertos/c/freertos_run_tiers.c` += static void rerun_wait(void *c, void *m) { (void)nros_platform_condvar | 0 | 1 | fails now |
| P2 | `check-no-direct-kernel-alloc.sh` · define | `packages/boards/nros-board-freertos/c/freertos_run_tiers.c` += #define RERUN_ALLOC(n) pvPortMalloc(n) | 0 | 0 | **HOLE** (W7, #1616); control rc 1,1 |
|  | `check-no-direct-kernel-alloc.sh` · k_calloc | `packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c` += void *rerun_kc(void) { return k_calloc(1, 8); } | 0 | 0 | **HOLE** (W7, #1616); control rc 1,1 |
| P2 | `check-no-alloc-image.py` · k_realloc | new `tmp/rerun/ka.marker` | 0 | 0 | **HOLE** (W7, #1616) |
|  | `check-no-alloc-image.py` · control-k_malloc | new `tmp/rerun/kb.marker` | 0 | 1 | fails now |
| P2 | `check-named-lane-fails.py` · rule4-comment | `justfile`: run_stage "$platform" just "$platform" build-fixtures; `justfile`: run_stage zephyr just zephyr build-fixtures | 0 | 1 | fails now |
| P2 | `check-nros-c-feature-agreement.py` · justfile | `justfile` += # rerun probe | 0 | 1 | fails now |
| P2 | `check-no-std-stdio.py` · writeln | `packages/core/nros-core/src/lib.rs` += fn rerun_stdio() { | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
|  | `check-no-std-stdio.py` · use-eprintln | `packages/core/nros-core/src/lib.rs` += use std::eprintln as _rerun_eprintln; | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-no-tracked-file-find.sh` | `scripts/build/cargo.sh` += _rerun_find() { find . -iname 'package.xml'; } | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-nuttx-links-snapshot.sh` | `packages/boards/nros-board-nuttx-qemu/build.rs` += fn rerun_live(p: &std::path::Path) -> std::path::PathBuf { | 0 | 0 | **HOLE** (W7, #1616); control rc 1 |
| P2 | `check-nuttx-shared-tree-headers.py` · examples | `examples/qemu-armv7a-nuttx/c/talker/CMakeLists.txt` += include_directories(${NUTTX_DIR}/include) | 0 | 1 | fails now |
|  | `check-nuttx-shared-tree-headers.py` · makefile | `integrations/nuttx/Makefile` += CFLAGS += -I$(NUTTX_DIR)/include | 0 | 1 | fails now |
| P2 | `check-one-producer-per-tool.py` | `just/workspace.just`: curl -L https://example.invalid/ninja-linux.zip -o /tmp/n.zip && unzip | 0 | 1 | fails now |
| P2 | `check-orphan-generated-stamp.py` · corrosion-cargo | remove `build/corrosion-cargo/threadx-riscv64/56c8585c79ad/rerun_8e5ec/nros-c-generated/nros/nros_config_generated.h` | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P2 | `check-posix-platform-purity.py` | `packages/platform/nros-platform-posix/src/net.c`: (void) eventfd(0, 0); | 0 | 1 | fails now |
| P2 | `check-platform-provider-features.py` | `packages/api/nros-c/Cargo.toml`: # "global-allocator", | 0 | 1 | fails now |
| P2 | `check-prelude-tiers.py` · path-qualified | `packages/api/nros/src/lib.rs`: pub use crate::action::ActiveGoal; | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
|  | `check-prelude-tiers.py` · glob | `packages/api/nros/src/lib.rs`: pub use crate::embedded::*; | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-nextest-test-filters.py` | — | 0 | — | N/A — needs compiled nextest binaries (runs from test-all); not built here |
| P2 | `check-no-std-entry-emission.py` | `git mv packages/cli/nros-cli-core/src/codegen/entry/packs`; `git mv packages/core/nros-macros/src` | 0 | 1 | fails now |
| P3 | `check-pipefail-sigpipe-assertions.py` | `.github/actions/setup-nros-cli/action.yml` += if ! printf 'a\n' \| grep -q a; then echo no; fi | 0 | 1 | fails now |
| P3 | `check-prose-issue-refs.py` | `justfile` += # see an issue id with no file for why | 0 | 1 | fails now |
| P3 | `check-no-silent-sample-drop.py` | `examples/native/c/listener/src/main.c` += static void rerun_cb(const uint8_t *data, size_t len) { | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-no-board-init.sh` · multiline-use | `packages/boards/nros-board-linux/src/lib.rs` += use nros_board_common::{ | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-message-crate-identity.py` · workspace-deps | `Cargo.toml`: nros-std-msgs = { path = "packages/interfaces/generated/humble/nros-st | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P3 | `check-nextest-binary-filters.py` · other-workspace | `.config/nextest.toml`: filter = "binary(integration_tests)" | 0 | 0 | **HOLE** (W8, #1617); control rc 1 |
| P2 | `check-sdk-store-not-enumerated.py` | `scripts/build/riscv64-toolchain.sh` += _rerun_newest() { ls -d "$HOME/.nros/sdk/riscv-none-elf-gcc"/* \| sort  | 0 | 1 | fails now |
| P2 | `check-rmw-slot-producers.py` · test-only-reader | `packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs` += fn rerun_reader(vtable: &nros_rmw_cffi::NrosRmwVtable) -> bool { | 0 | 1 | **HOLE** (W8, #1617): expected rc 0 (the slot stays inert); rc 1 is the stale-family error — the test-only reader made `feature_supported` count as reachable |
| P2 | `check-rmw-doc-slot-names.py` | `packages/core/nros-rmw-abi/include/nros/rmw_vtable.h`: /* see `rerun_slot_name` */; `packages/core/nros-rmw/src/lib.rs` += // rerun_slot_name is only named in this comment | 0 | 1 | fails now |
| P2 | `check-rmw-ret-sign.py` | `packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c` += static int rerun_sign(void *s) { | 0 | 1 | fails now |
| P2 | `check-qos-profile-ssot.py` · rule6-outside-traits | `packages/api/nros/src/lib.rs` += pub const QOS_PROFILE_RERUN: QoSProfile = QoSProfile { history: QoSHis | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P2 | `check-required-contexts-reportable.py` · paths-filter | `.github/workflows/gate.yml`: paths: ["docs/**"] | 0 | 1 | fails now |
| P2 | `check-release-manifest.py` · exit2 | `.github/workflows/release-nros.yml`: if [ "$(cat VERSION)" != "$(git describe --tags)" ]; then | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-single-rust-staticlib.py` · multiline | `cmake/NanoRosLink.cmake` += target_link_libraries(rerun_target PRIVATE | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-staleness-probe-exemptions.sh` · row-probe | `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`: (delete) staleness::record_fresh(&resolved).map_err(TestError::BuildFa | 0 | 0 | **HOLE** (W7, #1616); control rc 1 |
| P2 | `check-rust-targets-covered.py` · fvp-nested | `packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/nros-board.toml`: rust_targets = ["aarch64-unknown-none", "riscv32imc-rerun-none-elf"] | 0 | 1 | fails now |
| P2 | `check-skippable-tests-tolerant.py` · mod-justfile | `just/zephyr-setup.just` += # rerun probe | 0 | 1 | fails now |
| P2 | `check-skip-marker-matching.py` · python | `scripts/test/name-real-failures.py` += def _rerun(line): | 0 | 0 | **HOLE** → fixed here (rc 1 after fix); control rc 1 |
| P2 | `check-required-features-reachable.py` · comment | `packages/testing/nros-tests/Cargo.toml`: rerun-feature = []; `packages/testing/nros-tests/Cargo.toml`: required-features = ["rerun-feature"]; `justfile` += # cargo test --all-features | 0 | 1 | fails now |
| P3 | `check-sysdep-remedies.sh` · just-check | `just/check/cmake.just` += # rerun probe | 0 | 1 | fails now |
| P3 | `check-retired-submodule-refs.sh` · nested | `scripts/build/cargo.sh` += : "packages/cli/third-party/ros-launch-resolve/third-party/ros-launch- | 0 | 1 | fails now |
| P3 | `check-rmw-agnostic.py` · cfg-any-test | `packages/core/nros-core/src/lib.rs` += pub const RERUN_BACKEND: &str = "zenoh"; | 0 | 1 | fails now |
| P3 | `check-ros2-daemon-queries.py` · path+verb | `packages/testing/nros-tests/tests/ros2_action_e2e.rs` += const RERUN_Q: &str = "source /opt/ros/humble/setup.bash && ros2 topic | 0 | 1 | fails now |
| P3 | `check-ros-env-spelling.py` · triple-quoted | `scripts/test/name-real-failures.py` += _RERUN_CMD = """ | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-set-e-bare-assignment.py` · scripts-bin-cargo | `scripts/bin/cargo` += _rerun() { | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P3 | `check-self-pkg-package-xml.py` · no-component | new `examples/rerun-selfpkg/system.toml`; new `examples/rerun-selfpkg/Cargo.toml` | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-sdk-guard-can-fire.py` · just-check-unbraced | `just/check/cmake.just` += # rerun probe | 0 | 1 | fails now |
| P3 | `check-runtime-umbrella-link-sites.py` · variable | `cmake/NanoRosLink.cmake` += set(_rerun_umb NanoRos::NanoRosCpp) | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-ret-code-citations.py` · jinja | `packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/entry.c.jinja` += /* returns an undefined `RET_*` code (prefix dropped here) on failure */ | 0 | 0 | **HOLE** → fixed here (rc 1 after fix); control rc 1 |
| P3 | `check-retired-cmake-keywords.py` · rust-template | `packages/cli/nros-cli-core/src/builder/cmake_root.rs` += const RERUN_CMAKE: &str = "nano_ros_entry(app HOST native)\n"; | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P2 | `check-config-header-producers.py` · dup-define | `packages/api/nros-c/include/nros/nros_config_generated_buildless.h`: #define NROS_CODEGEN_VERSION 7 | 0 | 1 | fails now |
| P2 | `check-config-fallback-macros.py` · dup-define | `packages/api/nros-cpp/include/nros/nros_cpp_config_generated_buildless.h`: #define NROS_CODEGEN_VERSION 7 | 0 | 1 | fails now |
| P2 | `check-doc-recipe-refs.py` · package-doc | `packages/api/nros-c/README.md` += Run `a call of the retired `build-zenohd` recipe` first. | 0 | 1 | fails now |
| P2 | `check-feature-set-ssot.sh` · zephyr | `zephyr/CMakeLists.txt` += list(APPEND _rerun_feats ros-humble) | 0 | 1 | fails now |
| P2 | `check-dist-floors.py` · rust-rustup | `nros-sdk-index.toml`: dist.linux-riscv64 = { url = "https://example.invalid/rustup-init", sh | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P2 | `check-emitter-just-spelling.sh` · zephyr | `zephyr/CMakeLists.txt` += message(FATAL_ERROR "nros CLI missing: run just setup-cli") | 0 | 1 | fails now |
| P2 | `check-entry-rmw-vocabulary.py` · doc-comment-registration | `packages/rmw/xrce/nros-rmw-xrce/src/vtable.c`: return 0;; `packages/rmw/xrce/nros-rmw-xrce/src/vtable.c`: (void)0; | 0 | 1 | fails now |
| P2 | `check-codegen-version-surface.py` · nros-rmw | `packages/core/nros-rmw/src/type_descriptor.rs`: _rerun: u8, | 0 | 0 | **HOLE** (W7, #1616); control rc 1 |
| P2 | `check-feature-gated-modules.sh` · submodule | `packages/core/nros-node/src/executor/mod.rs` += mod rerun_mod;; new `packages/core/nros-node/src/executor/rerun_mod.rs` | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P2 | `check-executor-stack-floor.py` · 2nd-guard | `packages/tooling/nros-build-helpers/src/cpp.rs`: #if 0 | 0 | 1 | fails now |
| P2 | `check-entry-session-name.py` · c-jinja | `packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/boot_wrapper.jinja`: ("", nros_boot_config_namespace; `packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/boot_wrapper.jinja`: (uint8_t)NROS_ENTRY_DOMAIN_ID, "", | 0 | 0 | **HOLE** (W7, #1616); control rc 1 |
| P2 | `check-config-header-single-writer.py` · copy_if_different | `cmake/NanoRosLink.cmake` += add_custom_command(OUTPUT rerun_hdr COMMAND ${CMAKE_COMMAND} -E copy_i | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-codegen-tool-reconfigure.py` · sizing-descriptor | `cmake/NanoRosSizingDescriptor.cmake`: message(STATUS "x") | 0 | 1 | fails now |
| P2 | `check-feature-contract.py` · cfg_attr-global_allocator | `packages/boards/nros-board-linux/src/lib.rs` += static RERUN_ALLOC: std::alloc::System = std::alloc::System; | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
|  | `check-feature-contract.py` · build-prune | `packages/cli/nros-cli-core/src/builder/mod.rs` += static RERUN_ALLOC: std::alloc::System = std::alloc::System; | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-cyclone-backend-sources.py` · commented | `packages/rmw/cyclonedds/nros-rmw-cyclonedds-sys/build.rs`: // "publisher.cpp", | 0 | 1 | fails now |
| P2 | `check-decoupling.sh` · harvested-platforms | `packages/core/nros-node/Cargo.toml`: nros-platform-mps2-an385 = { path = "../../platform/nros-platform-mps2 | 0 | 1 | fails now |
| P2 | `check-dds-isolation-symmetry.py` · per-fn | `packages/testing/nros-tests/tests/advertised_state_interop.rs` += fn rerun_half_pinned() { | 0 | 1 | fails now |
| P2 | `check-cxx-compat-shim-coverage.py` · workspaces | `examples/workspaces/cpp/src/listener_pkg/src/Listener.cpp` += static int rerun_cmp(const char *a, const char *b) { return std::strco | 0 | 1 | fails now |
| P2 | `check-cpp-no-std-stdio.py` · public-header | `packages/api/nros-cpp/include/nros/node.hpp`: inline void rerun_print() { std::fprintf(stderr, "x"); } | 0 | 1 | fails now |
| P2 | `check-entry-locator-ssot.py` · nano_rosConfig | `nano_rosConfig.cmake` += set(NROS_ENTRY_LOCATOR "tcp/10.0.2.2:7447") | 0 | 1 | fails now |
| P3 | `check-cxx-standard-floor.py` · target-property | `cmake/NanoRosLink.cmake` += set_target_properties(rerun_t PROPERTIES CXX_STANDARD 11) | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
|  | `check-cxx-standard-floor.py` · cache | `cmake/NanoRosLink.cmake` += set(CMAKE_CXX_STANDARD 11 CACHE STRING "rerun") | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-component-entity-bounds.py` · qualified-impl | `examples/rv-virt-threadx/rust/listener/src/lib.rs`: impl nros::Node for Listener {; `examples/rv-virt-threadx/rust/listener/src/lib.rs`: (delete) const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds: | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-component-lang-vocabulary.py` · get_property | `cmake/NanoRosLink.cmake` += get_property(_rerun_lang TARGET rerun PROPERTY NROS_COMPONENT_LANG) | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-cpp-ffi-error-mapping.py` · Err_e | `packages/api/nros-cpp/src/lib.rs` += fn rerun_map(r: Result<(), ()>) -> i32 { | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-cross-toolchain-provenance.py` · 5th-site | `cmake/board/nano-ros-board-mps2-an385-freertos.cmake` += set(CMAKE_C_COMPILER "/usr/bin/arm-none-eabi-gcc") | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P3 | `check-declared-fact-carriers.py` · commented-rerun | `packages/core/nros-node/build.rs`: // println!("cargo:rerun-if-env-changed=NROS_DECLARED_MAX_QOS_DEPTH"); | 0 | 1 | fails now |
| P3 | `check-default-gates-run-somewhere.py` · if-false | `.github/workflows/gate.yml`: if: false | 0 | 0 | **HOLE** (W8, #1617); demonstrated at the classifier: `_events_of('if: false')` credits every event (no gate here has a single placement to remove as a control) |
| P3 | `check-deferred-call-args.py` · eval-escaped | `cmake/NanoRosLink.cmake` += function(_rerun_caller _tgt) | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-entity-slot-costs.py` · other-file | `packages/core/nros-node/src/executor/mod.rs` += impl<'s> Executor<'s> { | 0 | 0 | **HOLE** (W7, #1616); control rc 1 |
| P3 | `check-eyre-context-alias.sh` · multiline-use | `packages/cli/nros-cli-core/src/lib.rs` += use eyre::{ | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-gate-selftests.py` · args.self_test-guard | `scripts/check-fixture-stamp-honesty.py`: import argparse | 0 | 1 | fails now |
| P2 | `check-literal-domain-id.py` · after-mod-tests | `packages/core/nros-node/src/executor/mod.rs` += fn rerun_ship(q: crate::QosSettings) -> crate::QosSettings { | 0 | 1 | fails now |
| P2 | `check-ffi-struct-mirrors.sh` · subscription-options | `packages/api/nros-c/include/nros/component.h`: } nros_cpp_subscription_options_t; | 0 | 1 | fails now |
| P2 | `check-fixture-binary-names.py` · wrapper | `packages/testing/nros-tests/tests/bridge_zenoh_to_cyclonedds.rs`: build_native_c_example_rmw("listener", "c_listener_rerun_bogus", Rmw:: | 0 | 1 | fails now |
| P2 | `check-fixture-require.py` · match-bypass | `packages/testing/nros-tests/tests/qos.rs` += fn rerun_bypass() { | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-fixture-stamp-honesty.py` · guard-neutralised | `scripts/build/fixture-lane.sh`: if false && [ -n "$stamp_skipped" ]; then | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-fixture-variant-features.py` · rmw-one-sided | `examples/native/rust/talker/Cargo.toml`: (delete) rmw-xrce = ["dep:nros-rmw-xrce-cffi", "nros-board-linux/rmw-x; `packages/testing/nros-tests/src/fixtures/binaries/mod.rs` += fn rerun_variant() { | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-interop-verdicts.py` · missing-ledger | `git mv .config/interop-verdicts.toml` | 0 | 1 | fails now |
| P2 | `check-issue-index.sh` · dup-digest | `docs/issues/README.md`: Recently resolved (2026-09-04): **#1045** (testing) | 0 | 1 | fails now |
| P2 | `check-just-recipe-refs.py` · module-recipe | `just/check/cmake.just` += # rerun probe | 0 | 1 | fails now |
| P2 | `check-just-recipe-paths.py` · just-check | `just/check/cmake.just` += # rerun probe | 0 | 1 | fails now |
| P2 | `check-knob-resolved-once.py` · if-else-plus-third | `zephyr/cmake/nros_cargo_build.cmake` += if(RERUN_A) | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P2 | `check-lane-contracts.py` · build-resolver | `packages/testing/nros-tests/tests/zephyr_prjconf_requirements.rs` += fn rerun_needs_fixture() { | 0 | 1 | fails now |
| P2 | `check-lane-scope-consumers.py` · native-file | new `packages/testing/nros-tests/tests/native_rerun_matrix.rs` | 0 | 1 | fails now |
| P3 | `check-fixture-artifact-dir-inputs.py` · neighbour-recipe | `just/freertos.just` += # rerun probe | 0 | 1 | fails now |
| P3 | `check-fixture-id-guard.sh` · workspace-builder | `scripts/build/workspace-fixtures-build.sh`: exit 0 | 0 | 0 | **HOLE** (W7, #1616); control rc 1 |
| P3 | `check-gate-cache-keys-agree.py` · consumer-direction | `.github/workflows/gate.yml`: - name: rerun extra restore | 0 | 0 | **HOLE** (W7, #1616); control rc 1 |
| P3 | `check-generated-cmake-keywords.py` · plus-equals | `packages/cli/nros-cli-core/src/builder/cmake_root.rs`: fn rerun_emit(out: &mut String) { | 0 | 0 | **HOLE** (W6, #1615) |
|  | `check-generated-cmake-keywords.py` · control-push_str | `packages/cli/nros-cli-core/src/builder/cmake_root.rs`: fn rerun_emit(out: &mut String) { | 0 | 1 | fails now |
| P3 | `check-generated-leaf-regenerable.sh` · outside-pathspec | new `integrations/rerun_leaf/Cargo.toml` | 0 | 0 | **HOLE** (W5, #1614); control rc 1 |
| P3 | `check-generated-schema-coverage.py` · per-struct | `packages/interfaces/generated/humble/nros-diagnostic-msgs/src/srv/add_diagnostics.rs`: const FIELDZ_RERUN: &'static [::nros_serdes::Field] = &[ | 0 | 1 | fails now |
| P3 | `check-git-dir-layout-assumptions.py` · action | `.github/actions/setup-nros-cli/action.yml` += cat .git/HEAD | 0 | 1 | fails now |
| P3 | `check-goal-cdr-stripped.py` · no-floor | `packages/api/nros-c/src/action/client.rs`: pub extern "C" fn nros_action_client_send_goal_raw( | 0 | 0 | **HOLE** (W4, #1617); control rc 1 |
| P3 | `check-grep-q-error-conflation.py` · action | `.github/actions/setup-nros-cli/action.yml` += if grep -q foo bar.txt; then echo y; fi | 0 | 1 | fails now |
| P3 | `check-host-platform-vocabulary.py` · fvp-depth3 | `packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/nros-board.toml`: names = ["fvp-aemv8r-smp", "posix", "linux"] | 0 | 1 | fails now |
| P3 | `check-host-triple-literals.py` · no-rustc | new `packages/tooling/rerun-probe/.cargo/config.toml` | 0 | 1 | fails now |
| P3 | `check-image-paths-apply-policy.sh` · per-target | `cmake/platform/nano-ros-nuttx.cmake` += add_executable(rerun_img rerun.c) | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-interface-glob-configure-depends.py` · multiline | `cmake/NanoRosGenerateInterfaces.cmake` += file(GLOB _rerun_msgs | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-interlock-visibility.py` · needs | `.github/workflows/build-wide.yml`: needs: [] | 0 | 0 | **HOLE** (W6, #1615); control rc 1 |
| P3 | `check-interop-cell-runners.py` · echo | `just/native.just`: @echo "run binary(=advertised_state_interop) by hand" | 0 | 0 | **HOLE** (W3, #1617); control rc 1 |
| P3 | `check-lane-coverage-labels.py` · phrase | `.github/workflows/gate.yml`: name: check (fast + full compile tier for every PR) | 0 | 0 | **HOLE** (W7, #1616); control rc 1 |
| P3 | `check-lane-skip-protocol.py` · brace-exit0 | `just/freertos.just` += # rerun probe | 0 | 1 | fails now |
| P3 | `check-leaf-lockfiles.sh` · packages-cli | `packages/cli/colcon-cargo-ros2/test/rust-sample-package/Cargo.toml` += [target.'cfg(any())'.dependencies] | 0 | 1 | fails now |
| P3 | `check-ledger-key-spelling.py` · unprefixed | `docs/reference/api-parity-ledger/metadata.json`: "NodeCtx::create_timer_rerun": {"verdict": "extension", "why": "x"}, | 0 | 1 | fails now |
| P3 | `check-ledger-orphan-refs.py` · crate-relative | `docs/reference/api-parity-ledger/metadata.json`: "why": "see `src/rerun_missing.rs` -- | 0 | 0 | **HOLE** (W7, #1616); control rc 1 |

## New audits: the W5 "triaged, not audited" `cmake/` gates

| gate · facet | mutation | rc | verdict |
| --- | --- | ---: | --- |
| `check-board-alias-unique.py` · board-alias-unique/nested-fvp | `packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/nros-board.toml`: names = ["fvp-aemv8r-smp", "rv-virt-nuttx"] | 1 | clean (fails) |
| `check-board-vocabulary.py` · board-vocabulary/system-toml-board | `examples/rv-virt-threadx/rust/talker/system.toml`: board = "rerun-nonexistent-board" | 0 | **HOLE** |
| `check-build-type-spelling.py` · build-type-spelling/new-board-ament_cargo | new `packages/boards/nros-board-rerun/package.xml` | 0 | **HOLE** |
| `check-cmake-generated-source-owners.py` · cmake-generated-source-owners/two-targets | `zephyr/cmake/nros_rmw_zenoh.cmake` += add_custom_command(OUTPUT ${CMAKE_BINARY_DIR}/rerun_gen.c COMMAND ${CM | 0 | **HOLE** |
| `check-image-locator-bake.py` · image-locator-bake/freertos-no-locator | `examples/workspaces/rust/src/demo_bringup/system.toml`: (delete) locator = "tcp/192.0.3.1:7830" | 1 | clean (fails) |
| `check-knob-delivery.py` · knob-delivery | — | — | N/A — needs a configured Zephyr build dir (`check-knob-delivery.py <build-dir>`); none on this host |
| `check-no-allow-multiple-def.sh` · no-allow-multiple-def/zephyr-cmake | `zephyr/cmake/nros_cargo_build.cmake` += target_link_options(rerun PRIVATE -Wl,--allow-multiple-definition) | 0 | **HOLE** (W5, #1614) |
| `check-retired-platform-clock-symbols.py` · retired-platform-clock-symbols/zephyr-c | `zephyr/nros_zenoh_zephyr_system.c` += extern uint64_t nros_platform_clock_ms(void); | 1 | clean (fails) |
| `check-scaffold-builds.sh` · scaffold-builds | — | — | N/A — exercising it means rebuilding the nros CLI with a broken template and compiling every scaffold variant; not run in this budget |
| `check-schema-reader-provenance.py` · schema-reader-provenance/zephyr-cmake | `zephyr/cmake/nros_rmw_zenoh.cmake` += set(NROS_RERUN_SCHEMA_SUPPORTED 1) | 1 | clean (fails) |
| `check-vendor-fetch-pinned.py` · vendor-fetch-pinned/zephyr | `zephyr/CMakeLists.txt` += FetchContent_Declare(rerun URL https://example.invalid/x.tar.gz) | 1 | clean (fails) |
| `check-workspace-rmw-agreement.py` · workspace-rmw-agreement/disagree | new `examples/rerun-ws/CMakeLists.txt`; new `examples/rerun-ws/src/demo_bringup/system.toml` | 1 | clean (fails) |
| `check-xrce-config-manifest.py` · xrce-config-manifest/hand-value | `packages/rmw/xrce/nros-rmw-xrce/CMakeLists.txt` += set(UXR_CONFIG_SERIAL_TRANSPORT_MTU 512) | 0 | **HOLE** |
| `check-xrce-source-manifest.py` · xrce-source-manifest/hand-source | `packages/rmw/xrce/nros-rmw-xrce/CMakeLists.txt`: ${CMAKE_CURRENT_SOURCE_DIR}/src/rerun_extra.c | 1 | clean (fails) |
| `check-xrce-vendored-versions.py` · xrce-vendored-versions/literal | `packages/rmw/xrce/nros-rmw-xrce/CMakeLists.txt`: set(PROJECT_VERSION_MAJOR  2) | 1 | clean (fails) |
| `check-zenoh-feature-off-compile.py` · zenoh-feature-off-compile/zephyr-CMakeLists | `zephyr/CMakeLists.txt` += zephyr_compile_definitions(Z_FEATURE_LINK_UDP_UNICAST=0 Z_FEATURE_LINK | 0 | **HOLE** (W5, #1614) |
| `check-zenoh-feature-off-compile.py` · zenoh-feature-off-compile/owner-file | `zephyr/cmake/nros_rmw_zenoh.cmake` += zephyr_compile_definitions(Z_FEATURE_LINK_UDP_UNICAST=0 Z_FEATURE_LINK | 1 | control: fails ✓ |
| `check-zenoh-source-manifest.py` · zenoh-source-manifest/hand-glob | `zephyr/cmake/nros_rmw_zenoh.cmake` += file(GLOB_RECURSE _rerun_zp "${ZENOH_PICO_DIR}/src/*.c") | 1 | clean (fails) |
| `check-board-alias-unique.py` · board-alias-unique/depth1 | `packages/boards/nros-board-threadx-linux/nros-board.toml`: names = ["rv-virt-nuttx", "threadx", "threadx-linux"] | 1 | control: fails ✓ |
| `check-board-vocabulary.py` · board-vocabulary/c-leaf | `examples/rv-virt-threadx/c/talker/system.toml`: board = "rerun-nonexistent-board" | 1 | clean (fails) |
| `check-build-type-spelling.py` · build-type-spelling/board-provider-ament_cargo | new `packages/boards/nros-board-rerun/package.xml` | 1 | clean (fails) |
| `check-cmake-generated-source-owners.py` · cmake-generated-source-owners/cmake-dir | `cmake/NanoRosLink.cmake` += add_custom_command(OUTPUT ${CMAKE_BINARY_DIR}/rerun_gen.c COMMAND ${CM | 0 | control: passes ✗ |
| `check-no-allow-multiple-def.sh` · no-allow-multiple-def/cmake-dir | `cmake/NanoRosLink.cmake` += target_link_options(rerun PRIVATE -Wl,--allow-multiple-definition) | 1 | control: fails ✓ |
| `check-xrce-config-manifest.py` · xrce-config-manifest/UCLIENT-literal | `packages/rmw/xrce/nros-rmw-xrce/CMakeLists.txt` += set(UCLIENT_SERIAL_TRANSPORT_MTU 512) | 1 | clean (fails) |
| `check-cmake-generated-source-owners.py` · cmake-generated-source-owners/zephyr-helper | `zephyr/cmake/nros_rmw_zenoh.cmake` += nros_rmw_cyclonedds_idlc_compile(_rerun_srcs x.idl) | 1 | clean (fails) |
| `check-cmake-generated-source-owners.py` · cmake-generated-source-owners/cmake-helper | `cmake/NanoRosLink.cmake` += nros_rmw_cyclonedds_idlc_compile(_rerun_srcs x.idl) | 1 | control: fails ✓ |

## Spot-checks of gates recorded clean on 2026-09-28

| gate · facet | mutation | rc | verdict |
| --- | --- | ---: | --- |
| `check-workflow-just-provisioning.py` · workflow-just-provisioning/container-without-just | `.github/workflows/nightly.yml`: image: ubuntu:22.04 | 1 | clean (fails) |
| `check-workflow-doctor-after-setup.py` · workflow-doctor-after-setup/composite-action | `.github/actions/setup-nros-cli/action.yml` += run: bash scripts/ci/runner-doctor.sh | 0 | **HOLE** (W1, #1617) |
| `check-workflow-doctor-after-setup.py` · workflow-doctor-after-setup/workflow | new `.github/workflows/rerun-probe.yml` | 1 | control: fails ✓ |
| `check-test-scripts-have-callers.py` · test-scripts-have-callers/comment-only | new `tests/rerun-orphan-tests.sh`; `just/check/cmake.just` += # see tests/rerun-orphan-tests.sh | 1 | clean (fails) |
| `check-cyclone-domain-not-pinned.py` · cyclone-domain-not-pinned/example-conf | `examples/zephyr/c/talker/prj-cyclonedds.conf` += CONFIG_NROS_CYCLONE_DOMAIN_ID=0 | 1 | clean (fails) |
| `check-core-crates-are-no-std.py` · core-crates-are-no-std/block-comment | `packages/core/nros-core/src/lib.rs`: /* | 0 | **HOLE** (W3, #1617) |
| `check-manifests-parse.py` · manifests-parse/nested-leaf | new `examples/rerun-bad/Cargo.toml` | 1 | clean (fails) |
| `check-no-tracked-models.sh` · no-tracked-models/system_model | new `examples/rerun-ws/build/nros/models/demo/system_model.yaml` | 1 | clean (fails) |
| `check-rust-stdio-on-zephyr.py` · rust-stdio-on-zephyr/board-crate | `packages/boards/nros-board-zephyr/src/lib.rs` += fn rerun_print() { | 0 | **HOLE** (W7, #1616) |
| `check-zephyr-kconfig-symbols.py` · zephyr-kconfig-symbols/select | `zephyr/Kconfig`: select RERUN_NONEXISTENT_SYMBOL | 1 | clean (fails) |
| `check-vtable-positional-order.py` · vtable-positional-order/swap | `packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp`: /*drive_io*/                  session_destroy, | 1 | clean (fails) |
| `check-rmw-required-slots.sh` · rmw-required-slots/drop-required | `packages/rmw/cffi/src/lib.rs`: require!( | 1 | clean (fails) |
| `check-zephyr-module-allowlist.py` · zephyr-module-allowlist/extra-module | `west.yml`: - rerun_hal | 1 | clean (fails) |
| `check-std-census.py` · std-census/new-site | `packages/core/nros-core/src/lib.rs` += fn rerun_std() -> std::string::String { | 1 | clean (fails) |
| `check-cargo-config-tracked.sh` · cargo-config-tracked/generated-patch | `packages/boards/nros-board-nuttx-qemu/nros-nuttx-ffi/.cargo/config.toml` += [patch.crates-io] | 1 | clean (fails) |
| `check-core-crates-are-no-std.py` · core-crates-are-no-std/removed | `packages/core/nros-core/src/lib.rs`: (delete) #![no_std] | 1 | control: fails ✓ |
| `check-rust-stdio-on-zephyr.py` · rust-stdio-on-zephyr/nros-c | `packages/api/nros-c/src/lib.rs` += fn rerun_print() { | 1 | control: fails ✓ |

## Final re-run (2026-10-03) — after issues 1614–1618 and 1636

Every recorded mutation was re-applied against `main` after #1559 (1618),
#1563 (1617), #1583 (1616), #1595 (1614), #1607 (1615) and the 1636 PR,
using the same harness. Three things changed in the method:

- each gate now runs under the worktree's own `activate.sh` (the calling
  shell carried another checkout's `NROS_REPO_DIR`, and one gate honours it);
- four rows were re-expressed where the original mutation was not a defect:
  `goal-cdr-stripped` (drop `unsafe` AND the strip), `default-gates-run-somewhere`
  (proved at the classifier), `build-tool-verbs-exempt` top-verb (a GUARDED
  verb), and `no-allow-multiple-def` (the gate is now `.py`);
- R2 rows added by the fixes (third use, `-z muldefs`, fallen count, and so
  on) are included.

| | entries |
| --- | ---: |
| harness entries run | 292 |
| recorded findings (incl. facets and re-expressions): **FAIL now** | **163** |
| recorded findings: expected PASS (`rmw-slot-producers` test-only reader) | 1 |
| recorded findings: N/A (2 gates deleted, 2 need artifacts this host lacks) | 4 |
| spot-checks: FAIL now | 14 of 14 |
| new audits: FAIL now / **HOLE** / N/A | 15 / **4** / 2 |
| controls: fail as they must / known-vacuous | 80 / 2 |
| R2 rows: fail / pass as intended | 4 / 3 |

**Zero recorded holes remain.** Four holes survive, all from this doc's
"New audits" table. They were never assigned to a class issue and are now
**issue 1660**: `board-vocabulary`, `build-type-spelling`,
`cmake-generated-source-owners` and `xrce-config-manifest`. The two
known-vacuous controls (`default-gates-run-somewhere/removed`,
`cmake-generated-source-owners/cmake-dir`) cannot fail by construction and are
not counted as holes.

## Issue 1660 closure (2026-10-03) — the last four holes

Each gate moved onto its class's existing helper, and each recorded mutation
was re-applied on the fix branch: confirmed applied (`git diff --numstat`,
`git status` for the new file), run against the gate as it is on `main`
(`git show origin/main:<gate>`) and against the fixed gate, then restored and
verified clean.

| class | gate · facet | helper | `main` rc | fixed rc | normal-path selftest row |
| --- | --- | --- | ---: | ---: | --- |
| W5 | `check-board-vocabulary` · system-toml-board | `file_kinds` (new `system-toml` kind) + `population` | 0 | **1** | the Rust leaf is in the population; `judge("nope", rust)` is `none` |
| W7 | `check-build-type-spelling` · new-board-ament_cargo | `harvest` (board packages by KIND: under `packages/boards/` or beside `nros-board.toml`) | 0 | **1** | an unmarked board package and a package beside a descriptor both fire `owned-declares-ament` |
| W6 | `check-cmake-generated-source-owners` · two-targets (both `cmake/` and `zephyr/cmake/`) | `per_item` (new `cmake_calls` / `cmake_keyword_items`: every OUTPUT item) + `file_kinds` + `population` | 0 | **1** | `RAW_BAD`, `RAW_VIA_VAR` (one `set()` hop) and `RAW_ONE` (a custom target is not a second owner) |
| W6/W7 | `check-xrce-config-manifest` · hand-value | `harvest` (the symbol vocabulary is every manifest token + every template macro) | 0 | **1** | the template spelling `UXR_CONFIG_*` is in the harvest, and the bare prefix regex is shown NOT to know it |

What each fix newly saw in the tree, all clean: `board-vocabulary` reads 196
`system.toml` (was 91) against a sixth namespace, the board catalog's `names`,
which is where the Rust leaves' spellings (`qemu-mps2-an385`, `freertos`, …)
resolve; the strict index-key assertion stays on the C/C++ leaves, by stated
reason. `build-type-spelling` harvests 14 board packages. `cmake-generated-
source-owners` examines 44 generated source sets (was the helper sites only).
`xrce-config-manifest` checks the lanes against 64 harvested symbols.

Stated, not hidden: `xrce-config-manifest` harvests the `UXR_CONFIG_*` names
from the upstream templates, so where `micro-xrce-dds-client` is not checked
out the harvest is the manifest's 41 tokens and the gate already prints a SKIP
for template coverage. The CMake gate follows one level of `set()` /
`list(APPEND)`; deeper variable flow is still out of reach.

**The New-audits table has no holes left. Phase-472's acceptance is met.**

