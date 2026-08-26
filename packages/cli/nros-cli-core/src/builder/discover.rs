//! Stage 1 — what is in this workspace (phase-383 W2.a/W2.b, RFC-0065 D1).
//!
//! ## Why a union and not just the `package.xml` walk
//!
//! colcon's answer is "every directory carrying a `package.xml`", and that is
//! the right answer for a ROS workspace. It is the WRONG answer for a cargo
//! workspace root we are about to synthesize, because a Cargo workspace member
//! need not be a ROS package.
//!
//! Measured on `nano-ros-rt-eval` (phase-383 F4): `src/island_clock/` is
//! `Cargo.toml` + `src/` and nothing else — a plain helper crate, listed in
//! `[workspace] members` and depended on by the node packages. A members list
//! derived from the `package.xml` walk alone drops it, and the build then fails
//! on an unresolved path dependency, pointing at a file the user never edited.
//!
//! So: **ROS packages come from the walk; Cargo membership comes from Cargo.**
//! Neither is authoritative for the other's question.
//!
//! ## Which walk is authoritative
//!
//! Two walks exist and they differ in more than their ignore markers
//! (issue 0809): `nros-pkg-index` descends INTO a package, `provider_scan` stops
//! at one "as colcon does not"; they prune different directory sets. A union
//! that mixed them would not be idempotent.
//!
//! [`provider_scan`] is authoritative here, for one reason: it is the only one
//! that returns `depends`, and stage 1's output feeds a topological order. The
//! pkg-index walk answers "where is package X" — a different question, asked
//! later by `$(find …)` substitution.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use cargo_nano_ros::provider_scan::{self, WorkspacePackage};

/// What stage 1 found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovered {
    /// Every package, in dependency order.
    pub packages: Vec<WorkspacePackage>,
    /// Cargo members carrying no `package.xml`, by directory. A subset of
    /// `packages`, kept separately because they are the ones a generated cargo
    /// root must list and a generated CMake root must NOT `add_subdirectory`.
    pub cargo_only: BTreeSet<PathBuf>,
    /// Non-fatal problems — a malformed `package.xml`, an unreadable dir. Worth
    /// printing; never worth failing on, because one bad file in a large tree
    /// must not hide the rest.
    pub warnings: Vec<String>,
}

/// Discover the workspace at `root`.
///
/// `cargo_members` is the caller's list of Cargo workspace member directories.
/// Passed IN rather than read here, deliberately: reading it means either
/// spawning `cargo metadata` (measured at 42 % of a `nros sync` run in this
/// tree — `ws.rs`) or hand-parsing `[workspace] members`, and the right choice
/// depends on whether the caller already holds one. Stage 1 should not make
/// that decision for every caller.
pub fn discover(root: &Path, cargo_members: &[PathBuf]) -> Result<Discovered, String> {
    let (mut packages, scan) = provider_scan::scan_workspace_packages(root)
        .map_err(|e| format!("scanning {}: {e}", root.display()))?;

    let mut warnings: Vec<String> = scan
        .errors
        .iter()
        .map(|e| format!("{}: {}", e.path.display(), e.message))
        .collect();

    let known: BTreeSet<PathBuf> = packages.iter().map(|p| p.dir.clone()).collect();
    let mut cargo_only = BTreeSet::new();

    for dir in cargo_members {
        let dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        if known.contains(&dir) {
            continue;
        }
        if !dir.join("Cargo.toml").is_file() {
            // A member naming a directory with no manifest is a broken cargo
            // workspace, and cargo will say so far better than we can. Not our
            // error to raise.
            warnings.push(format!(
                "cargo member {} has no Cargo.toml — skipped",
                dir.display()
            ));
            continue;
        }
        let name = match dir.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        // `depends` is EMPTY, and that is correct rather than lazy: cargo
        // resolves its own dependency order from `Cargo.toml`, so a cargo-only
        // member has no ordering constraint to express HERE. The topological
        // sort orders ROS packages against each other; cargo orders crates
        // against each other.
        packages.push(WorkspacePackage {
            name,
            dir: dir.clone(),
            depends: Default::default(),
        });
        cargo_only.insert(dir);
    }

    let packages = provider_scan::topological_order(&packages).map_err(|cycle| {
        format!(
            "dependency cycle among workspace packages: {}. No build order \
             satisfies it.",
            cycle
        )
    })?;

    Ok(Discovered {
        packages,
        cargo_only,
        warnings,
    })
}

