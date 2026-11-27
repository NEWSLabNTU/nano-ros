//! Issue 2996 -- the nano-ros tree's own provider scan finds no test fixture.
//!
//! Every workspace's search path starts with the nano-ros tree (root 0), so a
//! fixture package the tree walk reaches is a provider every user workspace
//! resolves (`rmw = "acme"` found the out-of-tree-rmw fixture) and its knobs
//! are listed by `nros knob list` as if they shipped. A fixture tree keeps out
//! of that walk with an ignore marker at its root (`COLCON_IGNORE`), which
//! prunes descendants and never a root the caller names, so a test that uses
//! a fixture as its workspace still scans it.

use std::path::{Component, Path, PathBuf};

use cargo_nano_ros::provider_scan::scan_roots;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

/// A package below a crate's `tests/`, a `fixtures/` directory or the test
/// packages tree is test input, not something the SDK ships.
fn is_test_input(rel: &Path) -> bool {
    rel.starts_with("packages/testing")
        || rel
            .components()
            .any(|c| matches!(c, Component::Normal(n) if n == "tests" || n == "fixtures"))
}

#[test]
fn the_tree_scan_finds_no_test_fixture_provider() {
    let root = repo_root();
    let scan = scan_roots(std::slice::from_ref(&root)).expect("the tree scans");
    assert!(
        !scan.providers.is_empty(),
        "the tree scan found no provider at all, so this test proves nothing"
    );
    let leaked: Vec<String> = scan
        .providers
        .iter()
        .filter_map(|p| {
            let rel = p.dir.strip_prefix(&root).unwrap_or(&p.dir);
            is_test_input(rel).then(|| format!("  {} at {}", p.package, rel.display()))
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "the nano-ros tree's provider scan reaches test fixtures, so every user \
         workspace resolves them as providers:\n{}\nPut an empty `COLCON_IGNORE` \
         at the root of the fixture tree that holds them.",
        leaked.join("\n")
    );
}

/// The negative control: a fixture named as the root is still scanned, so
/// the markers keep fixtures out of the tree scan without hiding them from
/// the tests that use them.
#[test]
fn a_fixture_named_as_the_root_is_still_scanned() {
    let fixture = repo_root().join("packages/testing/nros-tests/fixtures/out-of-tree-rmw");
    assert!(
        fixture.join("COLCON_IGNORE").exists()
            || fixture.parent().unwrap().join("COLCON_IGNORE").exists(),
        "the fixture tree carries no ignore marker, so the control is vacuous"
    );
    let scan = scan_roots(std::slice::from_ref(&fixture)).expect("the fixture scans");
    assert!(
        scan.providers.iter().any(|p| p.package == "acme_rmw"),
        "naming the fixture as the root must still find its provider: {:?}",
        scan.providers
            .iter()
            .map(|p| &p.package)
            .collect::<Vec<_>>()
    );
}

/// The symptom the issue was filed for: `nros knob list` from an empty
/// workspace lists only core knobs and the built-in packages' knobs.
#[test]
fn knob_list_outside_a_workspace_lists_only_shipped_knobs() {
    let ws = tempfile::tempdir().unwrap();
    let cat = nros_cli_core::cmd::knob::workspace_catalog(Some(ws.path())).expect("catalog");
    let foreign: Vec<String> = nros_cli_core::cmd::knob::package_items(&cat)
        .into_iter()
        .filter(|i| i.root.as_deref() != Some("built in"))
        .map(|i| format!("  {} ({:?}, {:?})", i.name, i.package, i.root))
        .collect();
    assert!(
        foreign.is_empty(),
        "an empty workspace lists knobs nano-ros does not ship:\n{}",
        foreign.join("\n")
    );
}
