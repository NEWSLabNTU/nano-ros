//! Phase 212.M-F.13 path (b) — `nros::component!()` resolves with only
//! `nros` as a Cargo dep.
//!
//! Guards the macro re-export path: `nros::component!()` expansion
//! references `RuntimeCtx` / `RuntimeError` / the four `Component*Fn`
//! aliases via `::nros::__macro_support::nros_platform::*`. If
//! someone retargets back to bare `::nros_platform::*` (the
//! pre-M-F.13 shape that triggered the original FreeRTOS fixture
//! breakage) or removes the `__macro_support` re-export from
//! `packages/core/nros/src/lib.rs`, a downstream Component pkg that
//! only depends on `nros` fails with `error[E0433]: failed to
//! resolve: use of unresolved module or unlinked crate
//! 'nros_platform'`.
//!
//! The fixture under `fixtures/one_dep_component_pkg/` declares
//! exactly one `nros` dep — no `nros-platform`. The test stages a
//! tempdir copy with `@NANO_ROS_ROOT@` rewritten (same shape as the
//! `phase212_h3_freertos` helpers) and runs a native `cargo check`.
//!
//! Run with: `cargo test -p nros-tests --test phase212_macro_one_dep`

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/one_dep_component_pkg")
}

/// Copy the source fixture into a tempdir + rewrite `@NANO_ROS_ROOT@`
/// placeholders so the staged tree carries absolute `path =` deps.
/// Mirrors the helper in `phase212_h3_freertos.rs` (keeps the two
/// fixtures' staging logic identical — if a future change wants to
/// factor it out, both sites can share one helper).
fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src();
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root_str = workspace_root()
        .to_str()
        .expect("workspace root is utf-8")
        .to_string();
    rewrite_placeholders(dst.path(), &root_str).expect("rewrite placeholders");
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

#[test]
fn one_dep_component_pkg_compiles_without_explicit_nros_platform_dep() {
    // Pre-flight: the fixture's manifest must NOT list `nros-platform`
    // as a dep. If it does, the test's signal is meaningless — the
    // macro resolves through the direct dep instead of the
    // re-export, and a regression in the re-export wouldn't surface.
    // Read the source fixture (pre-staging) so the assertion catches
    // accidental edits that re-introduce the dep.
    let manifest_text =
        fs::read_to_string(fixture_src().join("Cargo.toml")).expect("read fixture Cargo.toml");
    // Match only a real dep line — comments referencing the crate
    // (e.g. the rationale block) are fine.
    let has_dep = manifest_text
        .lines()
        .any(|l| l.trim_start().starts_with("nros-platform"));
    assert!(
        !has_dep,
        "fixture `one_dep_component_pkg/Cargo.toml` must NOT depend on \
        `nros-platform` — the whole point is to prove the macro \
        re-export resolves without it. Found a `nros-platform = …` \
        line in the manifest:\n{manifest_text}"
    );

    let (_guard, root) = stage_fixture();

    // Run `cargo check` against the staged copy. Native host target,
    // no cross-compile — the macro's emit-token resolution doesn't
    // depend on target arch, so a host check is sufficient to flag
    // the regression.
    let out = Command::new("cargo")
        .args(["check", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("spawn cargo check");

    assert!(
        out.status.success(),
        "cargo check on `one_dep_component_pkg` failed — the \
        `nros::component!()` macro emit no longer resolves through \
        `nros::__macro_support::nros_platform`. Check that\n  \
        - `packages/core/nros/src/lib.rs` still defines `pub mod \
        __macro_support {{ pub use ::nros_platform; }}`\n  \
        - `packages/core/nros-macros/src/lib.rs` still emits \
        `::nros::__macro_support::nros_platform::*` instead of bare \
        `::nros_platform::*`.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Negative-signal guard: the FreeRTOS regression that triggered
    // M-F.13 produced `error[E0433]: failed to resolve: use of
    // unresolved module or unlinked crate `nros_platform``. If the
    // build somehow succeeded but emitted that diagnostic as a
    // warning (or future cargo flags it differently), surface it.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("unresolved module or unlinked crate `nros_platform`"),
        "cargo check completed but stderr mentions the M-F.13 \
        regression diagnostic — the macro re-export path is broken:\n{stderr}"
    );
}
