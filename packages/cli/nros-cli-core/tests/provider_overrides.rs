//! Declared overrides of provider packages (RFC-0119 D4, phase-527 W2b and
//! W2c): one test per rule.
//!
//! The fixture trees are tracked under
//! `tests/fixtures/extension_packages/overrides/` and read in place; nothing
//! here builds anything. That directory carries a `COLCON_IGNORE`, so the
//! nano-ros tree's own scan (and the `extension_packages` workspace other
//! tests list) never sees these providers. Each test names its roots
//! explicitly, and a root itself is scanned whatever markers sit above it:
//!
//! - `underlay/` plays the nano-ros root: runners `qemu` and `legacy`
//!   (protocol 1), command `fixture-greet`, serdes `acme`;
//! - `ws/` is the workspace: `src/my_qemu`, `src/my_legacy` (protocol 2),
//!   `src/my_greet`, `src/my_acme` replace them, and `src/solo_runner`
//!   provides a runner nothing else does;
//! - `late/` is a later root whose `late_qemu` claims `qemu` and whose
//!   `late_acme` claims serdes `acme`. Tests scan it either as a
//!   `package_paths` entry (a SOURCE root, peer of the workspace) or as a
//!   `NROS_PACKAGE_PATH` entry (the overlay), which is the difference W2c's
//!   rules turn on.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use cargo_nano_ros::{
    extension_descriptor::{ExtensionResolveError, resolve_command_with, resolve_runner_with},
    provider_overrides::{
        OverrideError, OverridePolicy, OverrideStatus, ReportRow, report, resolve_provider,
    },
    provider_scan::{RootOrigin, ScanResult, scan_roots},
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/extension_packages/overrides")
        .canonicalize()
        .expect("the override fixtures exist")
}

fn ws() -> PathBuf {
    fixtures().join("ws")
}

/// Scan the named fixture roots with explicit origins, as
/// `provider_scan::scan_search_path` records them.
fn scan_as(roots: &[(&str, RootOrigin)]) -> ScanResult {
    let paths: Vec<PathBuf> = roots.iter().map(|(r, _)| fixtures().join(r)).collect();
    let mut s = scan_roots(&paths).expect("the fixtures scan");
    assert!(s.errors.is_empty(), "{:?}", s.errors);
    s.root_origins = roots.iter().map(|(_, o)| *o).collect();
    s
}

/// The usual tiers: `underlay` is the nano-ros tree, `ws` the workspace,
/// `late` a `package_paths` entry.
fn scan(roots: &[&str]) -> ScanResult {
    let roots: Vec<(&str, RootOrigin)> = roots
        .iter()
        .map(|r| {
            let origin = match *r {
                "underlay" => RootOrigin::NanoRosTree,
                "ws" => RootOrigin::Workspace,
                _ => RootOrigin::PackagePaths,
            };
            (*r, origin)
        })
        .collect();
    scan_as(&roots)
}

fn policy(declared: &[(&str, &str)], allowed: &[&str]) -> OverridePolicy {
    let declared: BTreeMap<String, String> = declared
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    let allowed: Vec<String> = allowed.iter().map(|s| (*s).to_string()).collect();
    OverridePolicy::new(&ws(), &declared, &allowed)
}

fn override_error(e: ExtensionResolveError) -> OverrideError {
    match e {
        ExtensionResolveError::Override(o) => *o,
        other => panic!("expected an override refusal, got: {other}"),
    }
}

#[test]
fn an_undeclared_runner_is_refused_when_resolved_while_a_scan_is_silent() {
    let s = scan(&["underlay", "ws"]);
    let none = policy(&[], &[]);

    // The listing reports it and refuses nothing.
    let rows = report(&s, &none);
    let row = rows
        .iter()
        .find_map(|r| match r {
            ReportRow::Override(o) if o.kind == "runner" && o.name == "qemu" => Some(o),
            _ => None,
        })
        .expect("the listing names the qemu override");
    assert_eq!(row.status, OverrideStatus::Undeclared);

    // A resolution refuses it, with the paste-able fix.
    let e = override_error(resolve_runner_with(&s, "qemu", &none).unwrap_err());
    let msg = e.to_string();
    assert!(matches!(e, OverrideError::Undeclared { .. }), "{msg}");
    assert!(
        msg.starts_with("src/my_qemu announces runner `qemu`, which nros_runner_qemu"),
        "{msg}"
    );
    assert!(
        msg.contains("nros.toml:   [workspace.overrides] qemu = \"src/my_qemu\""),
        "{msg}"
    );
    assert!(msg.contains("--allow-overriding qemu"), "{msg}");

    // A command follows the same rule.
    let e = resolve_command_with(&s, "fixture-greet", &none).unwrap_err();
    assert!(
        matches!(override_error(e), OverrideError::Undeclared { .. }),
        "a command override is refused like a runner's"
    );
}

