//! RFC-0087 D2 / phase-420 W2 — the `<build_type>` vocabulary, read once.
//!
//! `<build_type>` says HOW a package is built, and a nano-ros-owned package
//! says so in its own words:
//!
//! | spelling      | build path | meaning |
//! | ---           | ---        | --- |
//! | `nros_cargo`  | cargo      | nano-ros-owned, canonical |
//! | `nros_cmake`  | cmake      | nano-ros-owned, canonical |
//! | `ament_cargo` | cargo      | an ament claim — legitimate on an interface pkg |
//! | `ament_cmake` | cmake      | ditto |
//! | `cargo`       | cargo      | a standalone project with no ROS identity |
//! | `cmake`       | cmake      | ditto |
//! | `ament_nros`  | cmake      | **retired** (5 in-tree uses, all cmake-side) |
//! | `nros_entry`  | cargo      | **retired** — encodes a ROLE, not a build system |
//! | `nros_bringup`| cmake      | **retired** — ditto |
//!
//! Two questions, deliberately separate, because W2 answers one and W3 the
//! other:
//!
//! * **Which build path is this?** — [`canonical`] answers it for every
//!   spelling, old and new. That is what "teach the reader both spellings"
//!   means: a reader that already understands `ament_cargo` must not stop
//!   understanding it the day `nros_cargo` appears, and must not need a second
//!   match arm to learn `nros_cargo`.
//! * **Is this the RIGHT spelling for this package's CLASS?** — it is not
//!   answerable here, because the class is a property of the package, not of
//!   the string. `scripts/check-build-type-spelling.py` answers it, from the
//!   evidence in the package directory, and the sweep that acts on the answer
//!   is phase-420 W3.
//!
//! So this module warns about exactly the spellings that are wrong for
//! **every** class — the three retired ones. A `DEPRECATION` on `ament_cargo`
//! would fire on 148 in-tree packages and on every legitimate interface
//! package, which trains readers to ignore it before W3 can act on it.
//!
//! ## Why the table is data and not a `match`
//!
//! Three readers need it: this one, `cmake/NanoRosPackageXml.cmake`, and the
//! gate. CLAUDE.md's recurring class is a rule that grew a second spelling
//! rather than a shared helper, and the rmw parity map is the case where two
//! green tools disagreed by 25 symbols because neither read the other. So the
//! table here is a literal array in a fixed shape, the cmake file carries the
//! same rows in a `set()` list, and the gate parses BOTH and refuses a
//! disagreement. A drift is then a red gate rather than two readers quietly
//! resolving one package differently.

use std::path::Path;

/// Which build system actually builds the package.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildPath {
    Cargo,
    Cmake,
}

/// `(raw spelling, canonical spelling, build path, retired)`.
///
/// Parsed by `scripts/check-build-type-spelling.py`, which cross-checks it
/// against the identical rows in `cmake/NanoRosPackageXml.cmake`. Keep the
/// literal shape — one tuple per line, string literals first.
const TABLE: &[(&str, &str, BuildPath, bool)] = &[
    ("nros_cargo", "nros_cargo", BuildPath::Cargo, false),
    ("nros_cmake", "nros_cmake", BuildPath::Cmake, false),
    ("ament_cargo", "nros_cargo", BuildPath::Cargo, false),
    ("ament_cmake", "nros_cmake", BuildPath::Cmake, false),
    ("cargo", "nros_cargo", BuildPath::Cargo, false),
    ("cmake", "nros_cmake", BuildPath::Cmake, false),
    // Retired. `ament_nros` is not an ament build type at all — no colcon
    // extension has ever registered it — so mapping it costs nothing that
    // worked before. All five in-tree uses are cmake-side (two CMakeLists
    // packages, three bringups, which generate a CMake root), which is what
    // decides the path here; the spelling itself does not say.
    ("ament_nros", "nros_cmake", BuildPath::Cmake, true),
    // `nros_entry` / `nros_bringup` name a ROLE. The role is already inferable
    // (a bringup carries `system.toml`; an entry declares one), so they carry
    // no information the tree does not already hold. Both in-tree `nros_entry`
    // packages are cargo; the one `nros_bringup` is a bringup.
    ("nros_entry", "nros_cargo", BuildPath::Cargo, true),
    ("nros_bringup", "nros_cmake", BuildPath::Cmake, true),
];

/// One row of [`TABLE`], resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildType {
    /// The spelling as authored.
    pub raw: &'static str,
    /// The RFC-0087 D2 spelling that means the same thing.
    pub canonical: &'static str,
    /// Which build system builds it.
    pub path: BuildPath,
    /// True for a spelling that has no legitimate remaining use in any class.
    pub retired: bool,
}

