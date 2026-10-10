//! Issue 1791 — a RELATIVE anchor finds the checkout it sits in.
//!
//! `nros build --workspace .` handed `"."` to the checkout walk, which climbs
//! with `Path::parent`: `"."` → `""` → none, so it never reached a real
//! directory and `abi_guard::runtime_root` fell through to the ambient
//! `NROS_REPO_DIR` — in a linked worktree, a different checkout at a different
//! codegen version, and the build refused.
//!
//! Its own test binary because it changes the process's current directory,
//! which no other test in a shared binary should have to tolerate.

use std::path::Path;

use nros_launcher::checkout::{MONOREPO_MARKER, find_monorepo_root};

#[test]
fn a_relative_anchor_finds_the_checkout_it_sits_in() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let marker = root.join(MONOREPO_MARKER);
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "[package]\n").unwrap();
    let workspace = root.join("examples/workspaces/c");
    std::fs::create_dir_all(&workspace).unwrap();

    std::env::set_current_dir(&workspace).unwrap();

    for anchor in [".", "src/.."] {
        let found = find_monorepo_root(Path::new(anchor)).map(|p| p.canonicalize().unwrap());
        assert_eq!(
            found.as_deref(),
            Some(root.as_path()),
            "anchor {anchor:?} from {}",
            workspace.display()
        );
    }
}
