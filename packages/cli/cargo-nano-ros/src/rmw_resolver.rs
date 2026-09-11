//! RMW selection lowering (Phase 227.2, design-of-record RFC-0031).
//!
//! RMW is a declared, language-agnostic value (`system.toml [system].rmw` /
//! `[deploy.<t>].rmw` / CLI flag). This module is the single place that
//! validates a declared RMW string and **lowers** it to each language's build
//! mechanism — a Rust cargo feature and a CMake `-DNANO_ROS_RMW` value — plus the
//! C `#define` token the bake emits. The cargo feature / CMake var are lowering
//! targets, not the user-facing knob.
//!
//! Phase 248 C5b (RFC-0031 amendment) — the Rust lowering target is the **board
//! crate's** `rmw-X` feature (the board self-links + registers the concrete
//! backend), NOT an `nros/rmw-X` feature. `cargo_feature` therefore names the
//! `rmw-X` feature codegen places on the entry's board dep.
//!
//! The umbrella is SELECTION-agnostic, which issue 1295 showed is not the same
//! as RMW-agnostic: `nros` declares `rmw-cyclonedds`, a marker forwarding
//! `nros-node/needs-type-descriptors`, and an image that links Cyclone without
//! it cannot create a publisher. So one `cargo_feature` name is offered to BOTH
//! crates and each takes it only if it declares it (`facade.rs`).
//!
//! It lives in `cargo-nano-ros` (the lower crate) so both the scaffolder here
//! and the orchestration loader in `nros-cli-core` share one mapping.

use std::{fmt, path::PathBuf};

use crate::{
    provider_scan::{ResolveError, ScanResult, resolve_unique},
    rmw_descriptor::{RMW_KIND, RmwDescriptor, agree_with_convention, parse_rmw_descriptor},
};

include!(concat!(env!("OUT_DIR"), "/rmw_table.rs"));

/// The RMW backends this checkout provides, **derived from the descriptors**
/// (`packages/rmw/*/*/nros-rmw.toml`) rather than enumerated here.
///
/// phase-347 W3 — this was a hand-written `const &["zenoh", "xrce",
/// "cyclonedds"]`, and it was already wrong: `uorb` is a first-class
/// `NANO_ROS_RMW` value in `NanoRosFeatureSet.cmake` and in
/// `nros-cpp/CMakeLists.txt`, but absent here, so `nros_rmw_dispatch`
/// FATAL_ERROR'd on a backend the tree ships. A closed list stopped covering
/// the tree it governed; that is why it is no longer a list.
pub fn known_rmw() -> Vec<&'static str> {
    RMW_ROWS.iter().map(|r| r.declared).collect()
}

/// A declared RMW value lowered to its per-language build forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRmw {
    /// The canonical declared name, e.g. `"zenoh"`.
    pub declared: &'static str,
    /// The board-crate cargo feature codegen lowers the RMW to, e.g.
    /// `"rmw-zenoh"` (Phase 248 C5b: lands on the entry's board dep, not
    /// `nros`). Board crate and `nros` share the `rmw-X` naming.
    pub cargo_feature: &'static str,
    /// The `-DNANO_ROS_RMW` CMake value, e.g. `"zenoh"`.
    pub cmake_value: &'static str,
    /// The C `#define NROS_SYSTEM_RMW_<TOKEN>` token, e.g. `"ZENOH"`.
    pub c_define_token: &'static str,
    /// `[rmw].cpp_define` — the define `nros-cpp` puts on its INTERFACE.
    pub cpp_define: &'static str,
    /// The cmake target the backend's own `CMakeLists.txt` creates, or empty.
    pub cmake_target: &'static str,
    /// `[rmw.codegen].per_message` — a cmake command run per message type.
    pub per_message_hook: &'static str,
    /// Phase 241 W13/R1 (RFC-0042 §D3 bullet 2) — the **link dispatch** data,
    /// the one place that records how each backend reaches the final link.
    /// Consumed by both the W11 synthesized `nros_ws_runtime` crate's nros-cpp
    /// feature and the cmake link extras (formerly hand-maintained prose).
    pub dispatch: RmwDispatch,
}

/// How a backend reaches the final binary's link (Phase 241 W13/R1). One SSoT for
/// the per-backend link requirement that cmake + the synthesized runtime crate
/// both consumed via duplicated conditionals before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RmwDispatch {
    /// The C/C++ umbrella (`nros-c`/`nros-cpp`) cffi feature that bundles +
    /// force-links this backend, e.g. `"rmw-zenoh-cffi"`. The W11 synthesized
    /// `nros_ws_runtime` crate sets this on its `nros-cpp` dep.
    pub umbrella_cffi_feature: &'static str,
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
    /// The `nros-c` feature that bundles this backend, or empty.
    ///
    /// AUTHORED per backend rather than derived: the two in-tree values are
    /// `cffi-zenoh-cffi` and `cffi-xrce-c`, irregular by history in the same way
    /// `cpp_define` is. `umbrella_cffi_feature` is the `nros-cpp` half, which IS
    /// regular (`<cargo_feature>-cffi`).
    pub c_cffi_feature: &'static str,
    /// Whether the final link must use the C++ linker driver (libstdc++ on the
    /// line). True for cyclonedds (its wrapper is C++), even for C binaries.
    pub needs_cxx_linker: bool,
}

