//! §212.N.10 pkg-index walk across a workspace — build-stage fixture (issue 0041).
//!
//! The Entry-pkg's `nros::main!()` macro resolves the bringup + node pkgs through
//! `nros_build::pkg_index::build_pkg_index`, which consults `package.xml` files
//! (NOT cargo-metadata). The fixture's `demo_bringup` ships **without** a
//! `Cargo.toml`, so a cargo-metadata-only resolver could never discover it —
//! a successful build of `demo_entry` necessarily proves the `package.xml` walk
//! ran (the macro `compile_error!()`s if it can't resolve a node pkg).
//!
//! Per issue 0041 ("No compilation inside tests") this compile runs in the
//! **build stage**: the `o4_pkg_index` build-fixture (`compile-check-fixtures.sh`,
//! run by `build-test-fixtures`) stages the workspace + `cargo build -p
//! demo_entry`. This test INSPECTS the prebuilt result — the build's success
//! (`.compile-ok` stamp) IS the headline assertion; the per-node `node_{a,b,c}`
//! rlibs in `target/debug/deps` confirm the macro emitted + compiled each node's
//! `register()` call. No cargo at run time. Fixture absence → tier-aware
//! skip/fail via the resolver.

use std::path::PathBuf;

fn fixture_src() -> PathBuf {
    nros_tests::project_root().join("packages/testing/nros-tests/fixtures/o4_pkg_index_workspace")
}

#[test]
fn n10_pkg_index_resolves_across_workspace() -> nros_tests::TestResult<()> {
    // Fixture invariant: `demo_bringup` is invisible to cargo-metadata (no
    // Cargo.toml) yet has a `package.xml` — so its discovery can only come from
    // the §212.N.10 pkg-index walk. Guards against a regression that drops a
    // Cargo.toml into the fixture (which would mask the real signal).
    let src = fixture_src();
    assert!(
        src.is_dir(),
        "fixture missing: {} — run `just build-test-fixtures`",
        src.display()
    );
    assert!(
        !src.join("src/demo_bringup/Cargo.toml").exists(),
        "fixture invariant violated: src/demo_bringup/Cargo.toml exists — the O.4 \
         acceptance hinges on the bringup pkg being invisible to cargo-metadata"
    );
    assert!(
        src.join("src/demo_bringup/package.xml").is_file(),
        "fixture invariant violated: src/demo_bringup/package.xml missing — \
         pkg-index has nothing to walk for"
    );

    // The build-stage `cargo build -p demo_entry` succeeded (the `.compile-ok`
    // stamp). A failed pkg-index resolve would `compile_error!()` first.
    let stamp = nros_tests::fixtures::require_compile_check("o4_pkg_index")?;
    let deps = stamp.parent().expect("stamp dir").join("target/debug/deps");

    // Each node pkg's rlib must be present — the macro emits a per-node
    // `::<pkg>::register(runtime)?;` call wired via the pkg-index path-deps.
    for pkg in ["node_a", "node_b", "node_c"] {
        let prefix = format!("lib{pkg}-");
        let found = std::fs::read_dir(&deps)
            .map(|rd| {
                rd.filter_map(|e| e.ok()).any(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with(&prefix) && name.ends_with(".rlib")
                })
            })
            .unwrap_or(false);
        assert!(
            found,
            "node pkg `{pkg}` rlib missing in {} — pkg-index did not resolve it \
             (the macro's per-node register call would not have compiled)",
            deps.display()
        );
    }
    Ok(())
}
