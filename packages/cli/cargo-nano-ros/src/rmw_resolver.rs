//! RMW selection lowering (Phase 227.2, design-of-record RFC-0031).
//!
//! RMW is a declared, language-agnostic value (`system.toml [system].rmw` /
//! `[deploy.<t>].rmw` / CLI flag). This module is the single place that
//! validates a declared RMW string and **lowers** it to each language's build
//! mechanism — a Rust cargo feature and a CMake `-DNROS_RMW` value — plus the
//! C `#define` token the bake emits. The cargo feature / CMake var are lowering
//! targets, not the user-facing knob.
//!
//! Phase 248 C5b (RFC-0031 amendment) — the Rust lowering target is the **board
//! crate's** `rmw-X` feature (the board self-links + registers the concrete
//! backend), NOT an `nros/rmw-X` feature. `cargo_feature` therefore names the
//! `rmw-X` feature codegen places on the entry's board dep.
//!
//! The umbrella is SELECTION-agnostic, which issue 1295 showed is not the same
//! as needing nothing: an image that links Cyclone without `nros`'s
//! `needs-type-descriptors` cannot create a publisher. The facade turns that on
//! for a backend whose descriptor names a `type_register_crate` (`facade.rs`);
//! `nros` itself names the capability, never a backend (RFC-0071).
//!
//! It lives in `cargo-nano-ros` (the lower crate) so both the scaffolder here
//! and the orchestration loader in `nros-cli-core` share one mapping.
//!
//! phase-545 W1 (RFC-0071 D14) — the mapping is the PROVIDER SCAN's. Every CLI
//! site asks an [`RmwIndex`], which memoises [`resolve_rmw_in`] over one scan;
//! the table `build.rs` bakes from `packages/rmw/` is a compile-time check and
//! this module's test oracle, never an answer.

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    provider_scan::{
        InTreeScope, ResolveError, ScanResult, SearchPath, resolve_unique, scan_search_path,
        search_path_over_scoped,
    },
    rmw_descriptor::{RMW_KIND, RmwDescriptor, agree_with_convention, parse_rmw_descriptor},
};

// phase-545 W1 (RFC-0071 D14) — the baked table is no longer an ANSWER
// anywhere outside this module's tests. `build.rs` still generates it, which
// is what checks the in-tree descriptors' convention fields at compile time
// (RFC-0087 D4), and `the_baked_table_and_the_scan_agree_on_every_in_tree_backend`
// holds it equal to the scan. Every CLI site asks an [`RmwIndex`].
#[cfg(test)]
mod baked {
    include!(concat!(env!("OUT_DIR"), "/rmw_table.rs"));
}
#[cfg(test)]
use baked::RMW_ROWS;

/// The RMW backends this checkout provides, **derived from the descriptors**
/// (`packages/rmw/*/*/nros-rmw.toml`) rather than enumerated here.
///
/// phase-347 W3 — this was a hand-written `const &["zenoh", "xrce",
/// "cyclonedds"]`, and it was already wrong: `uorb` is a first-class
/// `NROS_RMW` value in `NanoRosFeatureSet.cmake` and in
/// `nros-cpp/CMakeLists.txt`, but absent here, so `nros_rmw_dispatch`
/// FATAL_ERROR'd on a backend the tree ships. A closed list stopped covering
/// the tree it governed; that is why it is no longer a list.
#[cfg(test)]
pub fn known_rmw() -> Vec<&'static str> {
    RMW_ROWS.iter().map(|r| r.declared).collect()
}