/// A declared RMW value no descriptor claims (see [`known_rmw`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRmw {
    pub declared: String,
}

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

impl std::error::Error for UnknownRmw {}

/// The canonical backend name (`"zenoh"` / `"xrce"` / `"cyclonedds"`) from any
/// accepted spelling — the bare name, the `rmw-<x>` feature spelling, or the
/// legacy `rmw-<x>-cffi`. `None` for empty / unknown.
///
/// This is the single alias table; the orchestration layer's `normalize_rmw`
/// delegates here so there is one source of truth for RMW name recognition.
/// Does `backend`'s descriptor declare `capability`?
///
/// phase-347 W4 — the inverse of the old `Capability::backends_supporting`
/// list. The backend declares what it offers in `[rmw.capabilities]`; nothing
/// central enumerates the pairing, so a third-party backend can offer a
/// capability nano-ros has never heard of.
pub fn backend_declares_capability(backend: &str, capability: &str) -> bool {
    RMW_ROWS
        .iter()
        .find(|r| r.names.contains(&backend))
        .is_some_and(|r| r.capabilities.iter().any(|(k, _)| *k == capability))
}

/// Every backend whose descriptor declares `capability`, canonical names.
///
/// phase-347 W4 — replaces `Capability::backends_supporting`, whose only
/// consumer was a diagnostic listing "which backends carry this". Derived, so
/// the message cannot go stale the way the list did.
pub fn backends_declaring(capability: &str) -> Vec<&'static str> {
    RMW_ROWS
        .iter()
        .filter(|r| r.capabilities.iter().any(|(k, _)| *k == capability))
        .map(|r| r.declared)
        .collect()
}

/// The backend's own feature implementing `capability`, if it declares one.
pub fn backend_capability_feature(backend: &str, capability: &str) -> Option<&'static str> {
    RMW_ROWS
        .iter()
        .find(|r| r.names.contains(&backend))
        .and_then(|r| r.capabilities.iter().find(|(k, _)| *k == capability))
        .map(|(_, v)| *v)
}

pub fn canonical_rmw(input: &str) -> Option<&'static str> {
    RMW_ROWS
        .iter()
        .find(|r| r.names.contains(&input))
        .map(|r| r.declared)
}

/// Lower a declared RMW string to its per-language build forms.
///
/// Accepts the canonical name or any alias ([`canonical_rmw`]). An unknown
/// value is an error — caught early in the loader so a typo fails with a clear
/// message rather than producing a broken build.
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
            dispatch: RmwDispatch {
                umbrella_cffi_feature: r.cffi_feature,
                rlib_dep: (!r.rlib_dep.is_empty()).then_some(r.rlib_dep),
                link_strategy: r.link_strategy,
                c_cffi_feature: r.c_cffi_feature,
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
    // string the consumer typed: a provider announcing `zenoh`, `rmw-zenoh` and
    // `rmw-zenoh-cffi` has one canonical spelling, and the lowering must not
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
    let cffi_feature = crate::derived_descriptor::cffi_feature(&cargo_feature);
    for (field, stated, derived) in [
        ("cargo_feature", &d.stated_cargo_feature, &cargo_feature),
        ("cmake_value", &d.stated_cmake_value, &cmake_value),
        ("c_define_token", &d.stated_c_define_token, &c_define_token),
        ("cffi_feature", &d.stated_cffi_feature, &cffi_feature),
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

    Ok(ResolvedRmwIn {
        declared: canonical,
        cargo_feature,
        cmake_value,
        c_define_token,
        cffi_feature,
        package: pkg.package.clone(),
        package_dir: pkg.dir.clone(),
        cmake_dir,
        descriptor: d,
    })
}

/// A declared RMW value lowered against a provider scan ([`resolve_rmw_in`]).
///
/// Everything [`ResolvedRmw`] carries plus WHERE the provider is — which is the
/// half the compile-time table cannot have, and the half a `add_subdirectory()`
/// needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRmwIn {
    /// The provider's first `rmw` announcement — its canonical name.
    pub declared: String,
    pub cargo_feature: String,
    pub cmake_value: String,
    pub c_define_token: String,
    pub cffi_feature: String,
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
/// not one per announcement: `zenoh`, `rmw-zenoh` and `rmw-zenoh-cffi` are
/// three ways to reach one backend, and listing all three in the cache
/// drop-down would offer a user three choices that are one choice.
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
        assert_eq!(canonical_rmw("rmw-zenoh-cffi"), Some("zenoh"));
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
            assert_eq!(
                scanned.cffi_feature, baked.dispatch.umbrella_cffi_feature,
                "{name}: cffi"
            );
            let d = &scanned.descriptor;
            assert_eq!(d.cpp_define, baked.cpp_define, "{name}: cpp_define");
            assert_eq!(
                d.per_message_hook, baked.per_message_hook,
                "{name}: per_message_hook"
            );
            assert_eq!(
                d.link_strategy, baked.dispatch.link_strategy,
                "{name}: link_strategy"
            );
            assert_eq!(
                d.c_cffi_feature, baked.dispatch.c_cffi_feature,
                "{name}: c_cffi_feature"
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
