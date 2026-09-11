//! Phase 212.O.2, inverted by phase-445 W5 — integration test for the
//! `entry-deploy-retired` lint.
//!
//! The lint used to REQUIRE `[package.metadata.nros.entry] deploy`
//! (`entry-deploy-missing`). RFC-0098 D5 moved an entry's board into a
//! `system.toml` image and W5 deleted the reader that understood the key, so
//! the fixture that was the offending shape — an entry table with no `deploy` —
//! is now the CORRECT one, and the key's presence is the defect. Both halves are
//! asserted against `nros check --workspace`, the surface downstream tooling
//! reads the diagnostic id from.

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
        std::env::temp_dir().join(format!("phase-445-w5-{tag}-{}-{stamp}", std::process::id()));
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

fn check_workspace(root: &Path) -> eyre::Result<()> {
    check::run(check::Args {
        plan: PathBuf::from("build/nros/nros-plan.json"),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: Some(root.to_path_buf()),
    })
}

fn entry_table(manifest: &Path) -> toml::Value {
    let body = fs::read_to_string(manifest).unwrap();
    // Parsed, not substring-scanned: comments in the fixture mention the key.
    let parsed: toml::Value = toml::from_str(&body).unwrap();
    parsed
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("entry"))
        .cloned()
        .expect("fixture must declare [package.metadata.nros.entry]")
}

#[test]
fn an_entry_table_without_deploy_is_accepted() {
    let root = temp_root("entry_no_deploy");
    copy_dir(&fixture_root(), &root);
    let manifest = root.join("freertos_entry_pkg/Cargo.toml");
    assert!(
        entry_table(&manifest).get("deploy").is_none(),
        "fixture must NOT declare a `deploy = …` field"
    );
    check_workspace(&root).expect("an entry table with no deploy is the RFC-0098 shape");
}

#[test]
fn a_retired_entry_deploy_key_is_rejected() {
    let root = temp_root("entry_deploy_retired");
    copy_dir(&fixture_root(), &root);
    let manifest = root.join("freertos_entry_pkg/Cargo.toml");
    let body = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        body.replace(
            "[package.metadata.nros.entry]\n",
            "[package.metadata.nros.entry]\ndeploy = \"freertos\"\n",
        ),
    )
    .unwrap();
    // The negative control is the staging itself: the key must be there.
    assert!(
        entry_table(&manifest).get("deploy").is_some(),
        "staging failed to add the key"
    );
    let msg = check_workspace(&root)
        .expect_err("a retired deploy key must be rejected")
        .to_string();
    // Diagnostic id is part of the stable lint contract.
    assert!(
        msg.contains("entry-deploy-retired"),
        "diagnostic id missing: {msg}"
    );
    assert!(
        msg.contains("freertos_entry_pkg"),
        "diagnostic must name the offending pkg: {msg}"
    );
    assert!(
        msg.contains("system.toml"),
        "diagnostic must say where the board goes: {msg}"
    );
}
