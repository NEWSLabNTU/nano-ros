//! phase-373 W2 — one way to find the repo root, not twenty-three.
//!
//! `nros_tests::project_root()` is the helper. Before this gate the tests in
//! this directory carried **23 more** declarations of it under three names
//! (`workspace_root` x11, `repo_root` x6, `project_root` x6):
//!
//! * 17 hand reimplementations — `ancestors().nth(3)`, chained
//!   `.parent().unwrap()`, `.join("../../..")`
//! * 4 trivial aliases — `fn workspace_root() -> PathBuf { nros_tests::project_root() }`,
//!   which is a second spelling of the same call and nothing else
//! * 2 `canonicalize()` variants, which are **not equivalent to the other 21**:
//!   they resolve symlinks, so under a symlinked checkout they disagree about
//!   what the repo root is. That divergence was not a decision anyone made — it
//!   fell out of spelling the path as `../../..`, which has to be normalised to
//!   be usable. Nobody chose it and nothing recorded it.
//!
//! That last point is why this is a gate and not a cleanup. Twenty-three copies
//! of a pure function are ugly; twenty-three copies where two answer a different
//! question are a bug waiting for the first developer with a symlinked checkout.
//!
//! ## Scope, and what it deliberately does not cover
//!
//! Only `packages/testing/nros-tests/tests/`. Four sibling files elsewhere
//! (`nros-cli-core`, `rosidl-codegen`, `nros-rmw-cyclonedds`) define their own
//! root helper and are NOT covered — those crates do not depend on `nros-tests`
//! and should not grow a dependency on a heavy test-support crate just to reach
//! one path function. If a shared helper for them is ever wanted it belongs in a
//! smaller crate, and this gate should widen at the same time. Until then the
//! rule is enforced exactly where the helper is reachable.

use std::process::Command;

/// Local declarations that duplicate `nros_tests::project_root()`.
const FORBIDDEN: [&str; 3] = ["workspace_root", "repo_root", "project_root"];

#[test]
fn tests_use_the_one_project_root_helper() {
    let root = nros_tests::project_root();
    let dir = root.join("packages/testing/nros-tests/tests");

    // An index lookup, never a walk — same rule as `example_shape.rs`: the
    // working tree is full of build output and a walk reads it as source.
    let out = Command::new("git")
        .arg("ls-files")
        .arg("packages/testing/nros-tests/tests/*.rs")
        .current_dir(&root)
        .output()
        .expect("git ls-files");
    assert!(
        out.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let files: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    assert!(
        !files.is_empty(),
        "no test files listed under {} — the gate would pass vacuously",
        dir.display()
    );

    let mut violations = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(root.join(f)).expect("read test file");
        for (i, line) in src.lines().enumerate() {
            let t = line.trim_start();
            for name in FORBIDDEN {
                if t.starts_with(&format!("fn {name}()"))
                    || t.starts_with(&format!("pub fn {name}()"))
                {
                    violations.push(format!("{f}:{}: `fn {name}()`", i + 1));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "these tests declare their own repo-root helper instead of calling \
         `nros_tests::project_root()`:\n{}\n\nThere is one correct answer to \
         \"where is the repo root\" and one function that gives it. A local copy \
         is at best a second spelling; two of the copies this gate replaced also \
         resolved symlinks, so they answered a different question on a symlinked \
         checkout. See phase-373 W2.",
        violations.join("\n")
    );
}
