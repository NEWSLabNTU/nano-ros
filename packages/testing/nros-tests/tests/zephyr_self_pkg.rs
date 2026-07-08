//! §212.M-F.3 — Zephyr H.1 adapter shim accepts self-pkg bringup —
//! build-stage fixture (issue 0041).
//!
//! The M-F.2 nros-cli planner accepts an L.7 self-pkg bringup: a single
//! Node / Application Cargo pkg carrying `[package.metadata.nros.component]`
//! plus `[package.metadata.nros.deploy.zephyr]` (no sibling Path A
//! `system.toml`). The M-F.3 shim teaches
//! `zephyr/cmake/nros_system_generate.cmake::_nros_system_resolve_bringup()`
//! to accept that shape, invoked via either the self-pkg form
//! (`nros_system_generate(.)`) or a sibling-dir name
//! (`nros_system_generate(alpha_pkg)`).
//!
//! ## issue-0041 conversion
//!
//! The shim's bringup app + `west build` used to be generated + run IN the
//! test (`fs::write` of Cargo.toml/lib.rs/prj.conf/CMakeLists, then
//! `west build`). Per "No compilation inside tests" the two app layouts are
//! now committed fixture templates under
//! `fixtures/zephyr_self_pkg/{self,sibling}/`, and `west-fixtures.sh` runs
//! the build in the build stage. The shim contract is the configure-time
//! BAKE — `nros-system/system_{config.h,main.c}` — NOT a full ELF link (which
//! needs the rest of the runtime; out of scope, as in the original). So the
//! build-stage stamps on BAKE-EXISTS, and this test INSPECTS the prebuilt
//! bake. No west at run time.
//!
//! Tier-aware: `require_west_fixture` skips/deselects when west / a
//! provisioned Zephyr workspace was absent at build time (no bake artifact).

use std::path::Path;

/// The M-F.3 shim bake landed: `system_config.h` (config bake) and
/// `system_config.cmake` (cmake mirror) under `<build>/nros-system/`.
/// (`system_main.c` retired in phase-258 — components register through the
/// install seam, no generated TU; issue 0154.)
fn assert_bake(build_dir: &Path) {
    let baked = build_dir.join("nros-system");
    let config_h = baked.join("system_config.h");
    let config_cmake = baked.join("system_config.cmake");
    assert!(
        config_h.is_file() && config_cmake.is_file(),
        "M-F.3 shim bake missing under {} (config_h={}, config_cmake={}) — shim regressed?",
        baked.display(),
        config_h.is_file(),
        config_cmake.is_file(),
    );
}

/// 1. Self-pkg form `nros_system_generate(.)` — the app pkg IS the self-pkg.
///    The build-stage `west build` of `zephyr_self_pkg/self/alpha_pkg` baked
///    the system; assert the bake landed.
#[test]
fn zephyr_self_pkg_rust_builds_via_shim() -> nros_tests::TestResult<()> {
    let config_h = nros_tests::fixtures::require_west_fixture(
        "zephyr_self_pkg_rust",
        "nros-system/system_config.h",
    )?;
    let build_dir = config_h
        .parent()
        .and_then(|p| p.parent())
        .expect("self-pkg build dir");
    assert_bake(build_dir);
    Ok(())
}

/// 2. Sibling form — `caller/`'s `nros_system_generate(alpha_pkg)` resolves the
///    sibling self-pkg dir (no `system.toml`), still hitting the M-F.3 self-pkg
///    branch. The build-stage `west build` of `zephyr_self_pkg/sibling/caller`
///    baked the system from the sibling `alpha_pkg`; assert the bake landed.
#[test]
fn zephyr_self_pkg_resolve_bringup_handles_relative_path() -> nros_tests::TestResult<()> {
    let config_h = nros_tests::fixtures::require_west_fixture(
        "zephyr_self_pkg_sibling",
        "nros-system/system_config.h",
    )?;
    let build_dir = config_h
        .parent()
        .and_then(|p| p.parent())
        .expect("sibling build dir");
    assert_bake(build_dir);
    Ok(())
}
