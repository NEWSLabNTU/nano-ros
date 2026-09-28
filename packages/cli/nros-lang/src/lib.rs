//! The target-language enumeration, declared once.
//!
//! RFC-0091 §3 / phase-432 W2.1. Before this crate the enumeration existed
//! TWICE — `codegen::entry::Lang` and `orchestration::ComponentLanguage`, the
//! same three variants with no relationship between them — so a language added
//! to one was invisible to the other. Issue **#1062** is that already shipped:
//! two language readers disagreeing, "and the loser is a silent `C`".
//!
//! ## Why a crate of its own, with one dependency
//!
//! Placement is FORCED, not chosen. The consumers are `nros-cli-core`,
//! `rosidl-codegen` and — the binding one — `nros-macros`, the `nros::main!()`
//! proc-macro. `rosidl-codegen` does not depend on `nros-pkg-index`, and the
//! proc-macro depends on neither `nros-orchestration-ir` nor `nros-cli-core`:
//! issue **0083** removed its `nros-build` dependency precisely because that
//! pulled the whole planner and orchestration tree into every USER's entry
//! build. No existing crate is reachable by all three.
//!
//! So the dependency list is `serde` and nothing else, and it must stay that
//! way — a heavy dependency here is a dependency in every downstream user's
//! build, which is the force that created the duplication this crate removes.
//!
//! ## One enumeration, many narrowings
//!
//! This is the ENUMERATION. It deliberately does not absorb the types that
//! merely look like it, because they are different concerns wearing one word
//! (RFC-0091 §1):
//!
//! * `cmd::generate::Lang` adds `All`, which is a CLI affordance and not a
//!   language.
//! * `ComponentLang { Rust, Other }` is a binary predicate.
//! * `PayloadLang { Rust, C }` is a genuine NARROWING — only two emitters ask
//!   that question and `Cpp` is not a valid answer.
//!
//! A narrowing should DERIVE from this type rather than re-spell it.

#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::string::String;
use core::fmt;

/// A target language nano-ros generates code for.
///
/// The `snake_case` serde representation is a COMPATIBILITY SURFACE, not a
/// detail: `SourceMetadata` writes this field to disk, so the strings `rust`,
/// `c` and `cpp` are what already-written metadata files contain. Changing
/// them silently invalidates every one of them, which is why
/// `serde_repr_is_the_on_disk_contract` pins them.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Rust,
    C,
    Cpp,
}

impl Language {
    /// Every language, in a stable order.
    ///
    /// Exists so a consumer that must handle all of them — a CLI `All`, a
    /// golden matrix — iterates rather than listing, and therefore cannot go
    /// stale when a variant is added.
    pub const ALL: [Language; 3] = [Language::Rust, Language::C, Language::Cpp];

    /// The canonical spelling — the same string the serde repr uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::C => "c",
            Language::Cpp => "cpp",
        }
    }

    /// Parse a user-supplied language name.
    ///
    /// Accepts the aliases a CLI must accept (`c++`, `cxx`) alongside the
    /// canonical spellings. Returns the offending input in the error so the
    /// caller can phrase its own message — this crate does not depend on an
    /// error library, and should not acquire one.
    pub fn parse(s: &str) -> Result<Self, UnknownLanguage> {
        match s {
            "rust" => Ok(Language::Rust),
            "c" => Ok(Language::C),
            "cpp" | "c++" | "cxx" => Ok(Language::Cpp),
            other => Err(UnknownLanguage {
                input: String::from(other),
            }),
        }
    }

    /// Read the `LANGUAGE` keyword of a cmake `nano_ros_node_register()` /
    /// `nano_ros_add_node()` call.
    ///
    /// # Why this is not [`Language::parse`]
    ///
    /// Two user-facing contracts, two token sets, and they are NOT the same
    /// set — which is the whole reason this is a second entry point rather
    /// than a second parser. [`Language::parse`] serves a CLI flag
    /// (`nros codegen entry-node --lang …`) and accepts `c++`; the cmake
    /// keyword set is defined by `cmake/NanoRosNodeRegister.cmake`'s own
    /// validator, which uppercases the argument and then accepts exactly
    /// `C`, `CPP`, `CXX`, `RUST` and `RS` — `C++` is a FATAL_ERROR there,
    /// because a bare `C++` does not survive cmake's own argument handling.
    ///
    /// Unifying the two would change what a user may type, in both
    /// directions: `--lang rs` would start working and `LANGUAGE C++` would
    /// stop being refused at configure. Neither is required by anything, so
    /// the sets stay as they are — but the TABLE lives here, beside the one
    /// it differs from, where a reader can see the difference and a new
    /// [`Language`] variant is a compile error in both.
    ///
    /// `None` means "not one of the keywords", and the caller owns what it
    /// does about that: `orchestration::workspace` prints the declaration back
    /// and falls to its class-shape heuristic (issue 0641), which is a policy
    /// about CMakeLists that were written before the keyword existed, not a
    /// fact about the language table.
    pub fn parse_cmake_keyword(s: &str) -> Option<Self> {
        let mut lowered = String::new();
        for ch in s.chars() {
            for lc in ch.to_lowercase() {
                lowered.push(lc);
            }
        }
        CMAKE_KEYWORDS
            .iter()
            .find(|(kw, _)| *kw == lowered.as_str())
            .map(|(_, lang)| *lang)
    }
}

