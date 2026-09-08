//! RFC-0094 D3 / phase-439 W3 — the routing rule, over the REAL example
//! workspaces rather than synthetic trees.
//!
//! `builder::{cargo_root, cmake_root}`'s own unit tests pin the rule against
//! packages a test constructs. That is the right place for the rule, and it is
//! not enough on its own: every one of those trees was written to exercise the
//! rule, so none of them can tell you the rule matches the tree it governs.
//! These do, buildlessly — they read `package.xml` and the build files that are
//! already there.
//!
//! ## The package this exists for
//!
//! `examples/workspaces/mixed/src/rust_heartbeat_pkg` is the one routing change
//! with an observable consequence (RFC-0094 D3, found by phase-439 W0). It is a
//! Rust node registered with the workspace's CMake build
//! (`nano_ros_node_register … LANGUAGE RUST SOURCES Cargo.toml`), it declares
//! `nros_cmake`, and it carries its OWN `[workspace]` table. Under the pre-W3
//! rule the generated cargo root listed it as a member, and cargo refuses that
//! root outright — MEASURED on this tree, 2026-09-08:
//!
//! ```text
//! $ cargo metadata --no-deps       # members = ["build/…", "src/rust_heartbeat_pkg"]
//! error: multiple workspace roots found in the same workspace:
//!   …/examples/workspaces/mixed/src/rust_heartbeat_pkg
//!   …/examples/workspaces/mixed
//! ```
//!
//! With the package excluded instead, the same command exits 0. So the repair
//! is not a tidier member list — it is the difference between a generated root
//! cargo can read and one it cannot.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use nros_cli_core::builder::{cargo_root, discover};

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

/// The generated cargo root for a workspace, with no cargo members passed in
/// and nothing excluded — so what the member list contains is decided ONLY by
/// RFC-0094 D3, which is what these tests are about.
fn cargo_root_for(ws: &Path) -> Result<String, String> {
    let found = discover::discover(ws, &[]).expect("discovery must succeed");
    cargo_root::render(
        &found,
        &ws.join("build/native"),
        &BTreeSet::new(),
        // One synthetic member standing in for the generated entry package
        // (`builder::entry`, phase-383 W3.b), which a real build always
        // contributes. Without it a workspace whose only cargo package is
        // cmake-driven renders an empty member list and errors — a true
        // answer that would hide the one being tested here.
        &[ws.join("build/native/entry")],
        None,
    )
}

/// Just the `members = [ … ]` block.
///
/// Sliced on the KEY, not on the first `]` — `[workspace]` two lines above
/// contains one, so a naive `find(']')` returns an empty slice that every
/// `!contains(…)` assertion passes and every `contains(…)` assertion fails.
/// Both directions of this test caught it, which is the only reason it is a
/// helper and not a one-liner.
fn members_block(body: &str) -> &str {
    let start = body.find("members = [").expect("a members list");
    let rest = &body[start..];
    &rest[..rest.find(']').expect("members list must be closed")]
}

/// The headline: the cmake-driven Rust node leaves the members list.
#[test]
fn the_mixed_workspaces_cmake_driven_rust_node_is_not_a_cargo_member() {
    let ws = workspace("mixed");
    let pkg = ws.join("src/rust_heartbeat_pkg");
    // Preconditions, asserted rather than assumed — if the package stops being
    // dual-file or stops declaring cmake, this test is about nothing and must
    // say so rather than passing.
    assert!(
        pkg.join("Cargo.toml").is_file() && pkg.join("CMakeLists.txt").is_file(),
        "the package under test must still carry BOTH build files: {}",
        pkg.display()
    );
    let xml = std::fs::read_to_string(pkg.join("package.xml")).expect("package.xml");
    assert!(
        xml.contains("<build_type>nros_cmake</build_type>"),
        "the package under test must still declare a cmake build type"
    );

    let body = cargo_root_for(&ws).expect("the mixed cargo root must render");
    assert!(
        !members_block(&body).contains("rust_heartbeat_pkg"),
        "cmake drives this package; listing it as a member makes the root \
         unreadable by cargo (`multiple workspace roots`):\n{body}"
    );
}

/// And it is EXCLUDED, not merely unlisted. Cargo walks up from a package to
/// find its workspace; a manifest under the root that is in neither list is an
/// error rather than an omission. (Here the package's own `[workspace]` would
/// stop that walk anyway — which is exactly why the exclusion must not depend
/// on noticing that, for the next package that has no such table.)
#[test]
fn the_cmake_driven_rust_node_is_excluded_from_the_generated_root() {
    let ws = workspace("mixed");
    let body = cargo_root_for(&ws).expect("renders");
    let excl = body
        .find("exclude = [")
        .expect("a manifest left out of members must be excluded");
    assert!(
        body[excl..].contains("rust_heartbeat_pkg"),
        "unlisted-and-unexcluded is a cargo error:\n{body}"
    );
}

/// The negative control the headline needs: a Rust node that declares a CARGO
/// build type is still a member. Without this, a rule that simply dropped every
/// Rust package would pass the test above.
#[test]
fn a_cargo_declaring_node_in_the_rust_workspace_is_still_a_member() {
    let ws = workspace("rust");
    let body = cargo_root_for(&ws).expect("the rust cargo root must render");
    let members = members_block(&body);
    let found = discover::discover(&ws, &[]).expect("discovery");
    let expected: Vec<&str> = found
        .packages
        .iter()
        .filter(|p| {
            p.dir.join("Cargo.toml").is_file()
                && p.build_type.as_deref().is_some_and(|b| b.contains("cargo"))
        })
        .map(|p| p.name.as_str())
        .collect();
    assert!(
        !expected.is_empty(),
        "the rust workspace must still hold cargo-declaring packages, or this \
         control proves nothing"
    );
    for name in expected {
        assert!(
            members.contains(name),
            "{name} declares a cargo build type and must stay a member:\n{body}"
        );
    }
}

/// Every tracked example workspace renders a root, or reports why — never a
/// misdeclaration. This is the D3 intersection rule run over the trees the
/// build actually walks, next to the repo-wide buildless
/// `check-package-routing.py`, which walks every `package.xml` instead.
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
