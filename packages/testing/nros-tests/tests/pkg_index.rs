//! Phase 212.O.4 — `n10_pkg_index_resolves_across_workspace`.
//!
//! Acceptance test for §212.N.10's workspace pkg-index: prove the
//! resolver consults `package.xml` files (NOT cargo-metadata) by
//! staging a workspace whose bringup pkg has NO `Cargo.toml`. A
//! cargo-metadata-only resolver could never discover `demo_bringup`;
//! the §N.10 implementation walks `package.xml` recursively from the
//! workspace root and so it does.
//!
//! ## Fixture
//!
//! `packages/testing/nros-tests/fixtures/o4_pkg_index_workspace/`
//!
//! ```text
//! Cargo.toml                    [workspace] members = node_a/b/c + demo_entry
//! src/
//!   node_a/{Cargo.toml,package.xml,src/lib.rs}
//!   node_b/{Cargo.toml,package.xml,src/lib.rs}
//!   node_c/{Cargo.toml,package.xml,src/lib.rs}
//!   demo_bringup/                ← NO Cargo.toml
//!     package.xml                (<name>demo_bringup</name>)
//!     launch/system.launch.xml   (<node pkg="node_a/b/c"/>)
//!   demo_entry/
//!     Cargo.toml                 [package.metadata.nros.entry] deploy = "native"
//!     package.xml
//!     src/main.rs                nros::main!(launch = "demo_bringup:system.launch.xml");
//! ```
//!
//! ## Surface inspected
//!
//! The `nros::main!()` proc-macro lives in
//! `packages/core/nros-macros/src/main_macro.rs`. On a `launch = ...`
//! form it calls:
//!
//! 1. `nros_build::pkg_index::detect_workspace_root(...)` —
//!    walks `NROS_WORKSPACE_ROOT` env / `.colcon_workspace` /
//!    `Cargo.toml [workspace]` / `.git/`.
//! 2. `nros_build::pkg_index::build_pkg_index(workspace_root)` —
//!    `WalkDir` recurse from root, collect every `package.xml`,
//!    parse `<name>` element (NOT `Cargo.toml [package] name`).
//! 3. `pkg_index.resolve_pkg("demo_bringup")` — pkg-name → dir.
//! 4. Per `<node pkg="..."/>` in the loaded launch.xml, resolve again
//!    + emit `::<pkg>::register(runtime)?;`.
//!
//! The §N.10 work shipped in nros-cli commit
//! `de165c8 feat(212.N.10): workspace pkg-index + $(find <pkg>) resolver`.
//!
//! ## Test outcome
//!
//! `cargo build -p demo_entry` succeeds. Since the bringup pkg has NO
//! `Cargo.toml`, success necessarily means the macro walked
//! `package.xml` to discover it. Bonus assertions:
//!
//! - `demo_bringup/Cargo.toml` is absent on disk after staging
//!   (defensive — flag a regression if a future fixture refactor
//!   accidentally adds one).
//! - The build emits node_a/b/c rlibs (cargo's
//!   `--message-format=json` artifact stream lists them).
//!
//! Run with:
//!   cargo test -p nros-tests --test phase212_o4_pkg_index

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/o4_pkg_index_workspace")
}

/// Copy the fixture into a tempdir + rewrite `@NANO_ROS_ROOT@`
/// placeholders to absolute `path =` deps. Mirrors the §N.9 sibling
/// fixture-staging helper in `phase212_n9_main_macro_forms.rs`.
fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src();
    assert!(
        src.is_dir(),
        "fixture missing: {} (expected committed at {})",
        src.display(),
        "packages/testing/nros-tests/fixtures/o4_pkg_index_workspace",
    );
    let dst = tempfile::tempdir().expect("tempdir for staged fixture");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root_str = workspace_root()
        .to_str()
        .expect("workspace root utf-8")
        .to_string();
    rewrite_placeholders(dst.path(), &root_str).expect("rewrite @NANO_ROS_ROOT@");
    let root = dst.path().to_path_buf();
    (dst, root)
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn rewrite_placeholders(root: &Path, replacement: &str) -> std::io::Result<()> {
    for entry in walk(root)? {
        if !entry.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&entry) else {
            continue;
        };
        if !text.contains("@NANO_ROS_ROOT@") {
            continue;
        }
        fs::write(&entry, text.replace("@NANO_ROS_ROOT@", replacement))?;
    }
    Ok(())
}

fn walk(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in fs::read_dir(&p)? {
                stack.push(e?.path());
            }
        } else {
            out.push(p);
        }
    }
    Ok(out)
}

