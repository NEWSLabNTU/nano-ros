//! The release underlay (RFC-0119 D4, phase-527 W6): a released toolchain's
//! prebuilt runner packages, staged by `nros toolchain stage-packages`, are
//! the nano-ros root a user's runner overlays — under the same override rules
//! as a checkout's `packages/runners` (W2b, W2c).
//!
//! The prefix is built here from the REAL `packages/runners/nros_runner_qemu`
//! announcement and descriptor (only the executable is a stand-in), so a
//! change to the shipped package is a change to what this test stages. The
//! user's packages are the tracked override fixtures' `ws/`.

use std::path::{Path, PathBuf};

use cargo_nano_ros::{
    extension_descriptor::{ExtensionResolveError, resolve_runner_with},
    provider_overrides::{OverrideError, OverridePolicy, OverrideStatus, ReportRow, report},
    provider_scan::{build_search_path, scan_search_path},
};
use nros_cli_core::orchestration::release_packages::{
    SHIPPED_PACKAGES_SUBDIR, SHIPPED_SUBDIR, stage,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn user_ws() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/extension_packages/overrides/ws")
        .canonicalize()
        .unwrap()
}

/// `<tmp>/prefix` with an SDK root holding the real qemu runner package, and
/// `<tmp>/checkout/target/release/nros-runner-qemu` standing in for its build.
/// Returns the staged prefix's SDK root.
fn released_prefix(tmp: &Path) -> PathBuf {
    let rel = "packages/runners/nros_runner_qemu";
    let real = repo_root().join(rel);
    let sdk = tmp.join("prefix").join(SHIPPED_SUBDIR);
    let checkout = tmp.join("checkout");
    for root in [&sdk, &checkout] {
        let dir = root.join(rel);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["package.xml", "nros-runner.toml", "Cargo.toml"] {
            std::fs::copy(real.join(f), dir.join(f)).unwrap();
        }
    }
    let built = checkout.join("target/release/nros-runner-qemu");
    std::fs::create_dir_all(built.parent().unwrap()).unwrap();
    std::fs::write(&built, "#!/bin/sh\n").unwrap();
    let target = checkout.join("target");
    let staged = stage(&tmp.join("prefix"), &checkout, &|_| Ok(target.clone())).unwrap();
    assert_eq!(staged.len(), 1, "{staged:?}");
    sdk
}

fn policy(declared: &[(&str, &str)]) -> OverridePolicy {
    let declared = declared
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    OverridePolicy::new(&user_ws(), &declared, &[])
}

#[test]
fn the_released_qemu_runner_resolves_from_the_prebuilt_underlay() {
    let tmp = tempfile::tempdir().unwrap();
    let sdk = released_prefix(tmp.path());
    let empty_ws = tmp.path().join("plain_ws");
    std::fs::create_dir_all(&empty_ws).unwrap();
    let scan = scan_search_path(&build_search_path(Some(&sdk), &empty_ws, &[], None)).unwrap();
    let r = resolve_runner_with(
        &scan,
        "qemu",
        &OverridePolicy::new(&empty_ws, &Default::default(), &[]),
    )
    .unwrap();
    assert_eq!(r.package, "nros_runner_qemu");
    let packages = tmp.path().join("prefix").join(SHIPPED_PACKAGES_SUBDIR);
    assert!(
        r.dir
            .starts_with(packages.canonicalize().unwrap_or(packages))
    );
    assert_eq!(
        r.bin, "nros-runner-qemu",
        "stated by the stage, as the source derives it"
    );
    assert!(r.dir.join("bin").join(&r.bin).is_file());
}

#[test]
fn an_undeclared_user_qemu_is_refused_and_a_declared_one_replaces_ours() {
    let tmp = tempfile::tempdir().unwrap();
    let sdk = released_prefix(tmp.path());
    let path = build_search_path(Some(&sdk), &user_ws(), &[], None);
    let scan = scan_search_path(&path).unwrap();

    // Undeclared: the listing (what `nros doctor` prints) names it and the
    // resolution (what `nros run` does) refuses it.
    let none = policy(&[]);
    let row = report(&scan, &none)
        .into_iter()
        .find_map(|r| match r {
            ReportRow::Override(o) if o.kind == "runner" && o.name == "qemu" => Some(o),
            _ => None,
        })
        .expect("the listing names the qemu override");
    assert_eq!(row.status, OverrideStatus::Undeclared);
    match resolve_runner_with(&scan, "qemu", &none).unwrap_err() {
        ExtensionResolveError::Override(e) => {
            assert!(matches!(*e, OverrideError::Undeclared { .. }), "{e}");
        }
        other => panic!("expected an override refusal, got: {other}"),
    }

    // Declared: it replaces the released runner, and the listing says so.
    let declared = policy(&[("qemu", "src/my_qemu")]);
    let r = resolve_runner_with(&scan, "qemu", &declared).expect("the declared runner wins");
    assert_eq!(r.package, "my_qemu");
    let note = r.overrides.expect("the override is recorded");
    assert_eq!(note.status, OverrideStatus::Declared);
    assert_eq!(note.shadowed_package, "nros_runner_qemu");
    assert!(
        note.shadowed_dir
            .to_string_lossy()
            .contains(SHIPPED_PACKAGES_SUBDIR),
        "the package replaced is the prebuilt one: {}",
        note.shadowed_dir.display()
    );
    let row = report(&scan, &declared)
        .into_iter()
        .find_map(|r| match r {
            ReportRow::Override(o) if o.kind == "runner" && o.name == "qemu" => Some(o),
            _ => None,
        })
        .expect("the listing still names it");
    assert_eq!(row.status, OverrideStatus::Declared);
}