/// A declared RMW value lowered to its per-language build forms.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRmw {
    /// The canonical declared name, e.g. `"zenoh"`.
    pub declared: &'static str,
    /// The board-crate cargo feature codegen lowers the RMW to, e.g.
    /// `"rmw-zenoh"` (Phase 248 C5b: lands on the entry's board dep, not
    /// `nros`). Board crate and `nros` share the `rmw-X` naming.
    pub cargo_feature: &'static str,
    /// The `-DNROS_RMW` CMake value, e.g. `"zenoh"`.
    pub cmake_value: &'static str,
    /// The C `#define NROS_SYSTEM_RMW_<TOKEN>` token, e.g. `"ZENOH"`.
    pub c_define_token: &'static str,
    /// `[rmw].cpp_define` — the define `nros-cpp` puts on its INTERFACE.
    pub cpp_define: &'static str,
    /// The cmake target the backend's own `CMakeLists.txt` creates, or empty.
    pub cmake_target: &'static str,
    /// `[rmw.codegen].per_message` — a cmake command run per message type.
    pub per_message_hook: &'static str,
    /// `[rmw.provides.cargo] crate`, as a Rust path root (`-` -> `_`): the crate
    /// whose `register_fn` an image calls (RFC-0071 D11). Empty: the backend
    /// registers through the C ABI only.
    pub register_crate: &'static str,
    /// `[rmw.provides.cargo] register`, the function inside `register_crate`.
    pub register_fn: &'static str,
    /// `[rmw.provides.cargo] type_register_crate`: where `register::<T>()`
    /// lives, for a backend that needs per-type descriptors. Empty otherwise.
    pub type_register_crate: &'static str,
    /// `[rmw.config]` — the backend's own configuration file (RFC-0109 D8).
    pub config: RmwConfigFile,
    /// Phase 241 W13/R1 (RFC-0042 §D3 bullet 2) — the **link dispatch** data,
    /// the one place that records how each backend reaches the final link.
    /// Consumed by both the W11 synthesized `nros_ws_runtime` crate's nros-cpp
    /// feature and the cmake link extras (formerly hand-maintained prose).
    pub dispatch: RmwDispatch,
}

/// A backend's own configuration file, as its descriptor declares it
/// (`[rmw.config]`, RFC-0109 D8). All empty for a backend with no file format.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RmwConfigFile {
    /// The file, relative to a `system.toml`.
    pub file: &'static str,
    /// One of `rmw_descriptor::CONFIG_FORMATS`.
    pub format: &'static str,
    /// A retired file sync refuses naming `file`, or empty.
    pub retired: &'static str,
}

/// How a backend reaches the final binary's link (Phase 241 W13/R1). One SSoT for
/// the per-backend link requirement that cmake + the synthesized runtime crate
/// both consumed via duplicated conditionals before.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RmwDispatch {
    /// The pure-Rust backend crate force-linked **into** the umbrella as an rlib
    /// dep, e.g. `Some("nros-rmw-zenoh")`. `None` for a backend that is not a
    /// Rust crate — cyclonedds and uorb are C/C++ CMake projects.
    ///
    /// phase-439 W4 — this used to be read by NOTHING (issue 1216) and is now
    /// the discriminator: naming an rlib IS the statement that the backend
    /// arrives inside the umbrella staticlib and nothing separate reaches the
    /// link line. See [`RmwDispatch::link_strategy`].
    pub rlib_dep: Option<&'static str>,
    /// `umbrella` | `cmake` — see `rmw_descriptor::LINK_STRATEGIES`.
    pub link_strategy: &'static str,
    /// Whether the final link must use the C++ linker driver (libstdc++ on the
    /// line). True for cyclonedds (its wrapper is C++), even for C binaries.
    pub needs_cxx_linker: bool,
}

/// A declared RMW value no descriptor claims (see [`known_rmw`]).
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRmw {
    pub declared: String,
}

#[cfg(test)]
impl fmt::Display for UnknownRmw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown rmw `{}` (known: {})",
            self.declared,
            known_rmw().join(", ")
        )
    }
}

#[cfg(test)]
impl std::error::Error for UnknownRmw {}

// phase-545 W1 (RFC-0071 D14) — `backend_declares_capability`,
// `backend_states_fact`, `backends_declaring` and `backend_capability_feature`
// read the baked table and were deleted: they are [`ResolvedRmwIn`]'s
// `declares_capability` / `states_fact` / `capability_feature` and
// [`RmwIndex::backends_declaring`], asked of the descriptor the scan resolved.

/// The canonical backend name from any accepted spelling, over the BAKED
/// table. Test-only (RFC-0071 D14): the CLI asks [`RmwIndex::canonical`].
#[cfg(test)]
pub fn canonical_rmw(input: &str) -> Option<&'static str> {
    RMW_ROWS
        .iter()
        .find(|r| r.names.contains(&input))
        .map(|r| r.declared)
}

