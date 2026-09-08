//! RFC-0094 D3 / phase-439 W3 — which driver builds a package, and whether it
//! is built here at all.
//!
//! ## Two questions, and conflating them is a defect in BOTH directions
//!
//! ```text
//! <build_type>   -> which DRIVER builds this package, IF it is built here
//! file presence  -> IS it built here
//! the gate       -> a PARTICIPATING package must have the files its type needs
//! ```
//!
//! Before W3 all three routing sites answered both questions with file
//! presence, and the `<build_type>` on 411 tracked packages was parsed by
//! nothing (issue 1207):
//!
//! | site | predicate it used |
//! | --- | --- |
//! | [`crate::builder::cargo_root`] | `Cargo.toml` exists -> `[workspace] members` |
//! | [`crate::builder::cmake_root`] | `CMakeLists.txt` exists -> `add_subdirectory` |
//! | [`crate::cmd::build`] | `CMakeLists.txt` exists -> the graph crosses languages |
//!
//! ## Why participation is NOT the declaration
//!
//! Routing on the declaration alone hard-fails **64** tracked packages, MEASURED
//! by phase-439 W0: 23 declare `nros_cargo` with no `Cargo.toml`, 22 declare
//! `nros_cmake` with no `CMakeLists.txt`, 12 `ament_cmake`, 2 `ament_cargo`, and
//! 5 declare nothing. They are not an accident — 34 are bringups (launch +
//! `system.toml`), 13 are platform/board descriptors, and 17 are interface
//! packages whose build files `nros sync` generates. Every one of them is
//! legitimately not built here, and [`route`] routes them nowhere.
//!
//! ## Why an UNKNOWN declaration falls back rather than failing
//!
//! `ament_python` is a perfectly valid ROS 2 build type that is simply not ours
//! — [`crate::build_type::canonical`] answers `None` for it, and so does this
//! module, which then reaches the pre-RFC-0094 answer. D3 changes what a
//! declaration MEANS; it does not invent one where none was written, and it does
//! not claim authority over a vocabulary this project does not define.
//!
//! ## One helper, not three predicates
//!
//! CLAUDE.md's recurring class is a rule that grew a second spelling instead of
//! a shared helper — the Zephyr unset-variable guard (#282 -> #326) is the
//! canonical case. So the rule lives here once and the three sites call it;
//! `scripts/check/check-package-routing.py` models the SAME rule and asserts
//! the sites still read this module rather than a file probe of their own.

use std::path::Path;

use cargo_nano_ros::provider_scan::WorkspacePackage;

use crate::build_type::{BuildPath, canonical};

/// Which lists a package belongs on. Both may be false (it participates in
/// neither), and after W3 both can never be true at once for a DECLARED
/// package — that is the whole point of the declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Routing {
    /// Belongs in a generated cargo root's `[workspace] members`.
    pub cargo_member: bool,
    /// Belongs in a generated cmake root's `add_subdirectory()` list.
    ///
    /// The INTERFACE-package subtraction (issue 0862) is not applied here: it
    /// is a property of what `nros sync` does with `rosidl_generate_interfaces`,
    /// not of D3's routing rule, and only the cmake root observes it.
    pub cmake_subdir: bool,
}

/// A package that participates and cannot be built by the driver it declares.
///
/// Before W3 this was **silently skipped** — the site routed it by whichever
/// file it did have, and the declaration was simply unread. Converting that
/// silence into a diagnosis is what the work item buys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Misdeclared {
    /// Package name, so the message names the thing the author has to edit.
    pub package: String,
    /// Its directory, because two packages can share a name across workspaces.
    pub dir: std::path::PathBuf,
    /// The spelling as authored, not the canonical form — the author has to
    /// find this string in their `package.xml`.
    pub declared: String,
    /// The file that declaration promises and the directory does not have.
    pub missing_file: &'static str,
}

