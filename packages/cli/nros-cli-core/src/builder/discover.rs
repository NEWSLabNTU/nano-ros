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
    collections::{BTreeMap, BTreeSet},
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

/// Stage 1b — narrow the workspace to the packages the user asked for
/// (RFC-0087 D7, phase-420 W7).
///
/// Empty means "no narrowing", which is the overwhelmingly common case and is
/// why [`select`] short-circuits on it: a build that named no package must be
/// byte-identical to one built before these flags existed.
///
/// ## Why this is a filter over an order, not a second order
///
/// [`discover`] already returns `packages` in topological order. A subset of a
/// topologically ordered list, taken in place, is a topological order of the
/// subset — every edge that survives had its endpoints in the original order,
/// and filtering removes elements without reordering the ones that remain. So
/// there is exactly one dependency sort in this crate and this does not add a
/// second one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// `--packages-select`: exactly these packages, no dependencies and no
    /// dependents. Empty imposes no constraint.
    pub select: Vec<String>,
    /// `--packages-up-to`: these packages plus everything they depend on,
    /// transitively, and nothing else. Empty imposes no constraint.
    pub up_to: Vec<String>,
}

impl Selection {
    /// Whether the user narrowed anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.select.is_empty() && self.up_to.is_empty()
    }
}

/// Apply `sel` to what stage 1 discovered.
///
/// ## Where this matches colcon and where it deliberately does not
///
/// Matched: `--packages-select A B` is exactly A and B; `--packages-up-to A` is
/// A and its transitive `<depend>` closure; the two flags **compose as an
/// intersection**, because each is an independent deselecting filter — that is
/// colcon's own shape (`packages_select` and `packages_up_to` each clear
/// `decorator.selected` for anything they do not name), and it is the only
/// composition under which adding a flag can never widen a build.
///
/// Diverged, twice, and both times toward failing instead of continuing:
///
/// **1. A name matching no package is an ERROR, not a warning.** colcon warns
/// and carries on, which is defensible there because the unmatched name might
/// legitimately name something in an install prefix. nano-ros has no install
/// prefix (RFC-0087 D8) — the selection is resolved against the source tree and
/// nothing else — so an unmatched name can only be a typo or a stale script.
/// Warning past it narrows the build to something the user did not ask for and
/// then reports success, which is the failure this codebase treats as worse
/// than a red. `plan::resolve` already answers an unknown IMAGE the same way,
/// with the available names in the message.
///
/// **2. A selection that drops a package another selected package needs is an
/// ERROR.** This is colcon's headline use for `--packages-select` — rebuild one
/// package against the install prefix the others already populated — and it
/// does not survive the port. nano-ros builds per-target static objects through
/// one merged root with no install-and-source stage (RFC-0087 D8), so a package
/// left out of the selection is not "already built and available"; it is simply
/// absent from the generated `[workspace] members` / `add_subdirectory` set.
/// Honouring colcon's answer would hand the user a half-built workspace whose
/// failure surfaces one layer down as an unresolved path dependency or a
/// missing CMake target — an error about the wrong thing. So the closure is
/// checked here and named here, with `--packages-up-to` offered as the fix.
///
/// ## What the check can and cannot see
///
/// It sees the `<depend>` graph, which is what `provider_scan` returns and what
/// the topological order is built from. It does NOT see a cargo `path`
/// dependency between two crates that declare nothing in `package.xml` — a
/// cargo-only member (`cargo_only`) carries no `depends` at all, deliberately,
/// because "cargo resolves its own dependency order from `Cargo.toml`". Dropping
/// one of those is still loud, just later and from cargo rather than from here.
pub fn select(found: &Discovered, sel: &Selection) -> Result<Discovered, String> {
    if sel.is_empty() {
        return Ok(found.clone());
    }

    let by_name: BTreeMap<&str, &WorkspacePackage> = found
        .packages
        .iter()
        .map(|p| (p.name.as_str(), p))
        .collect();

    // Both flags at once: a script with two typos should learn about both.
    let mut unknown: Vec<String> = Vec::new();
    for (flag, names) in [
        ("--packages-select", &sel.select),
        ("--packages-up-to", &sel.up_to),
    ] {
        for n in names {
            if !by_name.contains_key(n.as_str()) {
                unknown.push(format!("{flag} {n}"));
            }
        }
    }
    if !unknown.is_empty() {
        return Err(format!(
            "no such package in this workspace: {}.\n\nDiscovered: {}",
            unknown.join(", "),
            name_list(&found.packages)
        ));
    }

    let mut keep: Option<BTreeSet<&str>> = None;
    if !sel.select.is_empty() {
        keep = Some(sel.select.iter().map(String::as_str).collect());
    }
    if !sel.up_to.is_empty() {
        let closure = up_to_closure(&by_name, &sel.up_to);
        keep = Some(match keep {
            // Intersection — see the doc comment. Each flag deselects.
            Some(k) => k.intersection(&closure).copied().collect(),
            None => closure,
        });
    }
    let keep = keep.expect("a non-empty Selection sets at least one constraint");

    if keep.is_empty() {
        return Err(format!(
            "`--packages-select` and `--packages-up-to` select no package in \
             common, so there is nothing to build. They compose as an \
             INTERSECTION: each narrows, neither widens.\n\n  \
             --packages-select {}\n  --packages-up-to  {} (closure: {})",
            sel.select.join(" "),
            sel.up_to.join(" "),
            {
                let c = up_to_closure(&by_name, &sel.up_to);
                c.into_iter().collect::<Vec<_>>().join(", ")
            }
        ));
    }

    // The closure check. Runs over the FINAL set, so it covers a selection
    // broken by the intersection as well as one broken by `--packages-select`
    // alone — an `--packages-up-to` closure is complete by construction, but
    // intersecting it with a `--packages-select` can punch a hole in it.
    let mut broken: Vec<String> = Vec::new();
    for pkg in found
        .packages
        .iter()
        .filter(|p| keep.contains(p.name.as_str()))
    {
        let mut missing: Vec<&str> = pkg
            .depends
            .iter()
            .map(String::as_str)
            .filter(|d| *d != pkg.name && by_name.contains_key(d) && !keep.contains(d))
            .collect();
        if missing.is_empty() {
            continue;
        }
        missing.sort_unstable();
        broken.push(format!("  {} needs {}", pkg.name, missing.join(", ")));
    }
    if !broken.is_empty() {
        let mut up_to: Vec<&str> = keep.iter().copied().collect();
        up_to.sort_unstable();
        return Err(format!(
            "this selection drops packages that selected packages depend on:\n\
             \n{}\n\n\
             colcon allows that, because a dropped dependency is found in the \
             install prefix. nano-ros has none (RFC-0087 D8): it builds one \
             merged root with no install-and-source stage, so a package left \
             out is absent from the build, not resolved from a previous one. \
             The failure would surface a layer down as an unresolved path \
             dependency or a missing CMake target.\n\n\
             Build the closure instead:  --packages-up-to {}",
            broken.join("\n"),
            up_to.join(" ")
        ));
    }

    // Filter in place — see `Selection`'s note on why this needs no second sort.
    let packages: Vec<WorkspacePackage> = found
        .packages
        .iter()
        .filter(|p| keep.contains(p.name.as_str()))
        .cloned()
        .collect();
    let kept_dirs: BTreeSet<&PathBuf> = packages.iter().map(|p| &p.dir).collect();
    let cargo_only = found
        .cargo_only
        .iter()
        .filter(|d| kept_dirs.contains(d))
        .cloned()
        .collect();

    Ok(Discovered {
        packages,
        cargo_only,
        warnings: found.warnings.clone(),
    })
}