/// The `LANGUAGE` keyword spellings `cmake/NanoRosNodeRegister.cmake` accepts,
/// lower-cased. Matched case-insensitively because the cmake side uppercases
/// before comparing, so `Cpp` and `CPP` are one token there.
const CMAKE_KEYWORDS: &[(&str, Language)] = &[
    ("c", Language::C),
    ("cpp", Language::Cpp),
    ("cxx", Language::Cpp),
    ("rust", Language::Rust),
    ("rs", Language::Rust),
];

/// What one source file's SPELLING says about the language that compiles it.
///
/// Three answers, not two, because a file list legitimately contains files
/// that decide nothing — and collapsing "decides nothing" into a language is
/// the whole defect (issue 1062, and the three cmake copies phase-469 S3
/// removes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceVerdict {
    /// The extension names a translation unit in this language.
    Decides(Language),
    /// A legitimate source entry that names no language. A `.h` is C or C++
    /// and the file cannot say which; an assembly TU is neither; a cmake
    /// generator expression is not a filename at all.
    Abstains,
}

/// Extensions that DECIDE, as data. Lower-cased before lookup except where the
/// case is the distinction: `.C` is a C++ TU to every C++ compiler while `.c`
/// is a C one, so that pair is matched before folding case.
const DECIDING: &[(&str, Language)] = &[
    ("c", Language::C),
    ("cpp", Language::Cpp),
    ("cxx", Language::Cpp),
    ("cc", Language::Cpp),
    ("c++", Language::Cpp),
    ("rs", Language::Rust),
];

/// Extensions that ABSTAIN. Headers are not translation units and a `.h` does
/// not know its own language; `.s`/`.S` are assembly. Listing them is what
/// makes the refusal below meaningful — without an abstain set, "not a source
/// language" and "a spelling nobody taught us" would be one answer.
const ABSTAINING: &[&str] = &["h", "hpp", "hh", "hxx", "h++", "inc", "ipp", "def", "s"];

impl Language {
    /// Which language compiles this source path — the ONE producer of that
    /// answer (RFC-0091 §3; phase-469 S3).
    ///
    /// # Why it can refuse
    ///
    /// Before this existed the answer was inferred in five places — three cmake
    /// sites, `orchestration::workspace`, and `cmd::build` — and every one of
    /// them was TOTAL: each mapped an unrecognised spelling onto a language
    /// rather than saying it did not know. Two mapped it to `c` (any source
    /// without a C++ extension) and one to `cpp` (any source whose extension
    /// was not exactly `.c`), so the same `Cargo.toml` in a `SOURCES` list was
    /// C to one reader and C++ to another. Neither answer is right, and the
    /// wrong one lands as a link error against symbols the other ABI never
    /// emitted, or as a TU handed to the wrong compiler.
    ///
    /// So an unknown spelling is an ERROR naming what was seen, and a spelling
    /// that is a real source but names no language is `Abstains` — which the
    /// caller resolves with a default it states OUT LOUD, rather than one
    /// hidden in a fallthrough.
    pub fn of_source(path: &str) -> Result<SourceVerdict, UnknownSourceExtension> {
        // A cmake generator expression is not a path. It can carry `.` and a
        // trailing `>`, so extension-splitting it produces nonsense; it names
        // object files whose own language is decided elsewhere.
        if path.contains("$<") {
            return Ok(SourceVerdict::Abstains);
        }
        // The extension is what follows the last `.` in the last path
        // component. A dot in a DIRECTORY name (`build-1.2/main.c`, and the
        // extensionless `src/Makefile` beside it) must not be read as one.
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let Some((_, ext)) = name.rsplit_once('.') else {
            return Err(UnknownSourceExtension {
                path: String::from(path),
            });
        };
        if ext.is_empty() {
            return Err(UnknownSourceExtension {
                path: String::from(path),
            });
        }
        // `.C` vs `.c` is the one place case IS the fact.
        if ext == "C" {
            return Ok(SourceVerdict::Decides(Language::Cpp));
        }
        let mut lowered = String::new();
        for ch in ext.chars() {
            for lc in ch.to_lowercase() {
                lowered.push(lc);
            }
        }
        for (candidate, lang) in DECIDING {
            if *candidate == lowered.as_str() {
                return Ok(SourceVerdict::Decides(*lang));
            }
        }
        for candidate in ABSTAINING {
            if *candidate == lowered.as_str() {
                return Ok(SourceVerdict::Abstains);
            }
        }
        Err(UnknownSourceExtension {
            path: String::from(path),
        })
    }

