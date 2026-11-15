//! RFC-0094 D3 / phase-439 W3 — the routing rule, over the REAL example
//! workspaces rather than synthetic trees.
//!
//! `routing`'s own unit tests pin the rule against packages a test constructs.
//! That is the right place for the rule, and it is not enough on its own: every
//! one of those trees was written to exercise the rule, so none of them can tell
//! you the rule matches the tree it governs. These do, buildlessly — they read
//! `package.xml` and the build files that are already there.
//!
//! ## The package this exists for
//!
//! `examples/workspaces/mixed/src/rust_heartbeat_pkg` is the one routing change
//! with an observable consequence (RFC-0094 D3, found by phase-439 W0). It is a
//! Rust node in a workspace whose images build through CMake. It used to carry
//! a CMakeLists.txt registering it (`nano_ros_node_register … LANGUAGE RUST
//! SOURCES Cargo.toml`) and declare `nros_cmake`; since issue 2599 it carries
//! neither: it is a plain cargo package, and the CMake roads register it from
//! the bringup's `[[component]]` row (`builder::cargo_node`).
//!
//! Since RFC-0098 D9 there is no cargo workspace root to list it in — each image
//! is its own cargo root — so these tests ask the rule directly
//! (`routing::route(..).cargo_member`), which is what decided that list.

use std::path::{Path, PathBuf};

use nros_cli_core::builder::discover;

/// `<repo>` — `CARGO_MANIFEST_DIR` is `<repo>/packages/cli/nros-cli-core`.
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("three levels above nros-cli-core is the repo root")
        .to_path_buf()
}

fn workspace(name: &str) -> PathBuf {
    repo().join("examples/workspaces").join(name)
}

/// The headline: the mixed workspace's Rust node is a cargo package with no
/// CMake file, and the CMake roads register it from its bringup row.
#[test]
fn the_mixed_workspaces_rust_node_is_registered_from_its_bringup_row() {
    let ws = workspace("mixed");
    let pkg = ws.join("src/rust_heartbeat_pkg");
    // Preconditions, asserted rather than assumed — if the package gains a
    // CMakeLists again, this test is about nothing and must say so.
    assert!(
        pkg.join("Cargo.toml").is_file() && !pkg.join("CMakeLists.txt").exists(),
        "the package under test must be a cargo crate with no CMakeLists.txt: {}",
        pkg.display()
    );
    let found = discover::discover(&ws, &[]).expect("discovery must succeed");
    let p = found
        .packages
        .iter()
        .find(|p| p.name == "rust_heartbeat_pkg")
        .expect("discovered");
    let r = nros_cli_core::routing::route(p);
    assert!(
        !r.cmake_subdir,
        "no CMakeLists, so never a cmake subdir: {r:?}"
    );
    let derived = nros_cli_core::builder::cargo_node::derived_registrations(
        &ws.join("src/demo_bringup"),
        found
            .packages
            .iter()
            .map(|p| (p.name.as_str(), p.dir.as_path())),
    )
    .expect("the bringup rows parse");
    let names: Vec<(&str, &str, &str)> = derived
        .iter()
        .map(|n| (n.package.as_str(), n.name.as_str(), n.class.as_str()))
        .collect();
    assert_eq!(
        names,
        [(
            "rust_heartbeat_pkg",
            "heartbeat",
            "rust_heartbeat_pkg::Heartbeat"
        )],
        "the C and C++ packages register themselves; only the Rust one is derived"
    );
}

/// The negative control the headline needs: a Rust node that declares a CARGO
/// build type is still a cargo package. Without this, a rule that simply dropped
/// every Rust package would pass the test above.
#[test]
fn a_cargo_declaring_node_in_the_rust_workspace_is_still_a_cargo_member() {
    let ws = workspace("rust");
    let found = discover::discover(&ws, &[]).expect("discovery");
    let cargo_declaring: Vec<_> = found
        .packages
        .iter()
        .filter(|p| {
            p.dir.join("Cargo.toml").is_file()
                && p.build_type.as_deref().is_some_and(|b| b.contains("cargo"))
        })
        .collect();
    assert!(
        !cargo_declaring.is_empty(),
        "the rust workspace must still hold cargo-declaring packages, or this \
         control proves nothing"
    );
    for p in cargo_declaring {
        assert!(
            nros_cli_core::routing::route(p).cargo_member,
            "{} declares a cargo build type and must stay a cargo member",
            p.name
        );
    }
}

/// Every tracked example workspace passes the D3 intersection rule — never a
/// misdeclaration. This is the rule run over the trees the build actually walks,
/// next to the repo-wide buildless `check-package-routing.py`, which walks every
/// `package.xml` instead.
#[test]
fn no_example_workspace_declares_a_driver_it_cannot_be_built_by() {
    let root = repo().join("examples/workspaces");
    let mut checked = 0;
    for entry in std::fs::read_dir(&root).expect("examples/workspaces") {
        let ws = entry.expect("dir entry").path();
        if !ws.join("src").is_dir() {
            continue;
        }
        let found = match discover::discover(&ws, &[]) {
            Ok(f) => f,
            // A workspace this discovery cannot read is a different defect and
            // has its own gates; not silently skipped, but not this test's
            // verdict either.
            Err(e) => panic!("{}: discovery failed: {e}", ws.display()),
        };
        if let Err(e) = nros_cli_core::routing::check_declarations(&found.packages) {
            panic!("{}: {e}", ws.display());
        }
        checked += 1;
    }
    assert!(
        checked >= 4,
        "expected the four large language workspaces at least, walked {checked}"
    );
}

/// RFC-0098 D9 — no example workspace tracks a root build file. The builder
/// refuses an authored `[workspace]` root, so one committed here would break
/// every cargo image of that workspace, not merely be untidy.
#[test]
fn no_example_workspace_has_a_root_build_file_on_disk_from_git() {
    for entry in std::fs::read_dir(repo().join("examples/workspaces")).expect("dir") {
        let ws = entry.expect("dir entry").path();
        if !ws.join("src").is_dir() {
            continue;
        }
        // A generated root may exist on a developer's disk (gitignored); an
        // authored one is what this forbids. `<ws>/.cargo/config.toml` is not
        // checked: `nros sync` still writes it, gitignored, until phase-445 W6
        // retires that writer — whether one is TRACKED is
        // `check-cargo-config-tracked`'s question, not an on-disk one.
        let p = ws.join("CMakeLists.txt");
        if let Ok(text) = std::fs::read_to_string(&p) {
            assert!(
                text.starts_with("# GENERATED"),
                "{} is an authored root build file; a workspace has none (RFC-0098 D9)",
                p.display()
            );
        }
        assert_ne!(
            nros_cli_core::builder::cargo_root::state(&ws),
            nros_cli_core::builder::cargo_root::RootState::Authored,
            "{} carries an authored cargo `[workspace]` root (RFC-0098 D9)",
            ws.display()
        );
    }
}