/// Resolve a `<build_type>` body, old spelling or new.
///
/// `None` for a value this project does not define — `ament_python` and
/// friends are perfectly valid ROS 2 build types that simply are not ours, so
/// an unknown value is "not mine to interpret", never an error. The gate is
/// what refuses an unknown value *inside this repository*.
pub fn canonical(raw: &str) -> Option<BuildType> {
    let raw = raw.trim();
    TABLE
        .iter()
        .find(|(spelling, ..)| *spelling == raw)
        .map(|&(raw, canonical, path, retired)| BuildType {
            raw,
            canonical,
            path,
            retired,
        })
}

/// The deprecation text for a retired spelling, or `None`.
///
/// Named separately from the printing so a caller that collects diagnostics
/// rather than printing them (a `nros check` lane, say) gets the same words.
pub fn retirement_notice(file: &Path, raw: &str) -> Option<String> {
    let bt = canonical(raw)?;
    if !bt.retired {
        return None;
    }
    Some(format!(
        "{}: <build_type>{}</build_type> is retired (RFC-0087 D2) — write \
         <build_type>{}</build_type>. It is read as `{}` meanwhile, so nothing \
         breaks today; phase-420 W3 removes the spelling.",
        file.display(),
        bt.raw,
        bt.canonical,
        bt.canonical
    ))
}

/// Print [`retirement_notice`] to stderr if the spelling is retired.
///
/// Naming the file is the whole point: the three retired spellings live in
/// test fixtures, so a warning that says only "a package uses `ament_nros`"
/// sends the reader grepping 406 `package.xml` files.
pub fn warn_if_retired(file: &Path, raw: &str) {
    if let Some(msg) = retirement_notice(file, raw) {
        eprintln!("warning: {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn both_spellings_resolve_to_one_build_path() {
        // The reader must not have learned the new spelling by forgetting the
        // old one — that is the migration hazard W2 exists to avoid, since the
        // tree is not rewritten until W3.
        for (old, new) in [("ament_cargo", "nros_cargo"), ("ament_cmake", "nros_cmake")] {
            let a = canonical(old).expect("legacy spelling must still resolve");
            let b = canonical(new).expect("canonical spelling must resolve");
            assert_eq!(a.path, b.path, "{old} and {new} must mean one build path");
            assert_eq!(a.canonical, b.canonical);
        }
    }

    #[test]
    fn plain_spellings_resolve_too() {
        // A standalone example keeps `cmake` / `cargo` (RFC-0087 D2), so the
        // reader has to understand them; it just must not claim they are ours.
        assert_eq!(canonical("cmake").unwrap().path, BuildPath::Cmake);
        assert_eq!(canonical("cargo").unwrap().path, BuildPath::Cargo);
    }

    #[test]
    fn a_foreign_build_type_is_not_an_error() {
        assert!(canonical("ament_python").is_none());
        assert!(canonical("").is_none());
    }

    #[test]
    fn surrounding_whitespace_is_not_a_different_build_type() {
        // `<build_type>\n    ament_cargo\n  </build_type>` is the shape a
        // hand-indented package.xml actually takes.
        assert_eq!(
            canonical("  ament_cargo\n").unwrap().canonical,
            "nros_cargo"
        );
    }

    #[test]
    fn only_the_retired_spellings_warn() {
        let f = PathBuf::from("src/demo/package.xml");
        for retired in ["ament_nros", "nros_entry", "nros_bringup"] {
            let msg = retirement_notice(&f, retired)
                .unwrap_or_else(|| panic!("{retired} must be reported as retired"));
            assert!(
                msg.contains("src/demo/package.xml"),
                "the warning must name the offending file, got: {msg}"
            );
            assert!(msg.contains(canonical(retired).unwrap().canonical));
        }
        // A blanket deprecation on `ament_cargo` would fire on 148 in-tree
        // packages AND on every legitimate interface package, so the class
        // question stays with the gate.
        for kept in ["ament_cargo", "ament_cmake", "cmake", "cargo", "nros_cargo"] {
            assert!(
                retirement_notice(&f, kept).is_none(),
                "{kept} is a class question, not a spelling question"
            );
        }
    }

    #[test]
    fn every_row_canonicalises_to_a_canonical_row() {
        // A table whose right-hand side names a spelling the table cannot read
        // back would send a migrating package to a value no reader accepts.
        for (raw, canon, ..) in TABLE {
            let back = canonical(canon)
                .unwrap_or_else(|| panic!("{raw} maps to {canon}, which does not resolve"));
            assert_eq!(back.canonical, *canon, "{canon} must be a fixed point");
            assert!(!back.retired, "{raw} must not map onto a retired spelling");
        }
    }
}