/// Lower a declared RMW string over the BAKED table. Test-only (RFC-0071
/// D14): it is the regression net that holds the table equal to the scan, and
/// every CLI site asks an [`RmwIndex`] instead.
#[cfg(test)]
pub fn resolve_rmw(declared: &str) -> Result<ResolvedRmw, UnknownRmw> {
    RMW_ROWS
        .iter()
        .find(|r| r.names.contains(&declared))
        .map(|r| ResolvedRmw {
            declared: r.declared,
            cargo_feature: r.cargo_feature,
            cmake_value: r.cmake_value,
            c_define_token: r.c_define_token,
            cpp_define: r.cpp_define,
            cmake_target: r.cmake_target,
            per_message_hook: r.per_message_hook,
            register_crate: r.register_crate,
            register_fn: r.register_fn,
            type_register_crate: r.type_register_crate,
            config: RmwConfigFile {
                file: r.config_file,
                format: r.config_format,
                retired: r.config_retired,
            },
            dispatch: RmwDispatch {
                rlib_dep: (!r.rlib_dep.is_empty()).then_some(r.rlib_dep),
                link_strategy: r.link_strategy,
                needs_cxx_linker: r.needs_cxx_linker,
            },
        })
        .ok_or_else(|| UnknownRmw {
            declared: declared.to_string(),
        })
}

/// Lower a declared RMW string against a PROVIDER SCAN.
///
/// The sibling of [`resolve_rmw`], and the answer to issue 1214. `resolve_rmw`
/// reads a table baked into this binary at COMPILE time from `packages/rmw/`,
/// so the set of resolvable names is structurally the contents of one checkout:
/// an out-of-tree provider that `nros ws providers --resolve rmw:acme` FINDS is
/// then reported unknown by every build path. This reads the descriptor at
/// SELECTION time, through the same scan, so discovery and dispatch are one
/// mechanism.
///
/// Modelled on `serdes_resolver::resolve_serdes_in`, which is the shape RFC-0088
/// D6 established and RFC-0094 D5 extends to this axis. Owned `String`s for the
/// same reason it gives: an out-of-repo provider's name is read from a file at
/// selection time and cannot be `'static`.
///
/// A provider with NO `nros-rmw.toml` is an error here, unlike serdes: a serdes
/// provider with nothing non-derivable to say gets every default, while an RMW
/// backend must at minimum say how it reaches the link and what `cpp_define`
/// consumers `#if` on. Both are non-derivable, so silence is not a default.
pub fn resolve_rmw_in(scan: &ScanResult, declared: &str) -> Result<ResolvedRmwIn, RmwResolveError> {
    let resolution = resolve_unique(scan, RMW_KIND, declared).map_err(RmwResolveError::Scan)?;
    let pkg = resolution.winner;

    // The CANONICAL name is the provider's first `rmw` announcement, not the
    // string the consumer typed: a provider announcing `zenoh` and
    // `rmw-zenoh` has one canonical spelling, and the lowering must not
    // depend on which alias the consumer reached it by.
    let canonical = pkg
        .provides
        .iter()
        .find(|p| p.kind == RMW_KIND)
        .map(|p| p.name.clone())
        .ok_or_else(|| RmwResolveError::Provider {
            dir: pkg.dir.clone(),
            message: "resolved as an rmw provider but announces no rmw provision".to_string(),
        })?;

    let fail = |message: String| RmwResolveError::Provider {
        dir: pkg.dir.clone(),
        message,
    };

    let descriptor_path = pkg.descriptor_path(RMW_KIND);
    if !descriptor_path.is_file() {
        return Err(fail(format!(
            "no {} beside the announcement. An rmw provider must declare at least \
             `[rmw] cpp_define` and how it reaches the link — neither is derivable \
             from the name, so there is no default that could be right.",
            descriptor_path.file_name().map_or_else(
                || descriptor_path.display().to_string(),
                |f| f.to_string_lossy().into_owned()
            )
        )));
    }
    let text = std::fs::read_to_string(&descriptor_path)
        .map_err(|e| fail(format!("read {}: {e}", descriptor_path.display())))?;
    let origin = descriptor_path.display().to_string();
    let d = parse_rmw_descriptor(&text, &origin).map_err(fail)?;

    // The same convention half `build.rs` applies to an in-tree descriptor, and
    // the same refusal of a restatement that disagrees (RFC-0087 D4).
    let cargo_feature = crate::derived_descriptor::cargo_feature(RMW_KIND, &canonical);
    let cmake_value = crate::derived_descriptor::cmake_value(&canonical);
    let c_define_token = crate::derived_descriptor::c_define_token(&canonical);
    for (field, stated, derived) in [
        ("cargo_feature", &d.stated_cargo_feature, &cargo_feature),
        ("cmake_value", &d.stated_cmake_value, &cmake_value),
        ("c_define_token", &d.stated_c_define_token, &c_define_token),
    ] {
        agree_with_convention(&origin, field, stated, derived).map_err(fail)?;
    }
    if !d.stated_names.is_empty() {
        let announced: Vec<String> = pkg
            .provides
            .iter()
            .filter(|p| p.kind == RMW_KIND)
            .map(|p| p.name.clone())
            .collect();
        agree_with_convention(
            &origin,
            "names",
            &d.stated_names.join(","),
            &announced.join(","),
        )
        .map_err(fail)?;
    }
    if d.cpp_define.is_empty() {
        return Err(fail(format!(
            "{origin}: [rmw].cpp_define is missing — it is the one non-derivable \
             lowering (spellings are inconsistent across backends by history and \
             consumers `#if` on them), so it must be authored"
        )));
    }

    // The cmake project directory is descriptor-relative, so a provider may put
    // its `CMakeLists.txt` in a subdirectory. Normalised here rather than in
    // cmake: a `dir` of `"."` must come back as the package dir itself, not as
    // `<dir>/.`, because the value is compared against build paths.
    let cmake_dir = if d.cmake_dir == "." || d.cmake_dir.is_empty() {
        pkg.dir.clone()
    } else {
        pkg.dir.join(&d.cmake_dir)
    };

    let names: Vec<String> = pkg
        .provides
        .iter()
        .filter(|p| p.kind == RMW_KIND)
        .map(|p| p.name.clone())
        .collect();

    Ok(ResolvedRmwIn {
        declared: canonical,
        names,
        cargo_feature,
        cmake_value,
        c_define_token,
        package: pkg.package.clone(),
        package_dir: pkg.dir.clone(),
        cmake_dir,
        descriptor: d,
    })
}