#[test]
fn a_declared_path_wins_quietly() {
    let s = scan(&["underlay", "ws"]);
    let p = policy(&[("qemu", "src/my_qemu")], &[]);
    let r = resolve_runner_with(&s, "qemu", &p).expect("the declared runner resolves");
    assert_eq!(r.package, "my_qemu");
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    let note = r.overrides.expect("the override is recorded");
    assert_eq!(note.status, OverrideStatus::Declared);
    assert_eq!(note.shadowed_package, "nros_runner_qemu");
}

#[test]
fn a_declaration_pinned_to_another_path_does_not_approve() {
    // `late/` is the environment overlay, so its `late_qemu` wins over the
    // workspace's `my_qemu`. The declaration names `my_qemu`; the approval
    // does not pass to the package that actually won.
    let s = scan_as(&[
        ("underlay", RootOrigin::NanoRosTree),
        ("ws", RootOrigin::Workspace),
        ("late", RootOrigin::Env),
    ]);
    let p = policy(&[("qemu", "src/my_qemu")], &[]);
    let e = override_error(resolve_runner_with(&s, "qemu", &p).unwrap_err());
    match &e {
        OverrideError::Undeclared { shown, .. } => assert!(shown.ends_with("late_qemu"), "{e}"),
        other => panic!("expected Undeclared, got {other}"),
    }
}

#[test]
fn allow_overriding_approves_for_one_invocation() {
    let s = scan(&["underlay", "ws"]);
    let p = policy(&[], &["qemu"]);
    let r = resolve_runner_with(&s, "qemu", &p).expect("the flag approves the override");
    assert_eq!(r.package, "my_qemu");
    assert_eq!(
        r.overrides.map(|n| n.status),
        Some(OverrideStatus::AllowedOnce)
    );
    // By name only, as colcon's flag: another name is still refused.
    assert!(resolve_command_with(&s, "fixture-greet", &p).is_err());
}

