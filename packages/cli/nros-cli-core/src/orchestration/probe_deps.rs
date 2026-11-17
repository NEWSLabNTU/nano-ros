//! Issue 2680 -- the workspace packages a C/C++ metadata probe member needs
//! configured before it.
//!
//! The probe project ([`super::metadata_probe_cmake`]) adds each component
//! package with `add_subdirectory`. A node package whose sources include a
//! header from a sibling workspace LIBRARY package it `<depend>`s on compiles
//! in the real workspace build, which adds every package in `<depend>` order
//! (phase-348 W4), and failed in the probe, which added the node alone. The
//! consumer's workaround was an `if(NOT TARGET x) add_subdirectory(...)` in its
//! own CMakeLists: every such package had to know the probe configures it
//! alone.
//!
//! So the probe adds the member's build-time workspace dependency closure to
//! its ONE project (#4131's ruling: one project per workspace), dependencies
//! first. The closure is read from `package.xml` by the scan the workspace
//! order already uses (`provider_scan`), so the probe and the real build agree
//! on what a package needs.
//!
//! What is followed, and what is added:
//!
//! - **Followed:** `<depend>`, `<build_depend>`, `<build_export_depend>`,
//!   transitively. Not `<exec_depend>`: a bringup's run-time dependency on a
//!   node is not a build edge.
//! - **Not added**, though the walk still goes through their own dependencies:
//!   a name outside the workspace (found by `find_package` as before); an
//!   interface package (served by the Find-stubs, issue 0662; its verbatim
//!   `ament_cmake` CMakeLists cannot configure without ROS); a package with no
//!   `CMakeLists.txt` (nothing to add).
//! - **Refused:** a dependency cycle, naming the packages on it; a dependency
//!   that is a west application (issue 2280's rule), naming it.

use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    path::{Path, PathBuf},
};

use cargo_nano_ros::provider_scan::{self, WorkspacePackage};

/// A workspace package the probe adds for a member, ahead of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeDep {
    /// The package's `<name>`; also its binary dir in the probe project.
    pub name: String,
    /// The package directory (holds its `CMakeLists.txt`).
    pub dir: PathBuf,
    /// The other added packages this one needs before it, through any chain
    /// of build edges (a package the probe does not add included), so the
    /// probe orders it on true dependencies only.
    pub needs: Vec<String>,
}

/// The workspace packages the member at `package_dir` needs added, in an order
/// where each follows the workspace packages it depends on.
///
/// The workspace scanned is the member's parent directory: a component lives
/// at `<ws>/src/<pkg>`, the rule the probe's interface search root already
/// uses. A member the scan does not find has no workspace dependencies.
pub fn workspace_deps(package_dir: &Path) -> Result<Vec<ProbeDep>, String> {
    let Some(root) = package_dir.parent() else {
        return Ok(Vec::new());
    };
    let (pkgs, _) = provider_scan::scan_workspace_packages(root).map_err(|e| format!("{e:#}"))?;
    let Some(me) = pkgs.iter().find(|p| same_dir(&p.dir, package_dir)) else {
        return Ok(Vec::new());
    };
    let deps = closure_of(&pkgs, &me.name)?;
    for d in &deps {
        if crate::builder::west_app::cmakelists_finds_zephyr(&d.dir) {
            return Err(format!(
                "its workspace dependency `{}` ({}) is a west application (its CMakeLists.txt \
                 calls find_package(Zephyr)), which a host CMake probe cannot configure",
                d.name,
                d.dir.display()
            ));
        }
    }
    Ok(deps)
}

