---
id: 1615
title: "Gate re-run 2026-10-01, W6: 34 gates still match one spelling, one line or one file where their rule is per item"
status: open
type: tech-debt
area: testing, build
severity: medium
found: 2026-10-01
related: [phase-472, 1614, 1615, 1616, 1617, 1618]
---

## What

The 2026-10-01 re-run of the phase-472 audit
([findings](../development/audit-findings-2026-10-01-rerun.md)) re-applied each
mutation the 2026-09-28 audit recorded, plus new audits and spot-checks. Every
gate below still exits **0** on the mutation in its row, and each has a positive
CONTROL that exits non-zero: the same defect placed where the gate does read.
So these are measured holes, not readings.

Class: **W6 — first match, or any match, where the rule is per item**.

| class | gate · facet | mutation | rc | control rc | source |
| --- | --- | --- | ---: | ---: | --- |
| W6 | `check-book-links.py` | `book/src/introduction.md` += See [the guide][rerunref]. | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-capability-slot-counts.sh` | `packages/core/nros-node/src/lifecycle_services.rs`: pub(crate) rerun_extra: LcSrv<GetState>, | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-cbindgen-pin.sh` | `packages/boards/nros-board-freertos/Cargo.toml` += [target.'cfg(any())'.build-dependencies] | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-ci-no-verb-fallback.py` | `.github/actions/setup-nros-cli/action.yml` += just zephyr build-all \ | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-component-entity-bounds.py` · qualified-impl | `examples/rv-virt-threadx/rust/listener/src/lib.rs`: impl nros::Node for Listener {; `examples/rv-virt-threadx/rust/listener/src/lib.rs`: (delete) const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds: | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-component-lang-vocabulary.py` · get_property | `cmake/NanoRosLink.cmake` += get_property(_rerun_lang TARGET rerun PROPERTY NROS_COMPONENT_LANG) | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-config-header-single-writer.py` · copy_if_different | `cmake/NanoRosLink.cmake` += add_custom_command(OUTPUT rerun_hdr COMMAND ${CMAKE_COMMAND} -E copy_i | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-cpp-ffi-error-mapping.py` · Err_e | `packages/api/nros-cpp/src/lib.rs` += fn rerun_map(r: Result<(), ()>) -> i32 { | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-cxx-standard-floor.py` · cache | `cmake/NanoRosLink.cmake` += set(CMAKE_CXX_STANDARD 11 CACHE STRING "rerun") | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-cxx-standard-floor.py` · target-property | `cmake/NanoRosLink.cmake` += set_target_properties(rerun_t PROPERTIES CXX_STANDARD 11) | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-deferred-call-args.py` · eval-escaped | `cmake/NanoRosLink.cmake` += function(_rerun_caller _tgt) | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-eyre-context-alias.sh` · multiline-use | `packages/cli/nros-cli-core/src/lib.rs` += use eyre::{ | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-feature-contract.py` · build-prune | `packages/cli/nros-cli-core/src/builder/mod.rs` += static RERUN_ALLOC: std::alloc::System = std::alloc::System; | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-feature-contract.py` · cfg_attr-global_allocator | `packages/boards/nros-board-linux/src/lib.rs` += static RERUN_ALLOC: std::alloc::System = std::alloc::System; | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-fixture-require.py` · match-bypass | `packages/testing/nros-tests/tests/qos.rs` += fn rerun_bypass() { | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-fixture-stamp-honesty.py` · guard-neutralised | `scripts/build/fixture-lane.sh`: if false && [ -n "$stamp_skipped" ]; then | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-fixture-variant-features.py` · rmw-one-sided | `examples/native/rust/talker/Cargo.toml`: (delete) rmw-xrce = ["dep:nros-rmw-xrce-cffi", "nros-board-linux/rmw-x; `packages/testing/nros-tests/src/fixtures/binaries/mod.rs` += fn rerun_variant() { | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-generated-cmake-keywords.py` · plus-equals | `packages/cli/nros-cli-core/src/builder/cmake_root.rs`: fn rerun_emit(out: &mut String) { | 0 | — | recorded 2026-09-28 |
| W6 | `check-image-paths-apply-policy.sh` · per-target | `cmake/platform/nano-ros-nuttx.cmake` += add_executable(rerun_img rerun.c) | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-interface-glob-configure-depends.py` · multiline | `cmake/NanoRosGenerateInterfaces.cmake` += file(GLOB _rerun_msgs | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-interlock-visibility.py` · needs | `.github/workflows/build-wide.yml`: needs: [] | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-knob-resolved-once.py` · if-else-plus-third | `zephyr/cmake/nros_cargo_build.cmake` += if(RERUN_A) | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-msg-dep-is-path.sh` · dotted | `examples/native/rust/talker/Cargo.toml`: std_msgs.version = "*" | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-no-board-init.sh` · multiline-use | `packages/boards/nros-board-linux/src/lib.rs` += use nros_board_common::{ | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-no-silent-sample-drop.py` | `examples/native/c/listener/src/main.c` += static void rerun_cb(const uint8_t *data, size_t len) { | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-no-std-stdio.py` · use-eprintln | `packages/core/nros-core/src/lib.rs` += use std::eprintln as _rerun_eprintln; | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-no-std-stdio.py` · writeln | `packages/core/nros-core/src/lib.rs` += fn rerun_stdio() { | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-no-tracked-file-find.sh` | `scripts/build/cargo.sh` += _rerun_find() { find . -iname 'package.xml'; } | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-prelude-tiers.py` · glob | `packages/api/nros/src/lib.rs`: pub use crate::embedded::*; | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-prelude-tiers.py` · path-qualified | `packages/api/nros/src/lib.rs`: pub use crate::action::ActiveGoal; | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-release-manifest.py` · exit2 | `.github/workflows/release-nros.yml`: if [ "$(cat VERSION)" != "$(git describe --tags)" ]; then | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-ros-env-spelling.py` · triple-quoted | `scripts/test/name-real-failures.py` += _RERUN_CMD = """ | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-runtime-umbrella-link-sites.py` · variable | `cmake/NanoRosLink.cmake` += set(_rerun_umb NanoRos::NanoRosCpp) | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-self-pkg-package-xml.py` · no-component | new `examples/rerun-selfpkg/system.toml`; new `examples/rerun-selfpkg/Cargo.toml` | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-single-rust-staticlib.py` · multiline | `cmake/NanoRosLink.cmake` += target_link_libraries(rerun_target PRIVATE | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-test-domain-assignment.sh` | `packages/testing/nros-tests/tests/qos.rs` += const RERUN_DOMAIN: &str = "export ROS_DOMAIN_ID=117 && ros2 topic ech | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-wait-evidence-discarded.py` · or_else-chain | `packages/testing/nros-tests/tests/qos.rs` += fn rerun_discard(p: &mut nros_tests::process::ManagedProcess) -> Strin | 0 | 1 | recorded 2026-09-28 |
| W6 | `check-zenohd-spawn-sites.sh` | `packages/testing/nros-tests/tests/qos.rs` += fn rerun_spawn() { | 0 | 1 | recorded 2026-09-28 |

## Direction

Per gate: the matcher sees the spelling the hole uses (multi-line forms, dotted keys, qualified paths, `+=`, `Err(_e)`, the `get_property` reader, …), or the check is made per item (`scripts/lib/per_item.py`) rather than per file. Each gets the negative control its hole names.

Per CLAUDE.md "Fix the CLASS": move each gate onto the class's shared helper,
add the negative control its row names, and re-run the row's mutation to show
it now fails. Phase-472's acceptance ("no confirmed hole in any class") stays
unmet until this list is empty.