    /// Fold a source list into the one language that compiles the target, or
    /// `None` when the list settles nothing.
    ///
    /// C++ WINS over C, which is not a tie-break but the linker's rule: a
    /// target holding one C++ TU needs the C++ driver and the C++ runtime
    /// umbrella. Rust cannot be mixed in — a Rust source beside a C-family one
    /// is a target with two link roots, which is a caller error rather than a
    /// language to pick.
    ///
    /// `None` is the issue-1062 answer, kept deliberately distinct from `C`:
    /// "every source is a C file" and "this list told me nothing" had one
    /// spelling, and the second is common — a `SOURCES ${var}` that expands to
    /// nothing, or a list of headers.
    pub fn of_sources<'a, I>(sources: I) -> Result<Option<Language>, SourceLanguageError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut c_family: Option<Language> = None;
        let mut rust = false;
        for src in sources {
            match Language::of_source(src)? {
                SourceVerdict::Abstains => {}
                SourceVerdict::Decides(Language::Rust) => rust = true,
                SourceVerdict::Decides(Language::Cpp) => c_family = Some(Language::Cpp),
                SourceVerdict::Decides(Language::C) => {
                    if c_family.is_none() {
                        c_family = Some(Language::C);
                    }
                }
            }
        }
        match (rust, c_family) {
            (true, Some(other)) => Err(SourceLanguageError::Mixed(other)),
            (true, None) => Ok(Some(Language::Rust)),
            (false, answer) => Ok(answer),
        }
    }
}

/// Why a source list produced no language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceLanguageError {
    /// A spelling this table does not know.
    Unknown(UnknownSourceExtension),
    /// Rust sources beside C-family ones: two link roots in one target.
    Mixed(Language),
}

impl From<UnknownSourceExtension> for SourceLanguageError {
    fn from(e: UnknownSourceExtension) -> Self {
        SourceLanguageError::Unknown(e)
    }
}

impl fmt::Display for SourceLanguageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceLanguageError::Unknown(e) => e.fmt(f),
            SourceLanguageError::Mixed(other) => write!(
                f,
                "sources mix rust with {other}: a target has one link root, so \
                 state the language rather than inferring it"
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SourceLanguageError {}

/// `Language::of_source` was given a spelling the table does not know.
///
/// It names the PATH rather than the extension: a build script's list is
/// usually variable-expanded, so the extension alone does not say which entry
/// to edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownSourceExtension {
    pub path: String,
}

impl fmt::Display for UnknownSourceExtension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "source `{}` has no known language extension (deciding: ",
            self.path
        )?;
        for (i, (ext, _)) in DECIDING.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, ".{ext}")?;
        }
        f.write_str("; .C is C++; carrying no language: ")?;
        for (i, ext) in ABSTAINING.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, ".{ext}")?;
        }
        f.write_str(")")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for UnknownSourceExtension {}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `Language::parse` was given something that is not a language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownLanguage {
    pub input: String,
}

