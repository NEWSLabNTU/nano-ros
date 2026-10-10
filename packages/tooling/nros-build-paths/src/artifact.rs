//! Where a cargo-built nano-ros ARTIFACT lands — phase-484 W6b (RFC-0103 D7).
//!
//! `nros-c` / `nros-cpp` build to two kinds of output a non-cargo consumer has
//! to find: the staticlib (`lib<crate>.a`) and the per-build config header,
//! which the build script copies to a FLAT directory under the cargo target
//! dir (issue 0360) so it is findable without parsing cargo's JSON stream.
//! Both are a function of (cargo target dir, triple, profile), and before this
//! module PX4's cmake module, NuttX's `Make.defs` and the build scripts that
//! WRITE the header each spelled that function themselves —
//! `${NROS_REPO_DIR}/target/release/libnros_cpp.a`, a literal that ignores
//! `CARGO_TARGET_DIR` and `[build] target-dir` alike.
//!
//! The build scripts call [`Artifact::relative`]; every other road asks
//! `nros locate artifact <name>`, which reads the target dir from cargo itself
//! and joins this. Where the target dir IS is cargo's answer, never ours.

use std::path::{Path, PathBuf};

/// The flat per-build header directory `nros-c`'s build script writes.
pub const C_GENERATED_DIR: &str = "nros-c-generated";
/// The flat per-build header directory `nros-cpp`'s build script writes.
pub const CPP_GENERATED_DIR: &str = "nros-cpp-generated";

/// One artifact a non-cargo consumer locates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Artifact {
    /// `libnros_c.a` — the C API staticlib.
    CArchive,
    /// `libnros_cpp.a` — the C++ API staticlib (bundles `nros-c`).
    CppArchive,
    /// The include directory holding `nros/nros_config_generated.h`.
    CInclude,
    /// The include directory holding `nros/nros_cpp_config_generated.h`.
    CppInclude,
    /// `nros/nros_config_generated.h` itself.
    CConfigHeader,
    /// `nros/nros_cpp_config_generated.h` itself.
    CppConfigHeader,
}

impl Artifact {
    pub const ALL: [Artifact; 6] = [
        Artifact::CArchive,
        Artifact::CppArchive,
        Artifact::CInclude,
        Artifact::CppInclude,
        Artifact::CConfigHeader,
        Artifact::CppConfigHeader,
    ];

    /// The name `nros locate artifact` takes.
    pub fn name(self) -> &'static str {
        match self {
            Artifact::CArchive => "c-archive",
            Artifact::CppArchive => "cpp-archive",
            Artifact::CInclude => "c-include",
            Artifact::CppInclude => "cpp-include",
            Artifact::CConfigHeader => "c-config-header",
            Artifact::CppConfigHeader => "cpp-config-header",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == name)
    }

    /// Whether the path depends on the target triple and profile. The
    /// headers are written FLAT, once per target dir, so they do not.
    pub fn per_profile(self) -> bool {
        matches!(self, Artifact::CArchive | Artifact::CppArchive)
    }

    /// The path relative to the cargo target dir. `triple` is `None` for a
    /// host build cargo ran without `--target`; `profile` is the cargo
    /// PROFILE name (`dev`, `release`, a custom one), mapped to its directory
    /// by [`profile_dir`].
    pub fn relative(self, triple: Option<&str>, profile: &str) -> PathBuf {
        let header = |dir: &str, file: &str| PathBuf::from(dir).join("nros").join(file);
        match self {
            Artifact::CInclude => PathBuf::from(C_GENERATED_DIR),
            Artifact::CppInclude => PathBuf::from(CPP_GENERATED_DIR),
            Artifact::CConfigHeader => header(C_GENERATED_DIR, "nros_config_generated.h"),
            Artifact::CppConfigHeader => header(CPP_GENERATED_DIR, "nros_cpp_config_generated.h"),
            Artifact::CArchive | Artifact::CppArchive => {
                let lib = if self == Artifact::CArchive {
                    "libnros_c.a"
                } else {
                    "libnros_cpp.a"
                };
                let mut p = PathBuf::new();
                if let Some(t) = triple.filter(|t| !t.is_empty()) {
                    p.push(t);
                }
                p.push(profile_dir(profile));
                p.push(lib);
                p
            }
        }
    }

    /// The absolute path under `target_dir`.
    pub fn path(self, target_dir: &Path, triple: Option<&str>, profile: &str) -> PathBuf {
        target_dir.join(self.relative(triple, profile))
    }
}

/// The directory cargo writes a profile's output to: `dev` and `test` share
/// `debug`, `release` and `bench` share `release`, and a custom profile is its
/// own name.
pub fn profile_dir(profile: &str) -> &str {
    match profile {
        "dev" | "test" | "debug" => "debug",
        "release" | "bench" => "release",
        p => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for a in Artifact::ALL {
            assert_eq!(Artifact::parse(a.name()), Some(a));
        }
        assert_eq!(Artifact::parse("nros-c"), None);
    }

    #[test]
    fn archives_follow_triple_and_profile_headers_do_not() {
        let t = Path::new("/t");
        assert_eq!(
            Artifact::CppArchive.path(t, None, "release"),
            PathBuf::from("/t/release/libnros_cpp.a")
        );
        assert_eq!(
            Artifact::CArchive.path(t, Some("armv7a-nuttx-eabihf"), "release"),
            PathBuf::from("/t/armv7a-nuttx-eabihf/release/libnros_c.a")
        );
        assert_eq!(
            Artifact::CArchive.path(t, None, "dev"),
            PathBuf::from("/t/debug/libnros_c.a")
        );
        assert_eq!(
            Artifact::CConfigHeader.path(t, Some("x"), "release"),
            PathBuf::from("/t/nros-c-generated/nros/nros_config_generated.h")
        );
        assert_eq!(
            Artifact::CppInclude.path(t, None, "dev"),
            PathBuf::from("/t/nros-cpp-generated")
        );
    }

    #[test]
    fn a_custom_profile_is_its_own_directory() {
        assert_eq!(profile_dir("nros-minsizerel"), "nros-minsizerel");
    }
}