/// `roots` plus everything they `<depend>` on, transitively, restricted to
/// packages of this workspace.
///
/// External names (`std_msgs`) are skipped for the same reason
/// `topological_order` skips them: they impose no local build order and are not
/// ours to select.
fn up_to_closure<'a>(
    by_name: &BTreeMap<&'a str, &'a WorkspacePackage>,
    roots: &[String],
) -> BTreeSet<&'a str> {
    let mut out: BTreeSet<&str> = BTreeSet::new();
    let mut stack: Vec<&str> = roots
        .iter()
        .filter_map(|r| by_name.get_key_value(r.as_str()).map(|(k, _)| *k))
        .collect();
    while let Some(name) = stack.pop() {
        if !out.insert(name) {
            continue;
        }
        let Some(pkg) = by_name.get(name) else {
            continue;
        };
        for dep in &pkg.depends {
            if let Some((k, _)) = by_name.get_key_value(dep.as_str()) {
                stack.push(k);
            }
        }
    }
    out
}

/// Every discovered package name, sorted, for an error message.
fn name_list(packages: &[WorkspacePackage]) -> String {
    let mut names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    names.join(", ")
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

    // ---- phase-420 W7 — the selection verbs (RFC-0087 D7) ---------------

    /// The graph every selection test below reasons about:
    ///
    /// ```text
    ///   entry ──> talker_pkg ──> msgs_pkg
    ///     └─────> listener_pkg ─┘
    ///   other_pkg          (unrelated)
    ///   helper             (cargo member, no package.xml)
    /// ```
    ///
    /// `talker_pkg` also declares `std_msgs`, which is NOT a workspace package
    /// — the closure must ignore it rather than fail on it.
    fn selection_workspace(root: &Path) -> Discovered {
        write(
            &root.join("src/entry/package.xml"),
            &pkg_xml("entry", &["talker_pkg", "listener_pkg"]),
        );
        write(
            &root.join("src/talker_pkg/package.xml"),
            &pkg_xml("talker_pkg", &["msgs_pkg", "std_msgs"]),
        );
        write(
            &root.join("src/listener_pkg/package.xml"),
            &pkg_xml("listener_pkg", &["msgs_pkg"]),
        );
        write(
            &root.join("src/msgs_pkg/package.xml"),
            &pkg_xml("msgs_pkg", &[]),
        );
        write(
            &root.join("src/other_pkg/package.xml"),
            &pkg_xml("other_pkg", &[]),
        );
        write(
            &root.join("src/helper/Cargo.toml"),
            "[package]\nname = \"helper\"\n",
        );
        discover(root, &[root.join("src/helper")]).expect("discovers")
    }

    fn sel(select: &[&str], up_to: &[&str]) -> Selection {
        Selection {
            select: select.iter().map(|s| (*s).to_string()).collect(),
            up_to: up_to.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    fn names(d: &Discovered) -> Vec<&str> {
        d.packages.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn no_selection_is_the_whole_workspace() {
        // A build that named no package must be identical to one built before
        // these flags existed — not "the same set by a different route".
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let got = select(&d, &Selection::default()).expect("no-op");
        assert_eq!(got, d, "an empty selection must not perturb stage 1");
    }

    #[test]
    fn a_select_of_one_package_builds_only_that_one() {
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let got = select(&d, &sel(&["msgs_pkg"], &[])).expect("selects");
        assert_eq!(names(&got), vec!["msgs_pkg"], "no deps, no dependents");
        assert!(
            got.cargo_only.is_empty(),
            "the cargo-only member is not selected: {:?}",
            got.cargo_only
        );
    }

    #[test]
    fn up_to_takes_the_transitive_closure_and_nothing_more() {
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let got = select(&d, &sel(&[], &["talker_pkg"])).expect("selects");
        let mut n = names(&got);
        n.sort_unstable();
        assert_eq!(
            n,
            vec!["msgs_pkg", "talker_pkg"],
            "the closure is the package and what it depends on — `entry` \
             depends on it and must NOT come along, and `std_msgs` is not a \
             workspace package at all"
        );
    }

    #[test]
    fn up_to_reaches_through_two_edges() {
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let got = select(&d, &sel(&[], &["entry"])).expect("selects");
        let mut n = names(&got);
        n.sort_unstable();
        assert_eq!(
            n,
            vec!["entry", "listener_pkg", "msgs_pkg", "talker_pkg"],
            "transitive, and still excludes other_pkg and the cargo-only helper"
        );
    }

    #[test]
    fn a_selection_keeps_the_topological_order_it_was_given() {
        // The wave's rule: filter the order stage 1 computed, never sort again.
        // A subset of a topological order, taken in place, is a topological
        // order of the subset — this asserts the filter does not disturb it.
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let got = select(&d, &sel(&[], &["entry"])).expect("selects");
        let full = names(&d);
        let kept = names(&got);
        let expected: Vec<&str> = full.into_iter().filter(|n| kept.contains(n)).collect();
        assert_eq!(kept, expected, "order must be stage 1's, filtered");
        let pos = |n: &str| kept.iter().position(|k| *k == n).expect("present");
        assert!(pos("msgs_pkg") < pos("talker_pkg"), "{kept:?}");
        assert!(pos("talker_pkg") < pos("entry"), "{kept:?}");
    }

    #[test]
    fn an_unknown_name_is_an_error_listing_what_exists() {
        // Diverges from colcon, which warns. There is no install prefix the
        // unmatched name could name (RFC-0087 D8), so it is a typo, and warning
        // past it builds a set the user did not ask for and reports success.
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let e = select(&d, &sel(&["talkr_pkg"], &[])).expect_err("must refuse");
        assert!(e.contains("talkr_pkg"), "names what was asked for: {e}");
        assert!(e.contains("talker_pkg"), "names what exists: {e}");
        assert!(e.contains("msgs_pkg"), "lists ALL of them: {e}");
    }

    #[test]
    fn both_flags_report_their_unknown_names_at_once() {
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let e = select(&d, &sel(&["nope_a"], &["nope_b"])).expect_err("must refuse");
        assert!(e.contains("--packages-select nope_a"), "{e}");
        assert!(e.contains("--packages-up-to nope_b"), "{e}");
    }

    #[test]
    fn a_selection_that_drops_a_needed_dependency_is_refused() {
        // The decision this wave had to make, and the one place colcon's answer
        // does not port: colcon lets `--packages-select entry` succeed because
        // `talker_pkg` is in the install prefix. nano-ros has no install prefix
        // and one merged root, so the package would simply be absent.
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let e = select(&d, &sel(&["entry"], &[])).expect_err("must refuse");
        assert!(
            e.contains("entry needs listener_pkg, talker_pkg"),
            "names the hole: {e}"
        );
        assert!(e.contains("--packages-up-to entry"), "offers the fix: {e}");
    }

    #[test]
    fn an_unrelated_dependency_of_an_unselected_package_is_not_a_hole() {
        // Only edges OUT of kept packages matter. `other_pkg` being dropped
        // while `msgs_pkg` is kept is not a hole in either direction.
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let got = select(&d, &sel(&["msgs_pkg", "talker_pkg"], &[])).expect("closed");
        let mut n = names(&got);
        n.sort_unstable();
        assert_eq!(n, vec!["msgs_pkg", "talker_pkg"]);
    }

    #[test]
    fn the_two_flags_compose_as_their_intersection() {
        // colcon's composition: each flag deselects independently, so adding
        // one can only narrow. `--packages-up-to entry` is the four-package
        // closure; intersecting it with a two-name select leaves those two.
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let got = select(
            &d,
            &sel(&["talker_pkg", "msgs_pkg", "other_pkg"], &["entry"]),
        )
        .expect("selects");
        let mut n = names(&got);
        n.sort_unstable();
        assert_eq!(
            n,
            vec!["msgs_pkg", "talker_pkg"],
            "`other_pkg` is in the select and NOT in the up-to closure, so the \
             intersection drops it; `listener_pkg` is in the closure and not \
             the select, so the intersection drops that too"
        );
    }

    #[test]
    fn an_intersection_that_punches_a_hole_is_still_refused() {
        // The closure check runs over the FINAL set for this reason: an up-to
        // closure is complete by construction, and intersecting it is not.
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let e = select(&d, &sel(&["entry", "talker_pkg", "msgs_pkg"], &["entry"]))
            .expect_err("must refuse");
        assert!(e.contains("entry needs listener_pkg"), "{e}");
    }

    #[test]
    fn disjoint_flags_say_the_intersection_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let e = select(&d, &sel(&["other_pkg"], &["msgs_pkg"])).expect_err("nothing to build");
        assert!(e.contains("INTERSECTION"), "says how they compose: {e}");
        assert!(e.contains("other_pkg"), "{e}");
        assert!(e.contains("msgs_pkg"), "{e}");
    }

    #[test]
    fn a_cargo_only_member_is_selectable_by_its_directory_name() {
        // It is discovered under its directory name and carries NO `depends`
        // (cargo resolves its own order), so it is selectable and imposes no
        // constraint. Its cargo `path` edges are the documented blind spot in
        // `select`: dropping it is loud, but the noise comes from cargo.
        let tmp = tempfile::tempdir().unwrap();
        let d = selection_workspace(tmp.path());
        let got = select(&d, &sel(&["helper", "msgs_pkg"], &[])).expect("selects");
        let mut n = names(&got);
        n.sort_unstable();
        assert_eq!(n, vec!["helper", "msgs_pkg"]);
        assert_eq!(
            got.cargo_only,
            [tmp.path().join("src/helper")].into_iter().collect(),
            "a kept cargo-only member stays cargo-only"
        );
    }
}
