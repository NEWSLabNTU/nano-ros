//! What makes a workspace package a ROS *interface* package — one spelling.
//!
//! An interface package declares message/service/action schemas and nothing
//! else. nano-ros does not build one the way ROS does: `nros sync` routes
//! `rosidl_generate_interfaces` through the nano-ros codegen pipeline and emits
//! a `generated/<pkg>` crate, so the package's own `CMakeLists.txt` — which is
//! deliberately verbatim upstream, `find_package(ament_cmake REQUIRED)` and all
//! — is never configured by us. It cannot be: `ament_cmake` exists only where
//! ROS is installed, and the whole point of the nano-ros build is that it is
//! not required.
//!
//! This predicate had two identical spellings in `cmd::ws` and was about to get
//! a third in `builder::cmake_root`. It is one function now — see issue 0886,
//! whose fix needed the third caller and consolidated the other two instead.
//!
//! The `member_of_group` marker is the canonical ROS declaration; the directory
//! probes catch packages that carry schemas without having declared it, which
//! `nros ws lint` warns about separately rather than treating as a
//! non-interface package.

use std::path::Path;

/// True iff `pkg_dir` (whose `package.xml` body is `manifest_body`) declares
/// message, service or action schemas.
pub fn is_interface_package(pkg_dir: &Path, manifest_body: &str) -> bool {
    manifest_body.contains("rosidl_interface_packages")
        || pkg_dir.join("msg").is_dir()
        || pkg_dir.join("srv").is_dir()
        || pkg_dir.join("action").is_dir()
}

/// The same question when only the directory is in hand — reads `package.xml`
/// itself. A directory with no readable manifest is not an interface package;
/// it is not a package at all, and whoever walked to it decides what that means.
pub fn dir_is_interface_package(pkg_dir: &Path) -> bool {
    let body = std::fs::read_to_string(pkg_dir.join("package.xml")).unwrap_or_default();
    is_interface_package(pkg_dir, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_member_of_group_marker_is_enough() {
        let d = tempfile::tempdir().expect("tempdir");
        assert!(is_interface_package(
            d.path(),
            "<member_of_group>rosidl_interface_packages</member_of_group>"
        ));
    }

    #[test]
    fn a_schema_directory_is_enough_without_the_marker() {
        for sub in ["msg", "srv", "action"] {
            let d = tempfile::tempdir().expect("tempdir");
            std::fs::create_dir(d.path().join(sub)).expect("mkdir");
            assert!(
                is_interface_package(d.path(), "<package format=\"3\"/>"),
                "a {sub}/ directory must count"
            );
        }
    }

    #[test]
    fn an_ordinary_node_package_is_not_one() {
        let d = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(d.path().join("src")).expect("mkdir");
        assert!(!is_interface_package(
            d.path(),
            "<package format=\"3\"><name>talker_pkg</name></package>"
        ));
    }

    #[test]
    fn a_missing_manifest_reads_as_not_one() {
        let d = tempfile::tempdir().expect("tempdir");
        assert!(!dir_is_interface_package(d.path()));
    }
}
