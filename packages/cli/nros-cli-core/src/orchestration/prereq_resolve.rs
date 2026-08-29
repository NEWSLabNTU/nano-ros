//! phase-398 W3 — resolve a workspace's `<depend>` names.
//!
//! RFC-0062 (amended 2026-08-29) settled the ladder. A `<depend>` is resolved
//! by asking, in order:
//!
//! 1. **a workspace package** — build ORDER, not a prerequisite. Today's
//!    behaviour, unchanged.
//! 2. **a generated message package** — `nros sync` owns it. Not decoration:
//!    `std_msgs` is a legitimate prerequisite key in other ecosystems and a
//!    GENERATED crate here, so without this rung sync and the resolver both
//!    claim it and the winner is whichever ran last.
//! 3. **a `[prereq.*]` key** — its provider installs it, its `check` decides.
//! 4. **a package the ambient ROS install provides** — the ament index.
//!    `ament_cmake`, `rclcpp` and friends are neither ours nor prerequisites we
//!    could install; they come with ROS. Measured on this tree: 42 of the 92
//!    names that resolved nowhere are exactly these.
//! 5. **otherwise UNKNOWN** — and that is an error, because the alternative is
//!    the silence this whole RFC exists to delete.
//!
//! ## Why the silence had to go
//!
//! A name that matched nothing was dropped without a word: `provider_scan`
//! says so in its own doc ("Includes names that are not workspace packages
//! (`std_msgs`); ordering ignores those rather than failing"). The first run of
//! this resolver over the tree found three `<exec_depend>` entries naming
//! packages that DO NOT EXIST — `reliable_talker_pkg`, `qos_listener_pkg`,
//! `param_talker_pkg`, stale since a rename — in workspaces that build green.
//! Nothing had ever looked.
//!
//! ## Scope
//!
//! A WORKSPACE, not the repository. `external/` and `third-party/` are
//! gitignored vendored checkouts of other people's code; 41 of the 50
//! genuinely-unresolved names in a repo-wide sweep were theirs, and none of
//! them is ours to declare.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// How one `<depend>` name resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// A package in this workspace — a build-order edge.
    WorkspacePackage,
    /// A message package `nros sync` generates.
    GeneratedMessage,
    /// A `[prereq.*]` key.
    Prereq,
    /// Provided by the ambient ROS install (the ament index).
    RosPackage,
    /// Nothing claims it.
    Unknown,
}

/// One unresolved name and the files that declare it.
#[derive(Clone, Debug)]
pub struct Unresolved {
    pub name: String,
    pub declared_by: Vec<String>,
}

/// The packages the ambient ROS install provides, from the ament index.
///
/// `AMENT_PREFIX_PATH` when set, else nothing — deliberately NOT a guess at
/// `/opt/ros/<distro>`, because a resolver that silently reaches a ROS the
/// caller did not select is how one tree resolves two ways (the reasoning that
/// removed the rosdep backend).
#[must_use]
pub fn ros_packages() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(paths) = std::env::var_os("AMENT_PREFIX_PATH") else {
        return out;
    };
    for prefix in std::env::split_paths(&paths) {
        let dir = prefix.join("share/ament_index/resource_index/packages");
        if let Ok(rd) = std::fs::read_dir(dir) {
            out.extend(
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned()),
            );
        }
    }
    out
}

/// Classify every `<depend>` name a workspace declares.
///
/// `workspace_packages` and `generated` are the first two rungs;
/// `prereq_keys` the third; `ros` the fourth.
#[must_use]
pub fn classify(
    name: &str,
    workspace_packages: &BTreeSet<String>,
    generated: &BTreeSet<String>,
    prereq_keys: &BTreeSet<String>,
    ros: &BTreeSet<String>,
) -> Resolution {
    if workspace_packages.contains(name) {
        Resolution::WorkspacePackage
    } else if generated.contains(name) {
        Resolution::GeneratedMessage
    } else if prereq_keys.contains(name) {
        Resolution::Prereq
    } else if ros.contains(name) {
        Resolution::RosPackage
    } else {
        Resolution::Unknown
    }
}

/// Every name a workspace's `package.xml` files declare, with where.
///
/// Reads the files directly rather than taking `provider_scan`'s `depends`,
/// because this needs the DECLARING FILE for the diagnostic — "which
/// package.xml names this?" is the first question anyone asks — and that scan
/// keeps only the set.
#[must_use]
pub fn declared_depends(ws_root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut stack = vec![ws_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if !matches!(
                    name.as_str(),
                    "build" | "target" | ".git" | "external" | "third-party" | "node_modules"
                ) && !name.starts_with("target-")
                {
                    stack.push(p);
                }
            } else if name == "package.xml"
                && let Ok(text) = std::fs::read_to_string(&p)
            {
                for dep in depend_names(&text) {
                    out.entry(dep).or_default().push(p.display().to_string());
                }
            }
        }
    }
    out
}