/// A declared RMW value lowered against a provider scan ([`resolve_rmw_in`]).
///
/// Everything the baked table's row carries plus WHERE the provider is — which is the
/// half the compile-time table cannot have, and the half a `add_subdirectory()`
/// needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRmwIn {
    /// The provider's first `rmw` announcement — its canonical name.
    pub declared: String,
    /// Every `rmw` name the provider announces, the canonical one first.
    pub names: Vec<String>,
    pub cargo_feature: String,
    pub cmake_value: String,
    pub c_define_token: String,
    /// The `package.xml` `<name>`.
    pub package: String,
    /// The directory holding the provider's `package.xml`.
    pub package_dir: PathBuf,
    /// The cmake project to `add_subdirectory()`, resolved absolute.
    pub cmake_dir: PathBuf,
    /// Everything the descriptor itself declares.
    pub descriptor: RmwDescriptor,
}

/// Why [`resolve_rmw_in`] could not lower a name.
#[derive(Debug)]
pub enum RmwResolveError {
    /// No provider announced it, or several did ambiguously.
    Scan(ResolveError),
    /// A provider WAS resolved and its descriptor is unusable.
    Provider { dir: PathBuf, message: String },
}

impl fmt::Display for RmwResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scan(e) => write!(f, "{e}"),
            Self::Provider { dir, message } => {
                write!(f, "rmw provider at {}: {message}", dir.display())
            }
        }
    }
}

impl std::error::Error for RmwResolveError {}