#[test]
fn a_non_host_kind_warns_once_instead_of_refusing() {
    let s = scan(&["underlay", "ws"]);
    let r = resolve_provider(&s, "serdes", "acme", &policy(&[], &[]))
        .expect("an undeclared serdes override resolves");
    assert!(r.resolution.winner.dir.ends_with("ws/src/my_acme"));
    assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
    assert!(
        r.warnings[0].contains("[workspace.overrides] acme = \"src/my_acme\""),
        "{:?}",
        r.warnings
    );
    // Declared, it is quiet.
    let r = resolve_provider(
        &s,
        "serdes",
        "acme",
        &policy(&[("acme", "src/my_acme")], &[]),
    )
    .unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn a_protocol_mismatch_is_refused_even_when_declared() {
    let s = scan(&["underlay", "ws"]);
    let p = policy(&[("legacy", "src/my_legacy")], &[]);
    let e = override_error(resolve_runner_with(&s, "legacy", &p).unwrap_err());
    let msg = e.to_string();
    assert!(matches!(e, OverrideError::Protocol(_)), "{msg}");
    assert!(msg.contains("speaks protocol 2"), "{msg}");
    assert!(msg.contains("speaks 1"), "{msg}");
}

#[test]
fn a_stale_declaration_is_an_error_not_a_fallback() {
    let s = scan(&["underlay", "ws"]);
    // Nothing at the path.
    let e = override_error(
        resolve_runner_with(&s, "qemu", &policy(&[("qemu", "src/gone")], &[])).unwrap_err(),
    );
    assert!(matches!(e, OverrideError::Stale { .. }), "{e}");
    assert!(e.to_string().contains("no package at"), "{e}");
    // A package at the path that announces something else.
    let e = override_error(
        resolve_runner_with(&s, "qemu", &policy(&[("qemu", "src/my_acme")], &[])).unwrap_err(),
    );
    assert!(e.to_string().contains("serdes `acme`"), "{e}");
    // The listing reports it as stale too.
    assert!(
        report(&s, &policy(&[("qemu", "src/gone")], &[]))
            .iter()
            .any(|r| matches!(r, ReportRow::Stale(_)))
    );
}

#[test]
fn a_declaration_that_overrides_nothing_warns() {
    let s = scan(&["underlay", "ws"]);
    let p = policy(&[("solo", "src/solo_runner")], &[]);
    let r = resolve_runner_with(&s, "solo", &p).expect("the runner still resolves");
    assert_eq!(r.overrides, None);
    assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
    assert!(
        r.warnings[0].contains("overrides nothing"),
        "{:?}",
        r.warnings
    );
    assert!(
        report(&s, &p)
            .iter()
            .any(|r| matches!(r, ReportRow::Unused { name, .. } if name == "solo"))
    );
}

#[test]
fn the_build_edge_follows_the_declaration() {
    use nros_cli_core::cmd::build::runner_edges;

    let s = scan(&["underlay", "ws"]);
    let wanted: BTreeMap<String, BTreeSet<String>> =
        BTreeMap::from([("demo_bringup".into(), BTreeSet::from(["qemu".into()]))]);
    let by_dir: BTreeMap<PathBuf, String> =
        BTreeMap::from([(ws().join("src/my_qemu"), "my_qemu".into())]);

    // Undeclared: resolving the image's runner refuses the build.
    let e = runner_edges(&s, &wanted, &by_dir, &policy(&[], &[])).unwrap_err();
    assert!(e.to_string().contains("[workspace.overrides] qemu"), "{e}");

    // Declared: the bringup depends on the workspace runner.
    let edges = runner_edges(
        &s,
        &wanted,
        &by_dir,
        &policy(&[("qemu", "src/my_qemu")], &[]),
    )
    .expect("the declared override adds its edge");
    assert_eq!(
        edges.get("demo_bringup"),
        Some(&BTreeSet::from(["my_qemu".to_string()]))
    );

    // `--allow-overriding` does the same for one build.
    let edges = runner_edges(&s, &wanted, &by_dir, &policy(&[], &["qemu"])).unwrap();
    assert!(edges.contains_key("demo_bringup"));
}

// ---------------------------------------------------------------------------
// Source roots are peers (W2c): colcon's duplicate rule.
// ---------------------------------------------------------------------------

#[test]
fn the_same_provider_in_the_workspace_and_a_package_path_is_refused() {
    let s = scan(&["underlay", "ws", "late"]);
    let e = override_error(resolve_runner_with(&s, "qemu", &policy(&[], &[])).unwrap_err());
    let msg = e.to_string();
    assert!(matches!(e, OverrideError::SourceDuplicate { .. }), "{msg}");
    // Both paths are named, with where each root came from.
    assert!(msg.contains("late_qemu"), "{msg}");
    assert!(msg.contains("src/my_qemu  (workspace)"), "{msg}");
    assert!(
        msg.contains("(nros.toml [workspace] package_paths)"),
        "{msg}"
    );
    assert!(msg.contains("[workspace.overrides] qemu = \""), "{msg}");
    assert!(msg.contains("--allow-overriding qemu"), "{msg}");

    // Every kind, not only the host kinds: a serdes duplicate is refused too.
    let e = resolve_provider(&s, "serdes", "acme", &policy(&[], &[])).unwrap_err();
    assert!(e.to_string().contains("serdes `acme`"), "{e}");

    // The listing refuses nothing, and reports it as a problem.
    let rows = report(&s, &policy(&[], &[]));
    assert!(
        rows.iter()
            .any(|r| matches!(r, ReportRow::Duplicate(_)) && r.is_problem()),
        "{rows:?}"
    );
}

#[test]
fn a_pin_settles_a_source_duplicate_wherever_the_pinned_package_sits() {
    // `my_qemu` sits in the EARLIER root; the pin still picks it.
    let s = scan(&["underlay", "ws", "late"]);
    let r = resolve_runner_with(&s, "qemu", &policy(&[("qemu", "src/my_qemu")], &[]))
        .expect("the pinned runner resolves");
    assert_eq!(r.package, "my_qemu");
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(
        r.overrides.map(|n| n.status),
        Some(OverrideStatus::Declared)
    );
}

#[test]
fn allow_overriding_settles_a_source_duplicate_for_the_later_root() {
    let s = scan(&["underlay", "ws", "late"]);
    let r = resolve_runner_with(&s, "qemu", &policy(&[], &["qemu"]))
        .expect("the flag settles the duplicate");
    assert_eq!(r.package, "late_qemu");
    assert_eq!(
        r.overrides.map(|n| n.status),
        Some(OverrideStatus::AllowedOnce)
    );
}

#[test]
fn two_package_paths_entries_providing_one_name_are_refused() {
    let s = scan_as(&[
        ("underlay", RootOrigin::NanoRosTree),
        ("late", RootOrigin::PackagePaths),
        ("ws", RootOrigin::PackagePaths),
    ]);
    let e = resolve_provider(&s, "serdes", "acme", &policy(&[], &[])).unwrap_err();
    let msg = e.to_string();
    assert!(msg.contains("more than one source tree"), "{msg}");
    assert!(msg.contains("src/my_acme"), "{msg}");
    assert!(msg.contains("late_acme"), "{msg}");
}

#[test]
fn the_environment_overlay_shadows_with_a_warning() {
    let s = scan_as(&[
        ("underlay", RootOrigin::NanoRosTree),
        ("ws", RootOrigin::Workspace),
        ("late", RootOrigin::Env),
    ]);
    // A non-host kind: the overlay wins and says so.
    let r = resolve_provider(&s, "serdes", "acme", &policy(&[], &[]))
        .expect("the overlay may shadow a source root");
    assert!(r.resolution.winner.dir.ends_with("late/late_acme"));
    assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
    // A runner still needs its declaration, overlay or not.
    let e = override_error(resolve_runner_with(&s, "qemu", &policy(&[], &[])).unwrap_err());
    assert!(matches!(e, OverrideError::Undeclared { .. }), "{e}");
}

#[test]
fn the_underlay_rule_is_unchanged_by_the_source_rule() {
    // One source root over the nano-ros tree: shadowing is ordered, not a
    // duplicate. A runner needs its declaration, a serdes warns.
    let s = scan(&["underlay", "ws"]);
    let e = override_error(resolve_runner_with(&s, "qemu", &policy(&[], &[])).unwrap_err());
    assert!(matches!(e, OverrideError::Undeclared { .. }), "{e}");
    let r = resolve_provider(&s, "serdes", "acme", &policy(&[], &[])).unwrap();
    assert!(r.resolution.winner.dir.ends_with("ws/src/my_acme"));
    assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
}

#[test]
fn a_package_reached_through_two_roots_is_one_package() {
    // A `package_paths` entry inside the workspace: the workspace root walks
    // it too. Same directory, so no duplicate.
    let s = scan_as(&[
        ("underlay", RootOrigin::NanoRosTree),
        ("ws", RootOrigin::Workspace),
        ("ws/src", RootOrigin::PackagePaths),
    ]);
    let r = resolve_provider(
        &s,
        "serdes",
        "acme",
        &policy(&[("acme", "src/my_acme")], &[]),
    )
    .expect("one package seen twice is not a duplicate");
    assert!(r.resolution.winner.dir.ends_with("ws/src/my_acme"));
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

// ---------------------------------------------------------------------------
// Where the declarations are read (W2c).
// ---------------------------------------------------------------------------

const BRINGUP_SYSTEM_TOML: &str = "[system]\nname = \"demo\"\nrmw = \"zenoh\"\ndomain_id = 0\n";

fn tmp_ws() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().canonicalize().unwrap();
    (tmp, ws)
}

#[test]
fn overrides_are_read_from_nros_toml() {
    let (_tmp, ws) = tmp_ws();
    std::fs::write(
        ws.join("nros.toml"),
        "[workspace]\npackage_paths = [\"vendor\"]\n\n[workspace.overrides]\nqemu = \"src/my_qemu\"\n",
    )
    .unwrap();
    // A bringup's system.toml without [overrides] is fine.
    let bringup = ws.join("src/demo_bringup");
    std::fs::create_dir_all(&bringup).unwrap();
    std::fs::write(bringup.join("system.toml"), BRINGUP_SYSTEM_TOML).unwrap();

    let p = nros_cli_core::provider_overrides::policy(&ws, &["legacy".into()]).unwrap();
    assert_eq!(p.declared["qemu"].as_written, "src/my_qemu");
    assert_eq!(p.declared["qemu"].path, ws.join("src/my_qemu"));
    assert!(p.allowed.contains("legacy"));
    assert_eq!(
        p.file,
        cargo_nano_ros::provider_overrides::OverridesFile::NrosToml
    );
    assert_eq!(
        p.declared_in.as_deref(),
        Some(ws.join("nros.toml").as_path())
    );
}

#[test]
fn a_leaf_without_nros_toml_reads_its_root_system_toml() {
    let (_tmp, ws) = tmp_ws();
    std::fs::write(
        ws.join("system.toml"),
        format!("{BRINGUP_SYSTEM_TOML}\n[overrides]\nqemu = \"my_qemu\"\n"),
    )
    .unwrap();
    let p = nros_cli_core::provider_overrides::policy(&ws, &[]).unwrap();
    assert_eq!(p.declared["qemu"].as_written, "my_qemu");
    assert_eq!(
        p.file,
        cargo_nano_ros::provider_overrides::OverridesFile::LeafSystemToml
    );
    assert_eq!(
        p.declared_in.as_deref(),
        Some(ws.join("system.toml").as_path())
    );
}

#[test]
fn overrides_in_a_package_system_toml_are_refused() {
    let (_tmp, ws) = tmp_ws();
    let vendored = ws.join("src/vendored_bringup");
    std::fs::create_dir_all(&vendored).unwrap();
    std::fs::write(
        vendored.join("system.toml"),
        format!("{BRINGUP_SYSTEM_TOML}\n[overrides]\nqemu = \"src/vendored_qemu\"\n"),
    )
    .unwrap();
    let e = nros_cli_core::provider_overrides::policy(&ws, &[])
        .unwrap_err()
        .to_string();
    assert!(e.contains("src/vendored_bringup/system.toml"), "{e}");
    assert!(e.contains("[workspace.overrides]"), "{e}");
    assert!(e.contains("cannot grant itself"), "{e}");
}

#[test]
fn overrides_in_both_nros_toml_and_root_system_toml_are_refused() {
    let (_tmp, ws) = tmp_ws();
    std::fs::write(
        ws.join("nros.toml"),
        "[workspace.overrides]\nqemu = \"src/my_qemu\"\n",
    )
    .unwrap();
    std::fs::write(
        ws.join("system.toml"),
        format!("{BRINGUP_SYSTEM_TOML}\n[overrides]\nqemu = \"src/my_qemu\"\n"),
    )
    .unwrap();
    let e = nros_cli_core::provider_overrides::policy(&ws, &[])
        .unwrap_err()
        .to_string();
    assert!(e.contains("nros.toml"), "{e}");
    assert!(e.contains("system.toml"), "{e}");
    assert!(e.contains("Keep the nros.toml table"), "{e}");
}

#[test]
fn nros_build_takes_allow_overriding() {
    use clap::Parser;
    let a = nros_cli_core::cmd::build::Args::try_parse_from([
        "build",
        "--allow-overriding",
        "qemu",
        "legacy",
        "--packages-select",
        "x",
    ])
    .unwrap()
    .split_cmake_args()
    .unwrap();
    assert_eq!(a.allow_overriding, ["qemu", "legacy"]);
    // The flag survives `--cmake-args`' swallowing, like every other flag.
    let a = nros_cli_core::cmd::build::Args::try_parse_from([
        "build",
        "--cmake-args",
        "-DX=1",
        "--allow-overriding",
        "qemu",
    ])
    .unwrap()
    .split_cmake_args()
    .unwrap();
    assert_eq!(a.allow_overriding, ["qemu"]);
    assert_eq!(a.cmake_args, ["-DX=1"]);
}