impl fmt::Display for UnknownLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown language `{}` (expected one of: rust, c, cpp)",
            self.input
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for UnknownLanguage {}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString};

    use super::*;

    /// The on-disk contract. `SourceMetadata.language` is serialized into
    /// metadata files that already exist on users' disks, so these three
    /// strings are not a naming choice — changing one invalidates every file
    /// written before the change, silently, because `deny_unknown_fields` is
    /// about KEYS and an unknown variant fails at the value.
    #[test]
    fn serde_repr_is_the_on_disk_contract() {
        for (lang, spelling) in [
            (Language::Rust, "\"rust\""),
            (Language::C, "\"c\""),
            (Language::Cpp, "\"cpp\""),
        ] {
            let encoded = serde_json::to_string(&lang).expect("serialize");
            assert_eq!(encoded, spelling, "{lang:?} changed its on-disk spelling");
            let decoded: Language = serde_json::from_str(spelling).expect("deserialize");
            assert_eq!(decoded, lang);
        }
    }

    /// `as_str` and the serde repr must not drift apart — they are two
    /// spellings of one fact, which is the defect this crate exists to remove.
    #[test]
    fn as_str_agrees_with_serde() {
        for lang in Language::ALL {
            let encoded = serde_json::to_string(&lang).expect("serialize");
            assert_eq!(encoded, format!("\"{}\"", lang.as_str()));
        }
    }

    #[test]
    fn parse_accepts_the_aliases_a_cli_must_accept() {
        assert_eq!(Language::parse("c++").unwrap(), Language::Cpp);
        assert_eq!(Language::parse("cxx").unwrap(), Language::Cpp);
        assert_eq!(Language::parse("cpp").unwrap(), Language::Cpp);
        assert_eq!(Language::parse("rust").unwrap(), Language::Rust);
        assert_eq!(Language::parse("c").unwrap(), Language::C);
    }

    /// The cmake `LANGUAGE` keyword set, pinned. It is NOT `parse`'s set, and
    /// the difference is the point: `rs` is a keyword there and not a CLI
    /// flag value, `c++` is a CLI flag value and a FATAL_ERROR there. Both
    /// halves are checked, so unifying them by accident fails here rather
    /// than changing what a user may type.
    ///
    /// The authority for this list is
    /// `cmake/NanoRosNodeRegister.cmake`'s `LANGUAGE` validator.
    #[test]
    fn the_cmake_keyword_set_is_not_the_cli_flag_set() {
        for (token, lang) in [
            ("c", Language::C),
            ("cpp", Language::Cpp),
            ("cxx", Language::Cpp),
            ("rust", Language::Rust),
            ("rs", Language::Rust),
        ] {
            assert_eq!(Language::parse_cmake_keyword(token), Some(lang), "{token}");
            // cmake uppercases before comparing, so the keyword is one token
            // however the declaration spells it.
            let upper: String = token.chars().flat_map(char::to_uppercase).collect();
            assert_eq!(Language::parse_cmake_keyword(&upper), Some(lang), "{upper}");
        }
        // The two sets differ, in both directions.
        assert_eq!(Language::parse_cmake_keyword("c++"), None);
        assert_eq!(Language::parse("c++").unwrap(), Language::Cpp);
        assert_eq!(Language::parse_cmake_keyword("rs"), Some(Language::Rust));
        assert!(Language::parse("rs").is_err());
        // And neither invents a language.
        assert_eq!(Language::parse_cmake_keyword("python"), None);
    }

    /// Every cmake keyword names a language that is in `ALL`, so a variant
    /// added without a keyword is visible rather than merely unreachable.
    #[test]
    fn every_cmake_keyword_lands_in_all() {
        for (kw, lang) in CMAKE_KEYWORDS {
            assert!(Language::ALL.contains(lang), "`{kw}` names {lang:?}");
        }
        for lang in Language::ALL {
            assert!(
                CMAKE_KEYWORDS.iter().any(|(_, l)| *l == lang),
                "{lang:?} has no cmake LANGUAGE keyword — a component declared \
                 in cmake could not state it"
            );
        }
    }

    /// The error names what was passed. A parse failure whose message does not
    /// quote the input makes a typo in a build script unreadable.
    #[test]
    fn parse_rejects_and_names_the_input() {
        let err = Language::parse("zig").expect_err("not a language yet");
        assert_eq!(err.input, "zig");
        assert!(err.to_string().contains("zig"), "{err}");
    }

    /// The table decides what the tree actually passes, in the casing the
    /// compilers care about. `.C` is C++ and `.c` is C: folding case first
    /// would make one of them wrong, silently, in a file list.
    #[test]
    fn deciding_extensions_decide() {
        for (path, lang) in [
            ("src/main.c", Language::C),
            ("src/main.cpp", Language::Cpp),
            ("src/main.cxx", Language::Cpp),
            ("src/main.cc", Language::Cpp),
            ("src/main.C", Language::Cpp),
            ("src/lib.rs", Language::Rust),
            ("/abs/CAPS.CPP", Language::Cpp),
        ] {
            assert_eq!(
                Language::of_source(path).expect(path),
                SourceVerdict::Decides(lang),
                "{path}"
            );
        }
    }

    /// A header is not a translation unit and a `.h` does not know its own
    /// language. Abstaining is what lets the refusal below mean something.
    #[test]
    fn headers_and_genexes_abstain() {
        for path in [
            "include/x.h",
            "include/x.hpp",
            "boot.s",
            "$<TARGET_OBJECTS:o>",
        ] {
            assert_eq!(
                Language::of_source(path).expect(path),
                SourceVerdict::Abstains,
                "{path}"
            );
        }
    }

    /// The refusal direction. Each of these was silently a language before:
    /// `Cargo.toml` read as C++ in `nano_ros_node_register` (anything not
    /// `.c`) and as C in `nano_ros_add_node` (anything without a C++
    /// extension). The message must name the path, because a build script's
    /// list is usually variable-expanded.
    #[test]
    fn an_unknown_spelling_is_refused_and_named() {
        for path in ["Cargo.toml", "src/main.zig", "Makefile", "src/weird."] {
            let err = Language::of_source(path).expect_err(path);
            assert_eq!(err.path, path);
            assert!(err.to_string().contains(path), "{err}");
        }
    }

    /// A dot in a DIRECTORY name is not an extension.
    #[test]
    fn only_the_last_component_carries_the_extension() {
        assert_eq!(
            Language::of_source("build-1.2/main.c").expect("path"),
            SourceVerdict::Decides(Language::C)
        );
        assert!(Language::of_source("build-1.2/Makefile").is_err());
    }

    /// The fold: C++ wins over C (the linker's rule, not a tie-break), an
    /// all-abstaining or empty list settles nothing, and `None` stays distinct
    /// from `C` — that collapse is issue 1062.
    #[test]
    fn the_fold_lets_cpp_win_and_keeps_unresolved_distinct() {
        let empty: [&str; 0] = [];
        assert_eq!(Language::of_sources(empty).expect("empty"), None);
        assert_eq!(
            Language::of_sources(["a.h", "b.hpp"]).expect("headers"),
            None
        );
        assert_eq!(Language::of_sources(["a.c"]).expect("c"), Some(Language::C));
        assert_eq!(
            Language::of_sources(["a.c", "b.cpp"]).expect("mixed c family"),
            Some(Language::Cpp)
        );
        assert_eq!(
            Language::of_sources(["b.cpp", "a.c"]).expect("order must not matter"),
            Some(Language::Cpp)
        );
        assert_eq!(
            Language::of_sources(["lib.rs"]).expect("rust"),
            Some(Language::Rust)
        );
    }

    /// Rust beside a C-family TU is two link roots in one target, so it is
    /// refused rather than resolved to either.
    #[test]
    fn rust_beside_c_family_is_refused() {
        let err = Language::of_sources(["lib.rs", "shim.c"]).expect_err("two link roots");
        assert_eq!(err, SourceLanguageError::Mixed(Language::C));
        assert!(err.to_string().contains("rust"), "{err}");
    }

    /// Every deciding extension folds to a `Language` that is in `ALL`, and no
    /// extension is in both tables — a spelling that both decides and abstains
    /// would make the answer depend on lookup order.
    #[test]
    fn the_two_extension_tables_are_disjoint_and_land_in_all() {
        for (ext, lang) in DECIDING {
            assert!(Language::ALL.contains(lang), ".{ext} names {lang:?}");
            assert!(
                !ABSTAINING.contains(ext),
                ".{ext} is in both tables; the answer would depend on lookup order"
            );
        }
    }

    /// `ALL` must stay exhaustive. A variant added without extending it makes
    /// every consumer that iterates silently skip the new language — the exact
    /// failure mode (a language invisible to one reader) this crate removes.
    #[test]
    fn all_is_exhaustive() {
        for lang in Language::ALL {
            // Exhaustive match: adding a variant fails to compile here first.
            let named = match lang {
                Language::Rust => "rust",
                Language::C => "c",
                Language::Cpp => "cpp",
            };
            assert_eq!(named, lang.as_str());
        }
        assert_eq!(Language::ALL.len(), 3);
    }
}
