//! `nros::node!()` resolves with only `nros` as a Cargo dep (no explicit
//! `nros-platform`).
//!
//! Guards the macro re-export path: `nros::node!()` expansion references
//! `RuntimeCtx` / `RuntimeError` / the `Node*Fn` aliases via
//! `::nros::__macro_support::nros_platform::*`. If someone retargets back to
//! bare `::nros_platform::*` (the pre-M-F.13 shape that broke the FreeRTOS
//! fixture) or drops the `__macro_support` re-export from
//! `packages/core/nros/src/lib.rs`, a downstream Node pkg that depends only on
//! `nros` fails with `error[E0433]: ... unlinked crate 'nros_platform'`.
//!
//! The compile proof lives in the **build stage**: the compile-check fixture
//! `one_dep_component_pkg` (`scripts/build/compile-check-fixtures.sh`, run by
//! `build-test-fixtures`) stages the one-dep template, rewrites its
//! `@NANO_ROS_ROOT@` placeholders, runs `cargo check`, and writes a
//! `.compile-ok` stamp on success. This test asserts the stamp rather than
//! running `cargo check` at run time (issue 0034 / AGENTS.md "No compilation
//! inside tests"). The static pre-flight below keeps the signal meaningful.

use std::{fs, path::PathBuf};

fn fixture_src() -> PathBuf {
    nros_tests::project_root().join("packages/testing/nros-tests/fixtures/one_dep_component_pkg")
}

#[test]
fn one_dep_pkg_compiles_implicit_platform() -> nros_tests::TestResult<()> {
    // Pre-flight (static, not a compile): the fixture manifest must NOT list
    // `nros-platform` as a dep — else the macro resolves through the direct dep
    // instead of the re-export, and a re-export regression wouldn't surface.
    let manifest_text =
        fs::read_to_string(fixture_src().join("Cargo.toml")).expect("read fixture Cargo.toml");
    let has_dep = manifest_text
        .lines()
        .any(|l| l.trim_start().starts_with("nros-platform"));
    assert!(
        !has_dep,
        "fixture `one_dep_component_pkg/Cargo.toml` must NOT depend on \
        `nros-platform` — the whole point is to prove the macro re-export \
        resolves without it. Found a `nros-platform = …` line:\n{manifest_text}"
    );

    // Compile proof: the build-stage `cargo check` stamped `.compile-ok`. If the
    // `nros::__macro_support::nros_platform` re-export broke, the build-stage
    // check fails and no stamp exists — surfacing here as a fixture-not-built
    // failure (full tier → run `build-test-fixtures`).
    let stamp = nros_tests::fixtures::require_compile_check("one_dep_component_pkg")?;
    assert!(
        stamp.exists(),
        "compile-check stamp missing: {}",
        stamp.display()
    );
    Ok(())
}