/// Cargo workspace member directories declared at `root`.
///
/// Hand-parses `[workspace] members` rather than spawning `cargo metadata`.
/// The trade, stated because it is not free: `cargo metadata` also honours
/// `exclude` and `default-members` and resolves globs exactly, while this reads
/// the list and expands a trailing `/*`. What it buys is no subprocess — the
/// same reason `cmd/ws.rs` already does it this way.
///
/// **`exclude` matters here and is honoured**, because `examples/workspaces/rust`
/// excludes its two west entries, and a generated root that re-listed them as
/// members would try to build a Zephyr staticlib for the host.
pub fn cargo_workspace_members(root: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        return Vec::new();
    };
    let Ok(doc) = text.parse::<toml::Value>() else {
        return Vec::new();
    };
    let Some(ws) = doc.get("workspace") else {
        return Vec::new();
    };

    let excluded: BTreeSet<PathBuf> = ws
        .get("exclude")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(|s| root.join(s))
                .collect()
        })
        .unwrap_or_default();

    let mut out = Vec::new();
    let members = ws.get("members").and_then(|v| v.as_array());
    for entry in members.into_iter().flatten().filter_map(|v| v.as_str()) {
        if let Some(prefix) = entry.strip_suffix("/*") {
            let Ok(rd) = std::fs::read_dir(root.join(prefix)) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() && !excluded.contains(&p) {
                    out.push(p);
                }
            }
        } else if entry.contains('*') {
            // A mid-path glob. Skipped rather than guessed: cargo's glob
            // semantics are not ours to reimplement, and a wrong guess here
            // silently builds the wrong package set.
            continue;
        } else {
            let p = root.join(entry);
            if p.is_dir() && !excluded.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Cargo package directories found by WALKING, for a workspace with no root
/// manifest.
///
/// The chicken-and-egg this exists for: cargo-only members are normally read
/// from `[workspace] members` — but once RFC-0065 D13's migration deletes the
/// hand-written root, that list is the thing being generated. Without a walk,
/// the first `nros build` after the migration would silently drop every helper
/// crate that carries no `package.xml` (phase-383 F4, one step later).
///
/// Uses the same pruning as the package walk so the two agree about what is in
/// the workspace.
#[must_use]
pub fn cargo_packages_by_walk(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        // The canonical layout is <root>/src/<pkg>/, so three levels is
        // generous. A bound keeps a deep vendored tree from being scanned.
        if depth > 3 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(
                name,
                "target" | "build" | ".git" | ".cargo" | "node_modules" | "generated"
            ) || name.starts_with("build-")
                || name.starts_with("target-")
            {
                continue;
            }
            if [
                "COLCON_IGNORE",
                "AMENT_IGNORE",
                "NROS_IGNORE",
                ".nros-ignore",
            ]
            .iter()
            .any(|m| p.join(m).exists())
            {
                continue;
            }
            if p.join("Cargo.toml").is_file() {
                out.push(p);
                // Do not descend into a package.
                continue;
            }
            walk(&p, out, depth + 1);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out, 0);
    out.sort();
    out
}