/// `member`'s build-time dependency closure within `pkgs`, filtered to the
/// packages the probe adds, in dependency order. Pure over the scan, so the
/// rules are testable without a filesystem walk (the add filter still reads
/// each candidate's directory).
pub fn closure_of(pkgs: &[WorkspacePackage], member: &str) -> Result<Vec<ProbeDep>, String> {
    let by_name: HashMap<&str, &WorkspacePackage> =
        pkgs.iter().map(|p| (p.name.as_str(), p)).collect();
    let Some(start) = by_name.get(member) else {
        return Ok(Vec::new());
    };
    // Every workspace package reachable from the member, the member included,
    // so a dependency that leads back to it is seen as the cycle it is.
    let mut seen: BTreeSet<&str> = BTreeSet::from([start.name.as_str()]);
    let mut queue: VecDeque<&WorkspacePackage> = VecDeque::from([*start]);
    while let Some(p) = queue.pop_front() {
        for d in &p.build_depends {
            if let Some(dep) = by_name.get(d.as_str())
                && seen.insert(dep.name.as_str())
            {
                queue.push_back(dep);
            }
        }
    }
    // Ordered by the workspace order's own sort, with build edges only: a
    // reachable set ordered on `depends` would let an `<exec_depend>` reorder
    // or even cycle it.
    let subset: Vec<WorkspacePackage> = seen
        .iter()
        .map(|n| {
            let mut p = by_name[n].clone();
            p.depends = p.build_depends.clone();
            p
        })
        .collect();
    let order = provider_scan::topological_order(&subset).map_err(|cycle| {
        format!(
            "dependency cycle among workspace packages: {cycle}; `<depend>` in these \
             package.xml files describes a loop, so no probe order satisfies it"
        )
    })?;
    let added: Vec<&WorkspacePackage> = order
        .iter()
        .filter(|p| p.name != member && probe_adds(&p.dir))
        .collect();
    let added_names: BTreeSet<&str> = added.iter().map(|p| p.name.as_str()).collect();
    Ok(added
        .iter()
        .map(|p| ProbeDep {
            name: p.name.clone(),
            dir: p.dir.clone(),
            needs: reachable(&by_name, p)
                .into_iter()
                .filter(|n| added_names.contains(n))
                .map(str::to_string)
                .collect(),
        })
        .collect())
}

/// Every workspace package `from` reaches over build edges, itself excluded.
fn reachable<'a>(
    by_name: &HashMap<&str, &'a WorkspacePackage>,
    from: &'a WorkspacePackage,
) -> BTreeSet<&'a str> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut queue: VecDeque<&WorkspacePackage> = VecDeque::from([from]);
    while let Some(p) = queue.pop_front() {
        for d in &p.build_depends {
            if let Some(dep) = by_name.get(d.as_str())
                && dep.name != from.name
                && seen.insert(dep.name.as_str())
            {
                queue.push_back(dep);
            }
        }
    }
    seen
}

/// Whether the probe adds this dependency package: it has a `CMakeLists.txt`
/// and is not an interface package.
fn probe_adds(dir: &Path) -> bool {
    dir.join("CMakeLists.txt").is_file() && !crate::interface_package::dir_is_interface_package(dir)
}