/// `<depend>`, `<build_depend>`, `<exec_depend>`, `<test_depend>` … all of them.
///
/// A tiny scanner rather than an XML parser: the tree has no namespaces or
/// entities in these files, and adding a dependency to read four tag names
/// would be the larger change.
#[must_use]
pub fn depend_names(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in xml.match_indices("depend>") {
        // Only an OPENING tag: `</exec_depend>` also contains "depend>".
        let open = xml[..i].rfind('<');
        let Some(open) = open else { continue };
        if xml[open..].starts_with("</") {
            continue;
        }
        let Some(rest) = xml.get(i + "depend>".len()..) else {
            continue;
        };
        let Some(end) = rest.find('<') else { continue };
        let value = rest[..end].trim();
        if !value.is_empty() {
            out.push(value.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The name of a `package.xml`'s own package.
#[must_use]
pub fn package_name(xml: &str) -> Option<String> {
    let i = xml.find("<name>")? + "<name>".len();
    let rest = xml.get(i..)?;
    let end = rest.find("</name>")?;
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    /// Every rung, in order, and the one that matters: an unmatched name is
    /// UNKNOWN rather than silently dropped.
    #[test]
    fn the_ladder_resolves_in_order() {
        let ws = set(&["talker_pkg"]);
        let msgs = set(&["std_msgs"]);
        let keys = set(&["libslirp"]);
        let ros = set(&["ament_cmake"]);
        let c = |n: &str| classify(n, &ws, &msgs, &keys, &ros);

        assert_eq!(c("talker_pkg"), Resolution::WorkspacePackage);
        assert_eq!(c("std_msgs"), Resolution::GeneratedMessage);
        assert_eq!(c("libslirp"), Resolution::Prereq);
        assert_eq!(c("ament_cmake"), Resolution::RosPackage);
        assert_eq!(c("nros-no-such-thing"), Resolution::Unknown);
    }

    /// The generated-message rung sits ABOVE the prereq rung on purpose.
    /// `std_msgs` is a legitimate prerequisite key elsewhere and a generated
    /// crate here; if the prereq rung won, `nros sync` and the resolver would
    /// both claim it and the winner would be whichever ran last.
    #[test]
    fn a_generated_message_outranks_a_prereq_key_of_the_same_name() {
        let both = set(&["std_msgs"]);
        assert_eq!(
            classify("std_msgs", &BTreeSet::new(), &both, &both, &BTreeSet::new()),
            Resolution::GeneratedMessage,
        );
    }

    /// A workspace package outranks everything — it is a build-order edge, not
    /// something to install.
    #[test]
    fn a_workspace_package_outranks_every_other_rung() {
        let all = set(&["talker_pkg"]);
        assert_eq!(
            classify("talker_pkg", &all, &all, &all, &all),
            Resolution::WorkspacePackage,
        );
    }

    /// All four depend spellings, and the closing tag must not be mistaken for
    /// an opening one — `</exec_depend>` also ends in `depend>`.
    #[test]
    fn every_depend_spelling_is_read_and_closing_tags_are_not() {
        let xml = "<package>\
                   <name>p</name>\
                   <depend>a</depend>\
                   <build_depend>b</build_depend>\
                   <exec_depend>c</exec_depend>\
                   <test_depend>d</test_depend>\
                   </package>";
        assert_eq!(depend_names(xml), vec!["a", "b", "c", "d"]);
        assert_eq!(package_name(xml).as_deref(), Some("p"));
    }

    /// Whitespace and duplicates: a name written twice is one dependency.
    #[test]
    fn names_are_trimmed_and_deduped() {
        let xml = "<depend> a </depend><exec_depend>a</exec_depend>";
        assert_eq!(depend_names(xml), vec!["a"]);
    }

    /// Without `AMENT_PREFIX_PATH` the ROS rung answers NOTHING rather than
    /// guessing `/opt/ros/<distro>`. A resolver that silently reaches a ROS the
    /// caller did not select is how one tree resolves two ways — the reasoning
    /// that removed the rosdep backend.
    #[test]
    fn the_ros_rung_does_not_guess_a_prefix() {
        let saved = std::env::var_os("AMENT_PREFIX_PATH");
        // SAFETY: single-threaded test; restored below.
        unsafe { std::env::remove_var("AMENT_PREFIX_PATH") };
        let empty = ros_packages().is_empty();
        if let Some(v) = saved {
            unsafe { std::env::set_var("AMENT_PREFIX_PATH", v) };
        }
        assert!(empty, "no AMENT_PREFIX_PATH ⇒ no ROS packages claimed");
    }
}