/// Cargo members for `root`: the declared list, or a walk when none exists.
#[must_use]
pub fn cargo_members_or_walk(root: &Path) -> Vec<PathBuf> {
    let declared = cargo_workspace_members(root);
    if declared.is_empty() {
        cargo_packages_by_walk(root)
    } else {
        declared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn pkg_xml(name: &str, depends: &[&str]) -> String {
        let deps: String = depends
            .iter()
            .map(|d| format!("  <depend>{d}</depend>\n"))
            .collect();
        format!(
            "<?xml version=\"1.0\"?>\n<package format=\"3\">\n  \
             <name>{name}</name>\n  <version>0.0.0</version>\n  \
             <description>t</description>\n  <maintainer email=\"a@b.c\">m</maintainer>\n  \
             <license>Apache-2.0</license>\n{deps}</package>\n"
        )
    }

    #[test]
    fn a_cargo_member_without_package_xml_is_still_discovered() {
        // phase-383 F4 — nano-ros-rt-eval/src/island_clock is exactly this.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("src/talker_pkg/package.xml"),
            &pkg_xml("talker_pkg", &[]),
        );
        write(
            &root.join("src/island_clock/Cargo.toml"),
            "[package]\nname = \"island_clock\"\n",
        );

        let members = vec![root.join("src/talker_pkg"), root.join("src/island_clock")];
        let d = discover(root, &members).expect("discovers");

        let names: BTreeSet<&str> = d.packages.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains("talker_pkg"), "{names:?}");
        assert!(
            names.contains("island_clock"),
            "a cargo member with no package.xml must survive: {names:?}"
        );
        assert_eq!(d.cargo_only.len(), 1, "{:?}", d.cargo_only);
    }

    #[test]
    fn a_ros_package_is_not_double_counted_when_also_a_cargo_member() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("src/talker_pkg/package.xml"),
            &pkg_xml("talker_pkg", &[]),
        );
        write(
            &root.join("src/talker_pkg/Cargo.toml"),
            "[package]\nname = \"talker_pkg\"\n",
        );

        let d = discover(root, &[root.join("src/talker_pkg")]).expect("discovers");
        assert_eq!(d.packages.len(), 1, "{:?}", d.packages);
        assert!(
            d.cargo_only.is_empty(),
            "it IS a ROS package: {:?}",
            d.cargo_only
        );
    }

    #[test]
    fn packages_come_back_in_dependency_order() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("src/entry/package.xml"),
            &pkg_xml("entry", &["talker_pkg"]),
        );
        write(
            &root.join("src/talker_pkg/package.xml"),
            &pkg_xml("talker_pkg", &[]),
        );

        let d = discover(root, &[]).expect("discovers");
        let names: Vec<&str> = d.packages.iter().map(|p| p.name.as_str()).collect();
        let talker = names
            .iter()
            .position(|n| *n == "talker_pkg")
            .expect("present");
        let entry = names.iter().position(|n| *n == "entry").expect("present");
        assert!(
            talker < entry,
            "a dependency must precede its dependent: {names:?}"
        );
    }

    #[test]
    fn a_vendored_checkout_with_nros_ignore_is_not_discovered() {
        // phase-383 F9 + issue 0809 — nano-ros-rt-eval vendors nano-ros and
        // touches .nros-ignore precisely so this does not happen.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("src/talker_pkg/package.xml"),
            &pkg_xml("talker_pkg", &[]),
        );
        write(
            &root.join("nano-ros/packages/core/package.xml"),
            &pkg_xml("nros_core", &[]),
        );
        write(&root.join("nano-ros/.nros-ignore"), "vendored\n");

        let d = discover(root, &[]).expect("discovers");
        let names: Vec<&str> = d.packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["talker_pkg"],
            "vendored tree must stay out: {names:?}"
        );
    }

    #[test]
    fn a_dependency_cycle_fails_and_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("src/a/package.xml"), &pkg_xml("a", &["b"]));
        write(&root.join("src/b/package.xml"), &pkg_xml("b", &["a"]));

        let e = discover(root, &[]).expect_err("a cycle has no build order");
        assert!(e.contains("cycle"), "{e}");
    }

    #[test]
    fn members_are_read_with_exclude_honoured() {
        // examples/workspaces/rust excludes its two west entries; a generated
        // root that re-listed them would build a Zephyr staticlib for the host.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src/native_entry")).unwrap();
        std::fs::create_dir_all(root.join("src/zephyr_entry")).unwrap();
        write(
            &root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"src/native_entry\", \"src/zephyr_entry\"]\n\
             exclude = [\"src/zephyr_entry\"]\n",
        );

        let m = cargo_workspace_members(root);
        assert_eq!(m, vec![root.join("src/native_entry")], "{m:?}");
    }

    #[test]
    fn a_trailing_glob_member_expands() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src/a")).unwrap();
        std::fs::create_dir_all(root.join("src/b")).unwrap();
        write(
            &root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"src/*\"]\n",
        );

        let mut m = cargo_workspace_members(root);
        m.sort();
        assert_eq!(m, vec![root.join("src/a"), root.join("src/b")], "{m:?}");
    }

    #[test]
    fn with_no_root_manifest_cargo_packages_are_found_by_walking() {
        // Once D13 deletes the hand-written root, `[workspace] members` is the
        // thing being generated — so it cannot also be the source of truth for
        // which cargo packages exist.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("src/helper/Cargo.toml"),
            "[package]\nname = \"helper\"\n",
        );
        write(
            &root.join("src/talker_pkg/Cargo.toml"),
            "[package]\nname = \"t\"\n",
        );
        write(
            &root.join("src/talker_pkg/package.xml"),
            &pkg_xml("talker_pkg", &[]),
        );

        let m = cargo_members_or_walk(root);
        assert!(m.contains(&root.join("src/helper")), "{m:?}");
        assert!(m.contains(&root.join("src/talker_pkg")), "{m:?}");
    }

    #[test]
    fn a_declared_member_list_wins_over_the_walk() {
        // An authored `exclude` must be honoured; the walk cannot see it.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("src/a/Cargo.toml"), "[package]\nname = \"a\"\n");
        write(&root.join("src/b/Cargo.toml"), "[package]\nname = \"b\"\n");
        write(
            &root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"src/a\", \"src/b\"]\nexclude = [\"src/b\"]\n",
        );
        let m = cargo_members_or_walk(root);
        assert_eq!(m, vec![root.join("src/a")], "{m:?}");
    }

    #[test]
    fn the_walk_honours_ignore_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("src/a/Cargo.toml"), "[package]\nname = \"a\"\n");
        write(
            &root.join("vendored/nano-ros/Cargo.toml"),
            "[package]\nname = \"v\"\n",
        );
        write(&root.join("vendored/nano-ros/.nros-ignore"), "");
        let m = cargo_packages_by_walk(root);
        assert_eq!(m, vec![root.join("src/a")], "{m:?}");
    }

    #[test]
    fn no_cargo_toml_means_no_members_not_an_error() {
        // A pure C/C++ workspace is normal, not broken.
        let tmp = tempfile::tempdir().unwrap();
        assert!(cargo_workspace_members(tmp.path()).is_empty());
    }
}
