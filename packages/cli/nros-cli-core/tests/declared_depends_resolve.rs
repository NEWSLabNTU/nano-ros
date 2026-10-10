//! Issue 1509 rule 2 (phase-477 W4) — every `<depend>` an example workspace
//! declares resolves.
//!
//! Issue 1108 is the precedent: two template `package.xml` files declared
//! `<depend>nano-ros</depend>` — not even a legal ROS package name — and every
//! gate in reach stayed green, because a workspace that is never built by
//! `nros build` never meets the ladder that would refuse it.
//!
//! THE LADDER is `nros build`'s own (`cmd::build::declared_depends_report`,
//! phase-398 W3 / RFC-0062): workspace package → generated message →
//! `[prereq.*]` key → ament index → own buildtool → rosdep snapshot. A second
//! spelling of it is what the issue rules out, so this asks that function.
//!
//! THE SCOPE is `check-node-package-invariance`'s (`--list-roots`): every
//! `examples/workspaces/<ws>` and every bringup-shaped template. Asked of that
//! script, never re-derived, so rules 1–4 cover one set of roots.
//!
//! WHY HERE AND NOT ON THE FAST LINE: the ladder lives in the CLI, and no
//! fast-line gate may exec the CLI (issue 0466). `check-cli-tests` is in the
//! required pull-request context, so this still runs on every PR.
//!
//! THREE OUTCOMES (issue 1043). A name that resolves nowhere is a FAIL. But the
//! ament rung reads `AMENT_PREFIX_PATH`; on a host with no ament index the
//! ladder cannot tell a real ROS package from an unknown name, so an
//! unresolved list from such a host is NOT VERIFIED and the test says so
//! instead of failing — every other rung still runs and still fails the test.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use nros_cli_core::{
    builder::discover,
    cmd::build::{DependsReport, declared_depends_report},
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// The roots rules 1–4 share, from the script that owns the predicate.
fn scope_roots(repo: &Path) -> Vec<PathBuf> {
    let out = Command::new("python3")
        .arg(repo.join("scripts/check-node-package-invariance.py"))
        .arg("--list-roots")
        .current_dir(repo)
        .output()
        .expect("run check-node-package-invariance.py --list-roots");
    assert!(
        out.status.success(),
        "--list-roots failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let roots: Vec<PathBuf> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| repo.join(l.trim()))
        .collect();
    // The scope was 24 roots when this was written; an empty or tiny list
    // means the predicate broke, and a test over nothing passes.
    assert!(
        roots.len() >= 20,
        "--list-roots printed {} root(s) — the scope predicate moved?",
        roots.len()
    );
    roots
}

fn report_for(root: &Path, repo: &Path) -> DependsReport {
    let members = discover::cargo_members_or_walk(root);
    let found = discover::discover(root, &members)
        .unwrap_or_else(|e| panic!("{}: discover failed: {e}", root.display()));
    declared_depends_report(root, &found.packages, Some(repo))
        .unwrap_or_else(|e| panic!("{}: ladder failed: {e:#}", root.display()))
}

#[test]
fn every_example_workspace_depend_resolves() {
    let repo = repo_root();
    let mut failures = Vec::new();
    let mut not_verified = Vec::new();
    let roots = scope_roots(&repo);
    for root in &roots {
        let rel = root
            .strip_prefix(&repo)
            .unwrap_or(root)
            .display()
            .to_string();
        let r = report_for(root, &repo);
        for (u, role) in &r.wrong_role {
            failures.push(format!(
                "{rel}: `{}` names a role = {role} key (declared by {})",
                u.name,
                u.declared_by.join(", ")
            ));
        }
        for u in &r.unresolved {
            let line = format!(
                "{rel}: `{}` resolves to nothing (declared by {})",
                u.name,
                u.declared_by.join(", ")
            );
            if r.ros_indexed {
                failures.push(line);
            } else {
                not_verified.push(line);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} <depend> name(s) in the example workspaces do not resolve \
         (issue 1509 rule 2):\n  {}\n\nEach must be a package in that \
         workspace, a generated message package, a `[prereq.*]` key whose role \
         is `package`, a package the ament index provides, or a key in the \
         pinned rosdep snapshot — the same ladder `nros build` refuses on.",
        failures.len(),
        failures.join("\n  ")
    );
    if !not_verified.is_empty() {
        // NOT VERIFIED, not PASS: printed so a reader of the log sees which
        // names went unanswered, and why.
        eprintln!(
            "declared_depends_resolve: NOT VERIFIED — no ament index \
             (AMENT_PREFIX_PATH unset or empty), so these may be ROS packages \
             or may be typos; source a ROS setup to decide:\n  {}",
            not_verified.join("\n  ")
        );
    }
    eprintln!(
        "declared_depends_resolve: {} root(s), {} failure(s), {} not verified",
        roots.len(),
        failures.len(),
        not_verified.len()
    );
}

/// The negative control: a name nothing provides must FAIL, whatever the
/// host — so the test above cannot pass by never reaching the ladder. Built in
/// the repo's gitignored `tmp/`, never `/tmp` (repo rule).
#[test]
fn a_misspelled_depend_is_refused() {
    let repo = repo_root();
    let ws = repo
        .join("tmp")
        .join(format!("declared-depends-negative-{}", std::process::id()));
    let pkg = ws.join("src/talker_pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(ws.join(".colcon_workspace"), "").unwrap();
    std::fs::write(
        pkg.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="3">
  <name>talker_pkg</name>
  <version>0.1.0</version>
  <description>negative control</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <depend>std_msgs</depend>
  <depend>nano-ros</depend>
  <export><build_type>ament_cmake</build_type></export>
</package>
"#,
    )
    .unwrap();
    let r = report_for(&ws, &repo);
    let _ = std::fs::remove_dir_all(&ws);
    let names: Vec<&str> = r.unresolved.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["nano-ros"],
        "the ladder must refuse issue 1108's `nano-ros` and resolve `std_msgs` \
         (a bundled message package) — got {names:?}"
    );
}
