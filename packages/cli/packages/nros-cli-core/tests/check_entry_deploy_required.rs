//! Phase 212.O.2 — integration test for the `entry-deploy-missing` lint.
//!
//! Stages a fixture workspace whose sole pkg declares
//! `[package.metadata.nros.entry]` with no `deploy` field, runs
//! `nros check --workspace <root>`, and asserts the diagnostic id
//! `entry-deploy-missing` surfaces in the error message.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::cmd::check;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/check/entry_no_deploy")
}

fn temp_root(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("phase-212-o2-{tag}-{}-{stamp}", std::process::id()));
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

#[test]
fn nros_check_workspace_rejects_entry_pkg_without_deploy_field() {
    let root = temp_root("entry_no_deploy");
    copy_dir(&fixture_root(), &root);
    // Sanity-check the fixture shape so a stray edit surfaces here, not
    // somewhere deep inside the lint.
    let cargo_toml = root.join("freertos_entry_pkg/Cargo.toml");
    assert!(
        cargo_toml.is_file(),
        "fixture missing: {}",
        cargo_toml.display()
    );
    let body = fs::read_to_string(&cargo_toml).unwrap();
    assert!(
        body.contains("[package.metadata.nros.entry]"),
        "fixture should declare the entry table"
    );
    // Parse the manifest and confirm the `[package.metadata.nros.entry]`
    // table really does NOT carry a `deploy` field. (Substring scans get
    // fooled by comments in the fixture body.)
    let parsed: toml::Value = toml::from_str(&body).unwrap();
    let entry = parsed
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("entry"))
        .expect("fixture must declare [package.metadata.nros.entry]");
    assert!(
        entry.get("deploy").is_none(),
        "fixture must NOT declare a `deploy = …` field"
    );

    let err = check::run(check::Args {
        plan: PathBuf::from("build/nros/nros-plan.json"),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: Some(root.clone()),
    })
    .expect_err("entry pkg without deploy must be rejected");
    let msg = err.to_string();
    // Diagnostic id is part of the stable lint contract.
    assert!(
        msg.contains("entry-deploy-missing"),
        "diagnostic id missing: {msg}"
    );
    assert!(
        msg.contains("freertos_entry_pkg"),
        "diagnostic must name the offending pkg: {msg}"
    );
    assert!(
        msg.contains("'deploy'") || msg.contains("deploy ="),
        "diagnostic must mention the deploy field: {msg}"
    );
}