impl std::fmt::Display for Misdeclared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: package.xml declares <build_type>{}</build_type>, so {} builds \
             it — but there is no {} in {}.\n    \
             Either write the build type that matches the files, or add the \
             file the declared driver needs. RFC-0094 D3: <build_type> selects \
             the DRIVER, file presence selects PARTICIPATION.",
            self.package,
            self.declared,
            match self.missing_file {
                "Cargo.toml" => "cargo",
                _ => "cmake",
            },
            self.missing_file,
            self.dir.display()
        )
    }
}

/// Does the package participate in a build at all?
///
/// File presence, never the declaration — see the module docs for the 64
/// packages that make this the only defensible answer.
#[must_use]
pub fn participates(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file() || dir.join("CMakeLists.txt").is_file()
}

/// The build path a package's declaration selects, or `None` when it declared
/// nothing this project defines.
#[must_use]
pub fn declared_path(build_type: Option<&str>) -> Option<BuildPath> {
    canonical(build_type?).map(|bt| bt.path)
}

/// RFC-0094 D3, for one package.
///
/// Total: a misdeclared package routes to its declared side, which it cannot
/// satisfy, and therefore routes NOWHERE. That is deliberate — dropping it
/// silently is exactly the pre-W3 behaviour, so [`misdeclaration`] must be
/// consulted separately and is what makes the drop loud.
#[must_use]
pub fn route(pkg: &WorkspacePackage) -> Routing {
    route_parts(&pkg.dir, pkg.build_type.as_deref())
}

/// [`route`] over the parts, for callers that hold no [`WorkspacePackage`].
#[must_use]
pub fn route_parts(dir: &Path, build_type: Option<&str>) -> Routing {
    let has_cargo = dir.join("Cargo.toml").is_file();
    let has_cmake = dir.join("CMakeLists.txt").is_file();
    if !(has_cargo || has_cmake) {
        return Routing::default();
    }
    match declared_path(build_type) {
        // Declared, and this project knows the spelling: the declaration picks
        // the side, file presence still decides participation.
        Some(BuildPath::Cargo) => Routing {
            cargo_member: has_cargo,
            cmake_subdir: false,
        },
        Some(BuildPath::Cmake) => Routing {
            cargo_member: false,
            cmake_subdir: has_cmake,
        },
        // Undeclared, or a build type that is not ours to interpret. Fall back
        // to file presence — the pre-RFC-0094 answer, unchanged.
        None => Routing {
            cargo_member: has_cargo,
            cmake_subdir: has_cmake,
        },
    }
}

/// D3's intersection rule: a participating package must carry the file its
/// declared driver needs.
#[must_use]
pub fn misdeclaration(pkg: &WorkspacePackage) -> Option<Misdeclared> {
    let dir = &pkg.dir;
    if !participates(dir) {
        // A package that participates in nothing cannot fail this: its
        // declaration is answering a question that does not arise. 64 tracked
        // packages are in exactly this state.
        return None;
    }
    let missing_file = match declared_path(pkg.build_type.as_deref())? {
        BuildPath::Cargo if !dir.join("Cargo.toml").is_file() => "Cargo.toml",
        BuildPath::Cmake if !dir.join("CMakeLists.txt").is_file() => "CMakeLists.txt",
        _ => return None,
    };
    Some(Misdeclared {
        package: pkg.name.clone(),
        dir: dir.clone(),
        declared: pkg.build_type.clone().unwrap_or_default(),
        missing_file,
    })
}

