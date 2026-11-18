//! The CMake registration of a Rust node package, derived from its bringup.
//!
//! An image that runs C or C++ nodes builds through CMake, and there every node
//! reaches the image by registering with `nano_ros_node_register()`. A Rust
//! node package is a cargo crate: it needs no `CMakeLists.txt` of its own,
//! because the bringup's `[[component]]` row already states everything the
//! registration says (the package, the class and the instance name), and the
//! package's `Cargo.toml` is the source.
//!
//! So the roads that configure such an image derive the registration from the
//! row. A package that ships its own `CMakeLists.txt` keeps it: an explicit
//! registration wins, and the package is added as a subdirectory as before.
//!
//! This module is the one rule. The native cmake root
//! ([`super::cmake_root`]), the generated Zephyr west application
//! ([`super::west_app`]) and stage 3.5's registered population
//! (`cmd::entity_inventory::registered_from_probe`) all call it, and the cmake
//! side has one spelling too, `nros_register_cargo_node()` in
//! `cmake/NanoRosNodeRegister.cmake`.

// GitHub #3857 (issue 2599): ruling A, derive rather than keep a shim.

use std::path::{Path, PathBuf};

/// One derived `nano_ros_node_register(... LANGUAGE RUST ...)` call.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CargoNodeRegistration {
    /// The ROS package (the metadata row's `pkg`).
    pub package: String,
    /// The package's directory, which holds its `Cargo.toml`.
    pub dir: PathBuf,
    /// The component instance name (`NAME`): the row's `name`.
    pub name: String,
    /// The registered class (`CLASS`): the row's `class`.
    pub class: String,
}

impl CargoNodeRegistration {
    /// The arguments of `nros_register_cargo_node(<pkg> <dir> <name> <class>)`,
    /// with `dir` written as given (each road expresses the directory relative
    /// to its own file). The same four words are a `CARGO_NODES` tuple of
    /// `nano_ros_workspace()`.
    #[must_use]
    pub fn cmake_args(&self, dir: &str) -> String {
        format!("{} \"{dir}\" {} {}", self.package, self.name, self.class)
    }
}

/// Whether the package at `dir` is registered by derivation rather than by its
/// own build file: a cargo crate (`Cargo.toml`) that is a ROS package
/// (`package.xml`) and ships no `CMakeLists.txt`.
#[must_use]
pub fn needs_derived_registration(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file()
        && dir.join("package.xml").is_file()
        && !dir.join("CMakeLists.txt").is_file()
}

/// The derived registrations of the bringup at `bringup_dir`: one per
/// `[[component]]` row whose package is among `packages` (name, directory) and
/// [`needs_derived_registration`]. A row naming a package that is not in
/// `packages` is someone else's business (another image's, or an installed
/// package) and is skipped. Sorted, so a file rendered from it is stable.
///
/// Refuses a row that names such a package but no `class` or `name`: without
/// them there is nothing to register, and the build would otherwise stop later
/// on a component missing from `nros-metadata.json`.
pub fn derived_registrations<'a>(
    bringup_dir: &Path,
    packages: impl IntoIterator<Item = (&'a str, &'a Path)>,
) -> Result<Vec<CargoNodeRegistration>, String> {
    let path = bringup_dir.join("system.toml");
    let Ok(text) = nros_system_include::read_to_string(&path) else {
        // No system.toml, no rows: a bringup-less root registers nothing here.
        return Ok(Vec::new());
    };
    let doc: toml::Table = text
        .parse()
        .map_err(|e| format!("parsing {}: {e}", path.display()))?;
    let by_name: std::collections::BTreeMap<&str, &Path> = packages.into_iter().collect();
    let mut out = Vec::new();
    for row in doc
        .get("component")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(pkg) = row.get("pkg").and_then(toml::Value::as_str) else {
            continue;
        };
        let Some(dir) = by_name.get(pkg) else {
            continue;
        };
        if !needs_derived_registration(dir) {
            continue;
        }
        let field = |key: &str| -> Result<String, String> {
            row.get(key)
                .and_then(toml::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| {
                    format!(
                        "{}: the [[component]] row for `{pkg}` states no `{key}`. `{pkg}` is a \
                         Rust package with no CMakeLists.txt, so its CMake registration is \
                         derived from this row, which needs `pkg`, `class` and `name`",
                        path.display()
                    )
                })
        };
        out.push(CargoNodeRegistration {
            package: pkg.to_string(),
            dir: dir.to_path_buf(),
            name: field("name")?,
            class: field("class")?,
        });
    }
    out.sort();
    out.dedup();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(root: &Path, name: &str, files: &[&str]) -> PathBuf {
        let dir = root.join("src").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for f in files {
            std::fs::write(dir.join(f), "").unwrap();
        }
        dir
    }

    fn bringup(root: &Path, rows: &str) -> PathBuf {
        let dir = root.join("src").join("demo_bringup");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("system.toml"),
            format!("[system]\nname = \"d\"\n{rows}"),
        )
        .unwrap();
        dir
    }

    const ROWS: &str = "[[component]]\npkg = \"rust_pkg\"\nclass = \"rust_pkg::Beat\"\n\
                        name = \"beat\"\n\n[[component]]\npkg = \"c_pkg\"\n\
                        class = \"c_pkg::Talker\"\nname = \"talker\"\n";

    #[test]
    fn a_cargo_only_component_package_is_derived_and_a_cmake_one_is_not() {
        let d = tempfile::tempdir().unwrap();
        let rust = pkg(d.path(), "rust_pkg", &["Cargo.toml", "package.xml"]);
        let c = pkg(d.path(), "c_pkg", &["CMakeLists.txt", "package.xml"]);
        let b = bringup(d.path(), ROWS);
        let got = derived_registrations(&b, [("rust_pkg", rust.as_path()), ("c_pkg", c.as_path())])
            .unwrap();
        assert_eq!(
            got,
            vec![CargoNodeRegistration {
                package: "rust_pkg".into(),
                dir: rust,
                name: "beat".into(),
                class: "rust_pkg::Beat".into(),
            }]
        );
        assert_eq!(
            got[0].cmake_args("src/rust_pkg"),
            "rust_pkg \"src/rust_pkg\" beat rust_pkg::Beat"
        );
    }

    #[test]
    fn a_rust_package_with_its_own_cmakelists_keeps_it() {
        let d = tempfile::tempdir().unwrap();
        let rust = pkg(
            d.path(),
            "rust_pkg",
            &["Cargo.toml", "package.xml", "CMakeLists.txt"],
        );
        let b = bringup(d.path(), ROWS);
        assert!(!needs_derived_registration(&rust));
        assert!(
            derived_registrations(&b, [("rust_pkg", rust.as_path())])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_row_outside_the_package_set_is_skipped() {
        let d = tempfile::tempdir().unwrap();
        pkg(d.path(), "rust_pkg", &["Cargo.toml", "package.xml"]);
        let b = bringup(d.path(), ROWS);
        assert!(derived_registrations(&b, []).unwrap().is_empty());
    }

    #[test]
    fn a_row_with_no_class_is_refused_by_name() {
        let d = tempfile::tempdir().unwrap();
        let rust = pkg(d.path(), "rust_pkg", &["Cargo.toml", "package.xml"]);
        let b = bringup(
            d.path(),
            "[[component]]\npkg = \"rust_pkg\"\nname = \"beat\"\n",
        );
        let why = derived_registrations(&b, [("rust_pkg", rust.as_path())]).unwrap_err();
        assert!(why.contains("rust_pkg") && why.contains("`class`"), "{why}");
    }
}