/// Every rmw backend a provider on this scan announces, by CANONICAL name.
///
/// The scan-based sibling of [`known_rmw`], and the producer of the cmake
/// `NROS_RMW_KNOWN` list. One entry per provider — its FIRST `rmw` provision —
/// not one per announcement: `zenoh` and `rmw-zenoh` are two ways to reach
/// one backend, and listing both in the cache drop-down would offer a user
/// two choices that are one choice.
/// [`resolve_rmw_in`] still accepts every alias.
///
/// Sorted rather than in scan order so the list does not reorder when an
/// unrelated package is added — the same reason `nano_ros_load_providers` sorts
/// its kinds.
#[must_use]
pub fn known_rmw_in(scan: &ScanResult) -> Vec<String> {
    let mut names: Vec<String> = scan
        .providers
        .iter()
        .filter_map(|p| p.provides.iter().find(|x| x.kind == RMW_KIND))
        .map(|p| p.name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

impl ResolvedRmwIn {
    /// Does this backend's descriptor declare `capability` (`[rmw.capabilities]`)?
    ///
    /// phase-545 W1 (RFC-0071 D14) — this was `backend_declares_capability`
    /// over the baked table, so a capability an out-of-tree backend offered
    /// was invisible. It is now a question asked of the descriptor the scan
    /// resolved.
    #[must_use]
    pub fn declares_capability(&self, capability: &str) -> bool {
        self.descriptor
            .capabilities
            .iter()
            .any(|(k, _)| k == capability)
    }

    /// The backend's own feature implementing `capability`, if it declares one.
    #[must_use]
    pub fn capability_feature(&self, capability: &str) -> Option<&str> {
        self.descriptor
            .capabilities
            .iter()
            .find(|(k, _)| k == capability)
            .map(|(_, v)| v.as_str())
    }

    /// What the descriptor states for `fact` (`[rmw.facts]`, issue 2174):
    /// `None` when it says nothing. A check must treat `None` as UNKNOWN,
    /// never as false.
    #[must_use]
    pub fn states_fact(&self, fact: &str) -> Option<bool> {
        self.descriptor
            .facts
            .iter()
            .find(|(k, _)| k == fact)
            .map(|(_, v)| *v)
    }
}

/// One provider scan, and every RMW name resolved over it (RFC-0071 D14).
///
/// The one answer every CLI site asks. It memoises [`resolve_rmw_in`] by the
/// name asked, so a build that resolves the same backend at a dozen sites
/// reads its descriptor once. Built from a scan, never from the table
/// `build.rs` bakes: that table is the contents of one checkout, and a
/// workspace provider it lacks is exactly the case it answered wrongly
/// (issue 2295).
#[derive(Debug)]
pub struct RmwIndex {
    scan: ScanResult,
    roots: Vec<PathBuf>,
    memo: Mutex<HashMap<String, Result<Arc<ResolvedRmwIn>, RmwLookupError>>>,
}

impl RmwIndex {
    /// An index over `scan`, whose search roots were `roots` (named in a
    /// refusal, so a user can see where a missing provider was looked for).
    #[must_use]
    pub fn new(scan: ScanResult, roots: Vec<PathBuf>) -> Self {
        Self {
            scan,
            roots,
            memo: Mutex::new(HashMap::new()),
        }
    }

    /// The index for a caller with no workspace: the SDK root alone, its root
    /// narrowed to `packages/rmw/` ([`InTreeScope::Rmw`]), no environment.
    ///
    /// # Errors
    ///
    /// The root cannot be walked.
    pub fn over_sdk_root(root: &Path) -> eyre::Result<Self> {
        let path = search_path_over_scoped(
            Some(root),
            &SearchPath::default(),
            root,
            None,
            InTreeScope::Rmw,
        );
        let scan = scan_search_path(&path)?;
        Ok(Self::new(scan, path.paths()))
    }

    /// Resolve `declared` (any announced alias) to its provider's lowering.
    ///
    /// # Errors
    ///
    /// No provider announces it, several in one root do, or the winner's
    /// descriptor is unusable — D5's message, naming the search.
    pub fn resolve(&self, declared: &str) -> Result<Arc<ResolvedRmwIn>, RmwLookupError> {
        let mut memo = self.memo.lock().expect("rmw index memo");
        if let Some(hit) = memo.get(declared) {
            return hit.clone();
        }
        let answer = resolve_rmw_in(&self.scan, declared)
            .map(Arc::new)
            .map_err(|e| RmwLookupError::new(declared, &e, &self.roots));
        memo.insert(declared.to_string(), answer.clone());
        answer
    }

    /// The canonical name of `declared`, or `None` when nothing provides it.
    #[must_use]
    pub fn canonical(&self, declared: &str) -> Option<String> {
        self.resolve(declared).ok().map(|r| r.declared.clone())
    }

    /// Every backend announced on this search path, canonical names, sorted.
    #[must_use]
    pub fn known(&self) -> Vec<String> {
        known_rmw_in(&self.scan)
    }

    /// Every announced backend whose descriptor declares `capability`.
    ///
    /// phase-347 W4's derived diagnostic list, over the scan rather than the
    /// baked table. A backend whose descriptor cannot be read is left out.
    #[must_use]
    pub fn backends_declaring(&self, capability: &str) -> Vec<String> {
        self.known()
            .into_iter()
            .filter(|n| {
                self.resolve(n)
                    .is_ok_and(|r| r.declares_capability(capability))
            })
            .collect()
    }

    /// The scan this index answers from.
    #[must_use]
    pub fn scan(&self) -> &ScanResult {
        &self.scan
    }

    /// The search roots, in precedence order (later wins).
    #[must_use]
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
}

/// An RMW name an [`RmwIndex`] could not resolve, with the search it made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RmwLookupError {
    /// The name as asked.
    pub declared: String,
    /// Why, as [`RmwResolveError`] put it.
    pub reason: String,
    /// The roots searched, in order.
    pub searched: Vec<PathBuf>,
}

impl RmwLookupError {
    fn new(declared: &str, e: &RmwResolveError, roots: &[PathBuf]) -> Self {
        let reason = match e {
            RmwResolveError::Scan(ResolveError::NotFound { available, .. }) => {
                let announced = if available.is_empty() {
                    "none".to_string()
                } else {
                    available.join(", ")
                };
                format!("no package provides rmw `{declared}` (announced: {announced})")
            }
            other => other.to_string(),
        };
        Self {
            declared: declared.to_string(),
            reason,
            searched: roots.to_vec(),
        }
    }
}

impl fmt::Display for RmwLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let searched: Vec<String> = self
            .searched
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        write!(f, "{}; searched: {}", self.reason, searched.join(", "))
    }
}