/// The headline assertion: a `cargo build` against the staged
/// fixture succeeds. Since `demo_bringup` ships WITHOUT a Cargo.toml,
/// success necessarily proves the macro's pkg-index walked
/// `package.xml` to discover it (a cargo-metadata-only resolver
/// could not see the directory).
///
/// **Ignored by default** — the test depends on the installed
/// `nros-macros` crate's N.10 wiring (the macro calls
/// `nros_build::pkg_index::detect_workspace_root` / `build_pkg_index`),
/// which is keyed on the `nros-build` git pin in
/// `packages/testing/nros-tests/Cargo.toml`:
///
///   nros-build = { git = "https://github.com/NEWSLabNTU/nros-cli.git",
///                  branch = "main" }
///
/// As of this commit (2026-06-04) the upstream `main` ships the
/// `de165c8 feat(212.N.10)` commit. Drop the `#[ignore]` once
/// `just ci` is green end-to-end and the gate is wired in (per the
/// §212.O.4 doc bullet — the surface is landed, the gate-flip is
/// what this test is for).
#[test]
#[ignore = "Phase 212.O.4 acceptance: install pin must include nros-cli de165c8 (§212.N.10)"]
fn n10_pkg_index_resolves_across_workspace() {
    // Preflight: cargo must be reachable. `nros_tests::skip!` raises
    // a `[SKIPPED]` panic so the test reports skipped (not silently
    // passed) on a missing precondition — per CLAUDE.md "Tests must
    // fail on unmet preconditions" rule.
    if which("cargo").is_none() {
        nros_tests::skip!("`cargo` not found on PATH (Phase 212.O.4 needs cargo)");
    }

    let (_guard, root) = stage_fixture();

    // The whole point of the fixture: demo_bringup has NO Cargo.toml.
    // Verify before kicking the build so a regression in the fixture
    // (someone accidentally drops in a Cargo.toml later) is caught
    // with a clear message rather than masking the actual signal.
    let bringup_cargo = root.join("src/demo_bringup/Cargo.toml");
    assert!(
        !bringup_cargo.exists(),
        "fixture invariant violated: `src/demo_bringup/Cargo.toml` exists \
         at `{}`. The O.4 acceptance hinges on the bringup pkg being \
         invisible to cargo-metadata — its discovery must come from the \
         §212.N.10 `package.xml` walk only.",
        bringup_cargo.display(),
    );
    let bringup_pkg_xml = root.join("src/demo_bringup/package.xml");
    assert!(
        bringup_pkg_xml.is_file(),
        "fixture invariant violated: `src/demo_bringup/package.xml` \
         missing at `{}` — pkg-index has nothing to walk for.",
        bringup_pkg_xml.display(),
    );

    // Drive `cargo build -p demo_entry --message-format=json` so we
    // can both (a) assert success and (b) inspect the per-artifact
    // stream to confirm node_a/b/c rlibs landed (the macro emits
    // `::<pkg>::register(runtime)?;` calls and the Cargo path-deps
    // wire the rlibs in; if pkg-index had failed to resolve any
    // node pkg the macro would `compile_error!()` first).
    let manifest = root.join("Cargo.toml");
    let out = Command::new("cargo")
        .args([
            "build",
            "-p",
            "demo_entry",
            "--message-format=json",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .expect("spawn `cargo build`");
    assert!(
        out.status.success(),
        "expected `cargo build -p demo_entry` to succeed (N.10 pkg-index \
         resolved `demo_bringup` via `package.xml` walk), but it failed.\n\
         status: {:?}\n\
         stdout (json stream):\n{}\n\
         stderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Walk the JSON artifact stream — look for `compiler-artifact`
    // messages tagged with `target.name` ∈ {node_a, node_b, node_c}.
    // Each must appear at least once for the macro's per-node
    // register-call emission to have compiled.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut seen = [false; 3];
    let pkgs = ["node_a", "node_b", "node_c"];
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-artifact") {
            continue;
        }
        let Some(name) = v
            .get("target")
            .and_then(|t| t.get("name"))
            .and_then(|n| n.as_str())
        else {
            continue;
        };
        for (i, pkg) in pkgs.iter().enumerate() {
            if name == *pkg {
                seen[i] = true;
            }
        }
    }
    for (i, pkg) in pkgs.iter().enumerate() {
        assert!(
            seen[i],
            "expected cargo's `--message-format=json` stream to report a \
             `compiler-artifact` for `{pkg}` (the macro emits \
             `::{pkg}::register(runtime)?;` and the Cargo path-dep \
             pulls the rlib in). Full stdout:\n{stdout}",
        );
    }
}

/// Lightweight `which` — searches `PATH` directories for an executable.
fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
