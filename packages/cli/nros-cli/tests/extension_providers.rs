//! phase-527 W1 (RFC-0119 D1) — the provider listing finds a `runner` and a
//! `command` package through the real binary, by the same scan as every other
//! kind.
//!
//! The fixture workspace is the tracked
//! `nros-cli-core/tests/fixtures/extension_packages`, read in place (the scan
//! writes nothing). The nano-ros root is an empty tempdir, so the listing holds
//! the two fixtures and nothing from the developer's tree.
//!
//! The verb is `nros ws providers`; phase-526 moves it under `nros dev`.

use std::{path::PathBuf, process::Command};

fn fixture_ws() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nros-cli-core/tests/fixtures/extension_packages")
        .canonicalize()
        .unwrap()
}

fn providers(args: &[&str]) -> (bool, String, String) {
    let empty_root = tempfile::tempdir().unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_nros"))
        .args(["ws", "providers", "--workspace"])
        .arg(fixture_ws())
        .arg("--nano-ros-root")
        .arg(empty_root.path())
        .args(args)
        .env_remove("NROS_PACKAGE_PATH")
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn the_listing_names_the_fixture_runner_and_command() {
    let (ok, out, err) = providers(&["--lines"]);
    assert!(ok, "{err}");
    let rows: Vec<Vec<&str>> = out.lines().map(|l| l.split('\t').collect()).collect();
    let has = |kind: &str, name: &str, pkg: &str| {
        rows.iter()
            .any(|r| r.len() >= 3 && r[0] == kind && r[1] == name && r[2] == pkg)
    };
    assert!(has("runner", "fixture", "fixture_runner"), "{out}");
    assert!(has("command", "fixture-hello", "fixture_command"), "{out}");
    assert!(
        !err.contains("warning:"),
        "the fixtures must scan clean: {err}"
    );
}

#[test]
fn each_kind_filters_and_resolves() {
    // The table, not `--lines`: the cmake seam is unfiltered by design.
    let (ok, out, err) = providers(&["--kind", "runner"]);
    assert!(ok, "{err}");
    assert!(out.contains("fixture_runner"), "{out}");
    assert!(!out.contains("fixture_command"), "{out}");
    let (ok, out, err) = providers(&["--resolve", "command:fixture-hello"]);
    assert!(ok, "{err}");
    assert!(out.contains("fixture_command"), "{out}");
}