/// Every misdeclared package in a discovered set, as one message.
///
/// Reported TOGETHER rather than one per attempt, the same reasoning
/// `check-tier-preconditions` uses: an author fixing four declarations should
/// learn about four, not run the build four times.
pub fn check_declarations<'a>(
    packages: impl IntoIterator<Item = &'a WorkspacePackage>,
) -> Result<(), String> {
    let bad: Vec<Misdeclared> = packages.into_iter().filter_map(misdeclaration).collect();
    if bad.is_empty() {
        return Ok(());
    }
    let mut out = format!(
        "{} package(s) declare a build type they cannot be built by:\n\n",
        bad.len()
    );
    for m in &bad {
        out.push_str(&format!("  - {m}\n\n"));
    }
    out.push_str(
        "  This was a SILENT skip before RFC-0094 D3: the package was routed by \
         whichever\n  build file it happened to have, and the declaration was \
         never read.",
    );
    Err(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A package directory with the requested build files, and a declaration.
    fn pkg(dir: &Path, name: &str, cargo: bool, cmake: bool, bt: Option<&str>) -> WorkspacePackage {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        if cargo {
            std::fs::write(d.join("Cargo.toml"), "[package]\n").unwrap();
        }
        if cmake {
            std::fs::write(d.join("CMakeLists.txt"), "project(x)\n").unwrap();
        }
        WorkspacePackage {
            name: name.to_string(),
            dir: d,
            depends: Default::default(),
            build_type: bt.map(str::to_string),
        }
    }

    #[test]
    fn a_plain_cargo_leaf_is_a_cargo_member_only() {
        let t = tempfile::tempdir().unwrap();
        let p = pkg(t.path(), "talker_pkg", true, false, Some("nros_cargo"));
        assert_eq!(
            route(&p),
            Routing {
                cargo_member: true,
                cmake_subdir: false
            }
        );
        assert!(misdeclaration(&p).is_none());
    }

    #[test]
    fn a_plain_cmake_leaf_is_a_cmake_subdir_only() {
        let t = tempfile::tempdir().unwrap();
        let p = pkg(t.path(), "c_talker_pkg", false, true, Some("nros_cmake"));
        assert_eq!(
            route(&p),
            Routing {
                cargo_member: false,
                cmake_subdir: true
            }
        );
    }

    /// The headline of W3 and the whole reason it exists: a Zephyr west leaf
    /// carries a `Cargo.toml` as an implementation detail INSIDE a cmake build,
    /// and file presence swept it into the cargo members list.
    #[test]
    fn a_dual_file_package_declaring_cmake_leaves_the_cargo_members_list() {
        let t = tempfile::tempdir().unwrap();
        let p = pkg(t.path(), "zephyr_entry", true, true, Some("nros_cmake"));
        assert_eq!(
            route(&p),
            Routing {
                cargo_member: false,
                cmake_subdir: true
            },
            "cmake drives it; the Cargo.toml is inside that build"
        );
    }

    /// And symmetrically — a dual-file package declaring cargo is not a cmake
    /// subdirectory. Both directions, because conflating the two questions was
    /// a defect in both.
    #[test]
    fn a_dual_file_package_declaring_cargo_leaves_the_cmake_subdir_list() {
        let t = tempfile::tempdir().unwrap();
        let p = pkg(
            t.path(),
            "crate_with_harness",
            true,
            true,
            Some("nros_cargo"),
        );
        assert_eq!(
            route(&p),
            Routing {
                cargo_member: true,
                cmake_subdir: false
            }
        );
    }

    /// The 64. Routing on the declaration ALONE would hard-fail every one of
    /// them, which is why participation stays file presence.
    #[test]
    fn a_declaration_with_no_build_file_routes_nowhere_and_is_not_an_error() {
        let t = tempfile::tempdir().unwrap();
        for bt in ["nros_cargo", "nros_cmake", "ament_cmake", "ament_cargo"] {
            let p = pkg(t.path(), bt, false, false, Some(bt));
            assert_eq!(route(&p), Routing::default(), "{bt} must route nowhere");
            assert!(
                misdeclaration(&p).is_none(),
                "{bt}: a non-participant cannot fail the intersection — 34 \
                 bringups, 13 descriptors and 17 interface packages are here"
            );
        }
    }

    #[test]
    fn an_undeclared_package_falls_back_to_file_presence() {
        let t = tempfile::tempdir().unwrap();
        let p = pkg(t.path(), "undeclared", true, true, None);
        assert_eq!(
            route(&p),
            Routing {
                cargo_member: true,
                cmake_subdir: true
            },
            "5 tracked packages declare nothing; D3 must not invent one"
        );
        assert!(misdeclaration(&p).is_none());
    }

    #[test]
    fn a_foreign_build_type_falls_back_rather_than_failing() {
        let t = tempfile::tempdir().unwrap();
        let p = pkg(t.path(), "py_pkg", true, false, Some("ament_python"));
        assert_eq!(
            route(&p),
            Routing {
                cargo_member: true,
                cmake_subdir: false
            },
            "ament_python is a real ROS build type that is not ours to interpret"
        );
        assert!(misdeclaration(&p).is_none());
    }

    /// A legacy spelling must keep resolving — the tree is not rewritten, and a
    /// reader that learned `nros_cmake` by forgetting `ament_cmake` would move
    /// 148 packages at once (RFC-0087 D2).
    #[test]
    fn the_legacy_spellings_route_identically() {
        let t = tempfile::tempdir().unwrap();
        let old = pkg(t.path(), "old", true, true, Some("ament_cmake"));
        let new = pkg(t.path(), "new", true, true, Some("nros_cmake"));
        assert_eq!(route(&old), route(&new));
    }

    /// THE RED, direction one. Silently skipped before W3.
    #[test]
    fn a_cargo_declaration_over_a_cmake_only_dir_is_loud() {
        let t = tempfile::tempdir().unwrap();
        let p = pkg(t.path(), "wrong_way", false, true, Some("nros_cargo"));
        let m = misdeclaration(&p).expect("must fire");
        assert_eq!(m.missing_file, "Cargo.toml");
        let msg = m.to_string();
        assert!(msg.contains("wrong_way"), "must name the package: {msg}");
        assert!(msg.contains("nros_cargo"), "and the spelling: {msg}");
        // And it routes nowhere, so the participation is genuinely lost — which
        // is what makes the loud report load-bearing rather than cosmetic.
        assert_eq!(route(&p), Routing::default());
    }

    /// THE RED, direction two.
    #[test]
    fn a_cmake_declaration_over_a_cargo_only_dir_is_loud() {
        let t = tempfile::tempdir().unwrap();
        let p = pkg(t.path(), "other_way", true, false, Some("nros_cmake"));
        let m = misdeclaration(&p).expect("must fire");
        assert_eq!(m.missing_file, "CMakeLists.txt");
        assert!(m.to_string().contains("other_way"));
    }

    #[test]
    fn the_report_names_every_offender_not_just_the_first() {
        let t = tempfile::tempdir().unwrap();
        let a = pkg(t.path(), "first_bad", false, true, Some("nros_cargo"));
        let b = pkg(t.path(), "second_bad", true, false, Some("ament_cmake"));
        let ok = pkg(t.path(), "fine", true, false, Some("nros_cargo"));
        let err = check_declarations([&a, &b, &ok]).expect_err("two are broken");
        assert!(err.contains("first_bad"), "{err}");
        assert!(err.contains("second_bad"), "{err}");
        assert!(
            !err.contains("fine"),
            "the healthy package must not appear: {err}"
        );
        assert!(err.contains("2 package(s)"), "{err}");
    }

    #[test]
    fn a_healthy_workspace_reports_nothing() {
        let t = tempfile::tempdir().unwrap();
        let a = pkg(t.path(), "r", true, false, Some("nros_cargo"));
        let b = pkg(t.path(), "c", false, true, Some("nros_cmake"));
        assert!(check_declarations([&a, &b]).is_ok());
    }

    #[test]
    fn participation_is_file_presence_not_declaration() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("empty");
        std::fs::create_dir_all(&d).unwrap();
        assert!(!participates(&d));
        std::fs::write(d.join("CMakeLists.txt"), "").unwrap();
        assert!(participates(&d));
        assert!(!participates(&PathBuf::from("/nonexistent/nowhere")));
    }
}