impl std::error::Error for RmwLookupError {}

/// Fixtures for the sites' out-of-tree tests (phase-545 W1). Not API.
#[doc(hidden)]
pub mod test_support {
    use std::path::{Path, PathBuf};

    use super::RmwIndex;

    /// Write an out-of-tree rmw provider `acme` into `dir`: a `package.xml`
    /// announcing it, an `nros-rmw.toml`, and the cargo crate `acme-rmw` it
    /// names. Nothing in the nano-ros tree names it.
    pub fn write_acme_provider(dir: &Path) {
        std::fs::create_dir_all(dir.join("src")).expect("acme dir");
        std::fs::write(
            dir.join("package.xml"),
            "<?xml version=\"1.0\"?>\n<package format=\"3\">\n  <name>acme_rmw</name>\n  \
             <version>0.0.0</version>\n  <export>\n    \
             <nano_ros_provides kind=\"rmw\" name=\"acme\"/>\n  </export>\n</package>\n",
        )
        .expect("acme package.xml");
        std::fs::write(
            dir.join("nros-rmw.toml"),
            "[rmw]\ncpp_define = \"NROS_RMW_ACME\"\n\n\
             [rmw.provides.cargo]\ncrate = \"acme-rmw\"\nregister = \"register\"\n\n\
             [rmw.link]\nrlib_dep = \"acme-rmw\"\nedition_feature = \"ros-{edition}\"\n\n\
             [rmw.capabilities]\nsafety = \"acme-safety\"\n\n\
             [rmw.facts]\nrequest_source_timestamp = false\n\n\
             [rmw.provides.cargo.platform_features]\nplatform-posix = \"platform-posix\"\n\n\
             [rmw.config]\nfile = \"rmw/acme.json5\"\nformat = \"session-json5\"\n",
        )
        .expect("acme nros-rmw.toml");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"acme-rmw\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [features]\ndefault = []\nros-humble = []\nros-jazzy = []\n\
             acme-safety = []\nplatform-posix = []\n",
        )
        .expect("acme Cargo.toml");
        std::fs::write(dir.join("src/lib.rs"), "pub fn register() {}\n").expect("acme lib.rs");
    }

    /// This checkout's root.
    #[must_use]
    pub fn checkout() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("the checkout")
    }

    /// An index over this checkout's `packages/rmw/` and `workspace` — the
    /// shape `nros build` scans, with `acme` written into the workspace.
    #[must_use]
    pub fn acme_index(workspace: &Path) -> RmwIndex {
        write_acme_provider(&workspace.join("src/acme_rmw"));
        let roots = vec![checkout().join("packages/rmw"), workspace.to_path_buf()];
        let scan = crate::provider_scan::scan_roots(&roots).expect("scan");
        RmwIndex::new(scan, roots)
    }

    /// An index over this checkout's `packages/rmw/` alone.
    #[must_use]
    pub fn in_tree_index() -> RmwIndex {
        RmwIndex::over_sdk_root(&checkout()).expect("the checkout scans")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zenoh_lowers_to_each_language() {
        let r = resolve_rmw("zenoh").expect("zenoh is known");
        assert_eq!(r.cargo_feature, "rmw-zenoh");
        assert_eq!(r.cmake_value, "zenoh");
        assert_eq!(r.c_define_token, "ZENOH");
    }

    #[test]
    fn xrce_and_cyclonedds_lower_uniformly() {
        let x = resolve_rmw("xrce").unwrap();
        assert_eq!(
            (x.cargo_feature, x.cmake_value, x.c_define_token),
            ("rmw-xrce", "xrce", "XRCE")
        );
        let c = resolve_rmw("cyclonedds").unwrap();
        assert_eq!(
            (c.cargo_feature, c.cmake_value, c.c_define_token),
            ("rmw-cyclonedds", "cyclonedds", "CYCLONEDDS")
        );
    }

    #[test]
    fn every_known_rmw_resolves() {
        for name in known_rmw() {
            assert!(resolve_rmw(name).is_ok(), "{name} should resolve");
        }
    }

    #[test]
    fn unknown_rmw_is_rejected_with_known_list() {
        let err = resolve_rmw("dust-dds").unwrap_err();
        assert_eq!(err.declared, "dust-dds");
        let msg = err.to_string();
        assert!(msg.contains("dust-dds"));
        assert!(msg.contains("zenoh") && msg.contains("xrce") && msg.contains("cyclonedds"));
    }

    #[test]
    fn canonical_rmw_accepts_aliases() {
        assert_eq!(canonical_rmw("zenoh"), Some("zenoh"));
        assert_eq!(canonical_rmw("rmw-zenoh"), Some("zenoh"));
        // phase-506 W4d — the `-cffi` spelling named an API-crate feature that
        // no longer exists, and no provider announces it any more.
        assert_eq!(canonical_rmw("rmw-zenoh-cffi"), None);
        assert_eq!(canonical_rmw("rmw-cyclonedds"), Some("cyclonedds"));
        assert_eq!(canonical_rmw("nope"), None);
        // resolve_rmw accepts aliases via canonical_rmw.
        assert_eq!(resolve_rmw("rmw-xrce").unwrap().cargo_feature, "rmw-xrce");
    }

    #[test]
    fn cargo_feature_matches_board_rmw_naming() {
        // Phase 248 C5b — the cargo lowering target is the board crate's
        // `rmw-X` feature; guards against drift from the board crates' (and
        // nros's) `rmw-<name>` feature naming.
        for name in known_rmw() {
            let r = resolve_rmw(name).unwrap();
            assert_eq!(r.cargo_feature, format!("rmw-{name}"));
        }
    }

    /// The repo root, from this crate's manifest dir.
    fn repo_root() -> std::path::PathBuf {
        // CARGO_MANIFEST_DIR = packages/cli/cargo-nano-ros → repo root is ../../..
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("the repo root exists relative to this manifest")
    }

    /// The compile-time table and the scan-time read must agree, field for
    /// field, for every backend in this checkout.
    ///
    /// Two readers of one file format that nobody compares is how the RMW
    /// parity map came to disagree with the vtable by 25 symbols, and it is the
    /// exact risk `resolve_rmw_in` introduces: `build.rs` bakes `RMW_ROWS` from
    /// `packages/rmw/`, `resolve_rmw_in` reads the same descriptors through the
    /// provider scan, and nothing else would notice them drifting. They share
    /// `rmw_descriptor::parse_rmw_descriptor`, so this asserts the DERIVATIONS
    /// around it agree too.
    #[test]
    fn the_baked_table_and_the_scan_agree_on_every_in_tree_backend() {
        let root = repo_root();
        let scan = crate::provider_scan::scan_roots(&[root]).expect("the nano-ros tree scans");
        for name in known_rmw() {
            let baked = resolve_rmw(name).expect("a baked name resolves");
            let scanned = resolve_rmw_in(&scan, name)
                .unwrap_or_else(|e| panic!("{name} resolves through the scan: {e}"));
            assert_eq!(scanned.declared, baked.declared, "{name}: declared");
            assert_eq!(
                scanned.cargo_feature, baked.cargo_feature,
                "{name}: cargo_feature"
            );
            assert_eq!(
                scanned.cmake_value, baked.cmake_value,
                "{name}: cmake_value"
            );
            assert_eq!(
                scanned.c_define_token, baked.c_define_token,
                "{name}: c_define_token"
            );
            let d = &scanned.descriptor;
            assert_eq!(d.cpp_define, baked.cpp_define, "{name}: cpp_define");
            assert_eq!(
                d.per_message_hook, baked.per_message_hook,
                "{name}: per_message_hook"
            );
            assert_eq!(d.config_file, baked.config.file, "{name}: config_file");
            assert_eq!(
                d.config_format, baked.config.format,
                "{name}: config_format"
            );
            assert_eq!(
                d.config_retired, baked.config.retired,
                "{name}: config_retired"
            );
            assert_eq!(
                d.link_strategy, baked.dispatch.link_strategy,
                "{name}: link_strategy"
            );
            assert_eq!(
                d.needs_cxx_linker, baked.dispatch.needs_cxx_linker,
                "{name}: cxx_linker"
            );
            assert_eq!(
                d.rlib_dep.as_str(),
                baked.dispatch.rlib_dep.unwrap_or(""),
                "{name}: rlib_dep"
            );
            // `cmake_target` is the one field whose SOURCE differs: the baked
            // row reads only the legacy top-level `[rmw].cmake_target` (uorb's),
            // while the scan also reads `[rmw.provides.cmake].target`. So the
            // scan is a SUPERSET, and the assertion is one-directional.
            if !baked.cmake_target.is_empty() {
                assert_eq!(d.cmake_target, baked.cmake_target, "{name}: cmake_target");
            }
        }
    }

    /// RFC-0071 D14 — the index answers an out-of-tree provider from the
    /// scan, memoises it, carries the capability helpers, and names the
    /// search when a name resolves to nothing.
    #[test]
    fn the_index_resolves_an_out_of_tree_provider_and_names_the_search() {
        let ws = tempfile::tempdir().unwrap();
        let idx = test_support::acme_index(ws.path());
        let acme = idx.resolve("acme").expect("acme resolves through the scan");
        assert_eq!(acme.package, "acme_rmw");
        assert_eq!(acme.package_dir, ws.path().join("src/acme_rmw"));
        assert_eq!(acme.descriptor.cargo_crate, "acme-rmw");
        assert!(acme.declares_capability("safety"));
        assert_eq!(acme.capability_feature("safety"), Some("acme-safety"));
        assert_eq!(acme.states_fact("request_source_timestamp"), Some(false));
        assert!(
            Arc::ptr_eq(&acme, &idx.resolve("acme").unwrap()),
            "memoised"
        );
        assert_eq!(idx.canonical("rmw-zenoh").as_deref(), Some("zenoh"));
        assert!(idx.known().iter().any(|n| n == "acme"));
        let safety = idx.backends_declaring("safety");
        assert!(safety.iter().any(|n| n == "acme") && safety.iter().any(|n| n == "zenoh"));
        let e = idx.resolve("dust-dds").unwrap_err().to_string();
        assert!(e.contains("no package provides rmw `dust-dds`"), "{e}");
        assert!(e.contains("searched:") && e.contains("packages/rmw"), "{e}");
    }

    /// The helpers that moved onto [`ResolvedRmwIn`] answer what the baked
    /// table's did, for every in-tree backend.
    #[test]
    fn the_moved_helpers_agree_with_the_baked_rows() {
        let idx = test_support::in_tree_index();
        for row in RMW_ROWS {
            for alias in row.names {
                let r = idx.resolve(alias).expect("an in-tree alias resolves");
                assert_eq!(r.declared, row.declared, "{alias}");
            }
            let r = idx.resolve(row.declared).unwrap();
            for (cap, feat) in row.capabilities {
                assert!(r.declares_capability(cap), "{}: {cap}", row.declared);
                assert_eq!(r.capability_feature(cap), Some(*feat));
            }
            assert_eq!(
                r.descriptor.capabilities.len(),
                row.capabilities.len(),
                "{}",
                row.declared
            );
            for (fact, v) in row.facts {
                assert_eq!(r.states_fact(fact), Some(*v), "{}: {fact}", row.declared);
            }
            assert_eq!(r.states_fact("not-a-fact"), None);
        }
    }

    /// Every backend a provider announces must be dispatchable, not merely
    /// discoverable — issue 1214's exact shape, as a test.
    #[test]
    fn every_announced_backend_resolves_through_the_scan() {
        let root = repo_root();
        let scan = crate::provider_scan::scan_roots(&[root]).expect("the nano-ros tree scans");
        let announced = known_rmw_in(&scan);
        assert!(
            announced.iter().any(|n| n == "uorb"),
            "uorb announces `kind=\"rmw\"` and must be in the scan's answer: {announced:?}"
        );
        for name in &announced {
            resolve_rmw_in(&scan, name)
                .unwrap_or_else(|e| panic!("{name} is announced but does not dispatch: {e}"));
        }
    }
}
