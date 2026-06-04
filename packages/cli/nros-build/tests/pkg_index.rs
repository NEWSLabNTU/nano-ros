//! Phase 212.N.10 — pkg-index + workspace-root + `$(find …)` tests.

use std::{
    fs,
    path::Path,
    sync::{Mutex, MutexGuard, OnceLock, PoisonError},
};

use nros_build::pkg_index::{build_pkg_index, detect_workspace_root};

/// Serialize tests that touch the process-wide `NROS_WORKSPACE_ROOT`
/// env var so a setter in one test can't bleed into another. Every
/// test in this file acquires the guard for the duration of the
/// closure body it runs under.
fn env_guard() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn write_package_xml(dir: &Path, name: &str) {
    fs::create_dir_all(dir).expect("mkdir pkg dir");
    let xml = format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.0.1</version>
  <description>{name} test fixture</description>
  <maintainer email="test@example.com">test</maintainer>
  <license>MIT</license>
</package>
"#
    );
    fs::write(dir.join("package.xml"), xml).expect("write package.xml");
}

/// Wipe any `NROS_WORKSPACE_ROOT` env override so an outer-test value
/// doesn't bleed into the test under inspection.
fn clear_env_override() {
    // SAFETY: tests run single-threaded under `cargo test` for this file
    // (no rstest matrix), and we own the process's env.
    unsafe {
        std::env::remove_var("NROS_WORKSPACE_ROOT");
    }
}

#[test]
fn resolves_pkg_by_name() {
    let _env = env_guard();
    clear_env_override();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    // Mark as workspace root so we can detect it deterministically.
    fs::write(root.join(".colcon_workspace"), "").unwrap();

    write_package_xml(&root.join("alpha_pkg"), "alpha_pkg");
    write_package_xml(&root.join("beta_pkg"), "beta_pkg");
    write_package_xml(&root.join("gamma/gamma_pkg"), "gamma_pkg");

    let index = build_pkg_index(root).expect("build pkg-index");
    let alpha = index.resolve_pkg("alpha_pkg").expect("alpha resolves");
    let beta = index.resolve_pkg("beta_pkg").expect("beta resolves");
    let gamma = index.resolve_pkg("gamma_pkg").expect("gamma resolves");

    assert!(alpha.ends_with("alpha_pkg"), "alpha dir: {alpha:?}");
    assert!(beta.ends_with("beta_pkg"), "beta dir: {beta:?}");
    assert!(
        gamma.ends_with("gamma/gamma_pkg") || gamma.ends_with("gamma_pkg"),
        "gamma dir: {gamma:?}"
    );

    let mut names: Vec<&str> = index.pkgs().map(|(n, _)| n).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["alpha_pkg", "beta_pkg", "gamma_pkg"]);
}

#[test]
fn detects_workspace_root_via_marker() {
    let _env = env_guard();
    clear_env_override();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::write(root.join(".colcon_workspace"), "").unwrap();
    let nested = root.join("a/b/c");
    fs::create_dir_all(&nested).unwrap();

    let detected = detect_workspace_root(&nested).expect("detect");
    assert_eq!(
        detected.canonicalize().unwrap(),
        root.canonicalize().unwrap()
    );
}

#[test]
fn detects_workspace_root_via_cargo_workspace() {
    let _env = env_guard();
    clear_env_override();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = []
"#,
    )
    .unwrap();
    let nested = root.join("crates/x");
    fs::create_dir_all(&nested).unwrap();

    let detected = detect_workspace_root(&nested).expect("detect");
    assert_eq!(
        detected.canonicalize().unwrap(),
        root.canonicalize().unwrap()
    );
}

#[test]
fn detects_workspace_root_via_env_override() {
    let _env = env_guard();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::create_dir_all(root.join("nested/inner")).unwrap();
    // No marker, no Cargo.toml, no .git — env override is the only
    // resolver path that succeeds.
    // SAFETY: this test owns the env var for its duration.
    unsafe {
        std::env::set_var("NROS_WORKSPACE_ROOT", root);
    }
    let detected = detect_workspace_root(&root.join("nested/inner")).expect("detect");
    assert_eq!(
        detected.canonicalize().unwrap(),
        root.canonicalize().unwrap()
    );
    // SAFETY: clear after to prevent bleed.
    unsafe {
        std::env::remove_var("NROS_WORKSPACE_ROOT");
    }
}

#[test]
fn duplicate_pkg_names_error() {
    let _env = env_guard();
    clear_env_override();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::write(root.join(".colcon_workspace"), "").unwrap();
    write_package_xml(&root.join("dir_a"), "samename");
    write_package_xml(&root.join("dir_b"), "samename");

    let err = build_pkg_index(root).expect_err("duplicate must error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("duplicate pkg name") && msg.contains("samename"),
        "diagnostic: {msg}"
    );
}

#[test]
fn resolve_find_substitution_basic() {
    let _env = env_guard();
    clear_env_override();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::write(root.join(".colcon_workspace"), "").unwrap();
    write_package_xml(&root.join("demo_bringup"), "demo_bringup");

    let index = build_pkg_index(root).expect("build");
    let resolved = index
        .resolve_find_substitution("$(find demo_bringup)/launch/x.xml")
        .expect("resolve");
    let expected = root
        .canonicalize()
        .unwrap()
        .join("demo_bringup/launch/x.xml");
    assert_eq!(resolved, expected.to_string_lossy());

    // No trailing path → just the pkg dir.
    let resolved = index
        .resolve_find_substitution("$(find demo_bringup)")
        .expect("resolve bare");
    let expected = root.canonicalize().unwrap().join("demo_bringup");
    assert_eq!(resolved, expected.to_string_lossy());
}

#[test]
fn resolve_find_substitution_unknown_pkg_errors() {
    let _env = env_guard();
    clear_env_override();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::write(root.join(".colcon_workspace"), "").unwrap();
    write_package_xml(&root.join("real_pkg"), "real_pkg");

    let index = build_pkg_index(root).expect("build");
    let err = index
        .resolve_find_substitution("$(find nonexistent)/foo")
        .expect_err("unknown pkg must error");
    let msg = format!("{err:#}");
    assert!(msg.contains("nonexistent"), "diagnostic: {msg}");
    assert!(
        msg.contains("real_pkg"),
        "diagnostic should list known pkgs: {msg}"
    );
}

#[test]
fn skips_colcon_ignore_dirs() {
    let _env = env_guard();
    clear_env_override();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::write(root.join(".colcon_workspace"), "").unwrap();
    write_package_xml(&root.join("kept_pkg"), "kept_pkg");
    write_package_xml(&root.join("hidden/hidden_pkg"), "hidden_pkg");
    fs::write(root.join("hidden/COLCON_IGNORE"), "").unwrap();

    let index = build_pkg_index(root).expect("build");
    assert!(index.resolve_pkg("kept_pkg").is_ok());
    assert!(index.resolve_pkg("hidden_pkg").is_err());
}