/// The freshness-key field for the member at `package_dir` (issue 1578's
/// key): its added dependencies, sorted by name, each with its source digest.
/// Empty when the member has none, so a key without dependencies is the key
/// from before this field. `None` when the closure cannot be resolved, which
/// is never fresh.
///
/// Only a CMake package has one: the C/C++ probe is the one that adds them.
pub fn key_field(package_dir: &Path) -> Option<String> {
    if !package_dir.join("CMakeLists.txt").is_file() {
        return Some(String::new());
    }
    let mut deps = workspace_deps(package_dir).ok()?;
    if deps.is_empty() {
        return Some(String::new());
    }
    deps.sort_by(|a, b| a.name.cmp(&b.name));
    let mut parts = Vec::with_capacity(deps.len());
    for d in &deps {
        let digest = super::metadata_refresh::source_digest(&d.dir).ok()?;
        parts.push(format!("{}@{digest}", d.name));
    }
    Some(format!(":deps={}", parts.join(",")))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write `<root>/<dir>/package.xml` naming `name`, with these tags.
    fn package(root: &Path, dir: &str, name: &str, tags: &[(&str, &str)], cmake: bool) -> PathBuf {
        let d = root.join(dir);
        std::fs::create_dir_all(&d).unwrap();
        let deps: String = tags
            .iter()
            .map(|(tag, n)| format!("  <{tag}>{n}</{tag}>\n"))
            .collect();
        std::fs::write(
            d.join("package.xml"),
            format!(
                "<?xml version=\"1.0\"?>\n<package format=\"3\">\n  <name>{name}</name>\n  \
                 <version>0.0.0</version>\n{deps}</package>\n"
            ),
        )
        .unwrap();
        if cmake {
            std::fs::write(d.join("CMakeLists.txt"), format!("project({name})\n")).unwrap();
        }
        d
    }

    fn names(deps: &[ProbeDep]) -> Vec<&str> {
        deps.iter().map(|d| d.name.as_str()).collect()
    }

    /// The ASI shape: the dependency's directory is not its package name, and
    /// it is matched on `<name>`. Transitive dependencies come first.
    #[test]
    fn deps_are_matched_by_name_and_ordered_dependencies_first() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src");
        let node = package(
            &src,
            "controller_pkg",
            "controller_pkg",
            &[
                ("depend", "autoware_controller_libs"),
                ("depend", "std_msgs"),
            ],
            true,
        );
        package(
            &src,
            "autoware",
            "autoware_controller_libs",
            &[("depend", "asi_common")],
            true,
        );
        package(&src, "asi_common", "asi_common", &[], true);
        let deps = workspace_deps(&node).unwrap();
        assert_eq!(names(&deps), ["asi_common", "autoware_controller_libs"]);
        assert_eq!(deps[1].dir, src.join("autoware"));
        assert_eq!(deps[1].needs, ["asi_common"]);
        assert!(deps[0].needs.is_empty());
    }

    /// Interface, CMakeLists-less and outside-workspace packages are not
    /// added, and `<exec_depend>` is not followed. The walk still passes
    /// THROUGH a package it does not add.
    #[test]
    fn filtered_packages_are_not_added_but_are_walked_through() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src");
        let node = package(
            &src,
            "node_pkg",
            "node_pkg",
            &[
                ("depend", "my_msgs"),
                ("build_depend", "py_helper"),
                ("exec_depend", "run_only_lib"),
                ("depend", "rclcpp"),
            ],
            true,
        );
        let msgs = package(
            &src,
            "my_msgs",
            "my_msgs",
            &[("build_export_depend", "behind_msgs")],
            true,
        );
        std::fs::create_dir_all(msgs.join("msg")).unwrap();
        package(&src, "py_helper", "py_helper", &[], false);
        package(&src, "run_only_lib", "run_only_lib", &[], true);
        package(&src, "behind_msgs", "behind_msgs", &[], true);
        let deps = workspace_deps(&node).unwrap();
        assert_eq!(names(&deps), ["behind_msgs"]);
    }

    #[test]
    fn a_cycle_is_refused_naming_its_packages() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src");
        let node = package(&src, "node_pkg", "node_pkg", &[("depend", "lib_a")], true);
        package(&src, "lib_a", "lib_a", &[("depend", "lib_b")], true);
        package(&src, "lib_b", "lib_b", &[("build_depend", "lib_a")], true);
        let why = workspace_deps(&node).unwrap_err();
        assert!(why.contains("dependency cycle"), "{why}");
        assert!(why.contains("lib_a") && why.contains("lib_b"), "{why}");
    }

    /// An `<exec_depend>` back to the member is not a build cycle.
    #[test]
    fn an_exec_depend_back_edge_is_not_a_cycle() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src");
        let node = package(&src, "node_pkg", "node_pkg", &[("depend", "lib_a")], true);
        package(&src, "lib_a", "lib_a", &[("exec_depend", "node_pkg")], true);
        assert_eq!(names(&workspace_deps(&node).unwrap()), ["lib_a"]);
    }

    #[test]
    fn a_west_application_dependency_is_refused_naming_it() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src");
        let node = package(
            &src,
            "node_pkg",
            "node_pkg",
            &[("depend", "board_glue")],
            true,
        );
        let glue = package(&src, "board_glue", "board_glue", &[], true);
        std::fs::write(
            glue.join("CMakeLists.txt"),
            "find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})\n",
        )
        .unwrap();
        let why = workspace_deps(&node).unwrap_err();
        assert!(
            why.contains("`board_glue`") && why.contains("west application"),
            "{why}"
        );
    }

    /// The key field: empty without dependencies (the key from before it),
    /// and moved by a dependency's source edit and by a `<depend>` edit.
    #[test]
    fn the_key_field_follows_dependency_sources_and_the_dependency_set() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src");
        let node = package(&src, "node_pkg", "node_pkg", &[], true);
        let lib = package(&src, "lib_pkg", "lib_pkg", &[], true);
        assert_eq!(key_field(&node).as_deref(), Some(""));

        package(&src, "node_pkg", "node_pkg", &[("depend", "lib_pkg")], true);
        let with_dep = key_field(&node).unwrap();
        assert!(with_dep.starts_with(":deps=lib_pkg@fnv1a64:"), "{with_dep}");

        std::fs::create_dir_all(lib.join("include/lib_pkg")).unwrap();
        std::fs::write(lib.join("include/lib_pkg/lib.hpp"), "int f();\n").unwrap();
        let edited = key_field(&node).unwrap();
        assert_ne!(
            edited, with_dep,
            "a dependency header edit must move the key"
        );

        package(&src, "node_pkg", "node_pkg", &[], true);
        assert_eq!(key_field(&node).as_deref(), Some(""));
    }
}
