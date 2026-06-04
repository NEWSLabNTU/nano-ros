//! Phase 212.O.6 — integration test for the
//! `application-rtos-deploy-forbidden` lint.
//!
//! Stages a fixture workspace whose sole pkg declares
//! `[package.metadata.nros.application]` with `deploy = ["native",
//! "freertos"]`, runs `nros check --workspace <root>`, and asserts the
//! diagnostic id `application-rtos-deploy-forbidden` surfaces in the error
//! message (Application pkgs are native-only per Phase 212.L.2 / M-F.1).

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::cmd::check;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/check/application_with_rtos_deploy")
}

fn temp_root(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("phase-212-o6-{tag}-{}-{stamp}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap().flatten() {
        let path = entry.path();
        let target = dst.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}

/// Verifies workspace checks reject an application package with RTOS deploy entries.
#[test]
fn check_workspace_rejects_rtos_deploy() {
    let root = temp_root("application_with_rtos_deploy");
    copy_dir(&fixture_root(), &root);
    let cargo_toml = root.join("demo_app/Cargo.toml");
    assert!(
        cargo_toml.is_file(),
        "fixture missing: {}",
        cargo_toml.display()
    );
    let body = fs::read_to_string(&cargo_toml).unwrap();
    assert!(
        body.contains("[package.metadata.nros.application]"),
        "fixture should declare the application table"
    );
    assert!(
        body.contains("freertos"),
        "fixture must include an RTOS target in the deploy list"
    );

    let err = check::run(check::Args {
        plan: PathBuf::from("build/nros/nros-plan.json"),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: Some(root.clone()),
    })
    .expect_err("application pkg with RTOS deploy must be rejected");
    let msg = err.to_string();
    // Diagnostic id is part of the stable lint contract.
    assert!(
        msg.contains("application-rtos-deploy-forbidden"),
        "diagnostic id missing: {msg}"
    );
    assert!(
        msg.contains("demo_app"),
        "diagnostic must name the offending pkg: {msg}"
    );
    assert!(
        msg.contains("freertos"),
        "diagnostic must name the offending RTOS target: {msg}"
    );
}
