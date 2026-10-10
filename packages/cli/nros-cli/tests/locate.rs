//! `nros locate` through the real binary — phase-484 W1 (RFC-0103 D4).
//!
//! A synthetic nano-ros root (the marker, an index, one checked-out source)
//! and a tempdir store, so nothing reads the developer's real tree or store.

use std::{path::Path, process::Command};

fn root(tmp: &Path) -> std::path::PathBuf {
    let r = tmp.join("checkout");
    std::fs::create_dir_all(r.join("packages/core/nros-core")).unwrap();
    std::fs::write(r.join("packages/core/nros-core/Cargo.toml"), "").unwrap();
    std::fs::write(
        r.join("nros-sdk-index.toml"),
        "[source.zenoh-pico]\nenv = \"ZENOH_PICO_DIR\"\nversion = \"1.7.2\"\n\
         dest = \"third-party/zenoh-pico\"\n\n\
         [source.rosidl]\nversion = \"humble-1\"\nlocation = \"store\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(r.join("third-party/zenoh-pico")).unwrap();
    std::fs::write(r.join("third-party/zenoh-pico/version.txt"), "1.7.2").unwrap();
    r
}

fn locate(
    root: &Path,
    store: &Path,
    args: &[&str],
    env: &[(&str, &Path)],
) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nros"));
    cmd.arg("locate")
        .args(args)
        .arg("--index")
        .arg(root.join("nros-sdk-index.toml"))
        .current_dir(root)
        .env("NROS_HOME", store)
        .env_remove("ZENOH_PICO_DIR")
        .env_remove("NROS_STORE")
        .env_remove("NROS_SDK_STORE");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let o = cmd.output().unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn a_checkout_source_resolves_to_the_checkout_and_why_names_the_rung() {
    let tmp = tempfile::tempdir().unwrap();
    let r = root(tmp.path());
    let (ok, out, err) = locate(&r, &tmp.path().join("store"), &["zenoh-pico", "--why"], &[]);
    assert!(ok, "{err}");
    assert!(out.trim().ends_with("third-party/zenoh-pico"), "{out}");
    assert!(err.contains("$ZENOH_PICO_DIR unset"), "{err}");
    assert!(
        err.contains("checkout") && err.contains("<- chosen"),
        "{err}"
    );
}

#[test]
fn the_rows_env_variable_outranks_the_checkout() {
    let tmp = tempfile::tempdir().unwrap();
    let r = root(tmp.path());
    let vendor = tmp.path().join("vendor/zenoh-pico");
    std::fs::create_dir_all(&vendor).unwrap();
    let (ok, out, err) = locate(
        &r,
        &tmp.path().join("store"),
        &["zenoh-pico"],
        &[("ZENOH_PICO_DIR", &vendor)],
    );
    assert!(ok, "{err}");
    assert!(out.trim().ends_with("vendor/zenoh-pico"), "{out}");
}

#[test]
fn a_store_source_missing_from_the_store_fails_naming_the_setup_command() {
    let tmp = tempfile::tempdir().unwrap();
    let r = root(tmp.path());
    let (ok, _, err) = locate(&r, &tmp.path().join("store"), &["rosidl"], &[]);
    assert!(!ok, "a miss must fail, not invent a path");
    assert!(err.contains("nros setup --source rosidl"), "{err}");
}

#[test]
fn batch_cmake_output_is_one_normal_variable_per_row() {
    let tmp = tempfile::tempdir().unwrap();
    let r = root(tmp.path());
    let (ok, out, _) = locate(
        &r,
        &tmp.path().join("store"),
        &["--all", "--format", "cmake"],
        &[],
    );
    assert!(ok);
    assert!(out.contains("set(NROS_LOCATE_ZENOH_PICO \""), "{out}");
    assert!(!out.contains("CACHE"), "locations are never cached: {out}");
    assert!(
        !out.contains("NROS_LOCATE_ROSIDL"),
        "an unprovisioned row is not invented: {out}"
    );
}
