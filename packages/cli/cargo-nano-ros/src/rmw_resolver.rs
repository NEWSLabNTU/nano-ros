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
//! `rmw-X` feature codegen places on the entry's board dep; the `nros` umbrella
//! stays RMW-agnostic.
//!
//! It lives in `cargo-nano-ros` (the lower crate) so both the scaffolder here
//! and the orchestration loader in `nros-cli-core` share one mapping.

use std::fmt;

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
    /// dep, e.g. `Some("nros-rmw-zenoh")`. `None` for cyclonedds — it is a C++
    /// library linked separately (see `extra_link_libs`), not a Rust rlib.
    pub rlib_dep: Option<&'static str>,
    /// Extra non-umbrella link libraries the final binary needs for this backend.
    /// Empty for the pure-Rust backends (zenoh/xrce); cyclonedds pulls its C++
    /// RMW wrapper + Cyclone + the C++ runtime (incl. for C binaries — the locked
    /// design choice). Names are cmake link targets / `-l` stems.
    pub extra_link_libs: &'static [&'static str],
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
            dispatch: RmwDispatch {
                umbrella_cffi_feature: r.cffi_feature,
                rlib_dep: (!r.rlib_dep.is_empty()).then_some(r.rlib_dep),
                extra_link_libs: r.extra_link_libs,
                needs_cxx_linker: r.needs_cxx_linker,
            },
        })
        .ok_or_else(|| UnknownRmw {
            declared: declared.to_string(),
        })
}

/// Phase 241 W13/R1 — render the per-backend link dispatch ([`RmwDispatch`]) as a
/// CMake-includable `nros_rmw_dispatch(<rmw>)` function, the **generated** form of the
/// formerly hand-maintained "RMW backend dispatch" prose (RFC-0042 §D3 bullet 2). The
/// output is committed at `cmake/NanoRosRmwDispatch.cmake`; `rmw_cmake_dispatch_is_current`
/// asserts the committed copy matches this renderer, so the SSoT is `resolve_rmw()` and
/// drift fails the build. Consumed by `NanoRosRuntimeCrate.cmake` (the W11 synthesized
/// runtime crate's nros-cpp cffi feature) and the cmake Cyclone link block.
pub fn render_cmake_dispatch() -> String {
    let mut out = String::new();
    out.push_str(
        "# Generated from cargo-nano-ros `resolve_rmw()` — DO NOT EDIT.\n\
         # Regenerate: `cargo test -p cargo-nano-ros rmw_cmake_dispatch_is_current -- --ignored`\n\
         # (or run the bin helper). The SSoT is rmw_resolver.rs; this is its CMake lowering.\n\
         #\n\
         # nros_rmw_dispatch(<rmw>) sets in the CALLER scope:\n\
         #   NROS_RMW_UMBRELLA_CFFI_FEATURE  the nros-c/nros-cpp cffi feature (e.g. rmw-zenoh-cffi)\n\
         #   NROS_RMW_RLIB_DEP               backend rlib crate bundled in the umbrella, or \"\"\n\
         #   NROS_RMW_EXTRA_LINK_LIBS        ;-list of extra link libs (cyclonedds C++ path), or \"\"\n\
         #   NROS_RMW_NEEDS_CXX_LINKER       ON/OFF — force the C++ linker driver (libstdc++)\n\
         #   NROS_RMW_CPP_DEFINE             the define nros-cpp puts on its INTERFACE\n\
         #   NROS_RMW_CMAKE_TARGET           a cmake target to link when present, or \"\"\n\
         #   NROS_RMW_CAPABILITIES           ;-list of capabilities this backend declares\n\
         function(nros_rmw_dispatch rmw)\n",
    );
    let mut first = true;
    for name in known_rmw() {
        let row = RMW_ROWS
            .iter()
            .find(|x| x.declared == name)
            .expect("known_rmw() names come from RMW_ROWS");
        let r = resolve_rmw(name).expect("a descriptor-provided name resolves");
        let d = &r.dispatch;
        let branch = if first { "if" } else { "elseif" };
        first = false;
        let rlib = d.rlib_dep.unwrap_or("");
        let extra = d.extra_link_libs.join(";");
        let needs_cxx = if d.needs_cxx_linker { "ON" } else { "OFF" };
        out.push_str(&format!(
            "    {branch}(rmw STREQUAL \"{decl}\")\n\
             \x20       set(NROS_RMW_UMBRELLA_CFFI_FEATURE \"{feat}\" PARENT_SCOPE)\n\
             \x20       set(NROS_RMW_RLIB_DEP \"{rlib}\" PARENT_SCOPE)\n\
             \x20       set(NROS_RMW_EXTRA_LINK_LIBS \"{extra}\" PARENT_SCOPE)\n\
             \x20       set(NROS_RMW_NEEDS_CXX_LINKER {needs_cxx} PARENT_SCOPE)\n\
             \x20       set(NROS_RMW_CPP_DEFINE \"{cpp_define}\" PARENT_SCOPE)\n\
             \x20       set(NROS_RMW_CMAKE_TARGET \"{cmake_target}\" PARENT_SCOPE)\n\
             \x20       set(NROS_RMW_CAPABILITIES \"{caps}\" PARENT_SCOPE)\n",
            decl = r.declared,
            feat = d.umbrella_cffi_feature,
            cpp_define = row.cpp_define,
            cmake_target = row.cmake_target,
            caps = row
                .capabilities
                .iter()
                .map(|(k, _)| *k)
                .collect::<Vec<_>>()
                .join(";"),
        ));
    }
    out.push_str(
        "    else()\n\
         \x20       message(FATAL_ERROR \"nros_rmw_dispatch: unknown rmw '${rmw}' \"\n\
         \x20           \"(known: KNOWN_PLACEHOLDER)\")\n\
         \x20   endif()\n\
         endfunction()\n",
    );
    // phase-347 W3 — the "known:" list in the FATAL_ERROR is DERIVED. It used to
    // be the literal "zenoh xrce cyclonedds", which is how the message came to
    // omit `uorb`: a hand-written list inside a generator is still a hand-written
    // list.
    out = out.replace("KNOWN_PLACEHOLDER", &known_rmw().join(" "));
    // phase-347 W3 — one derived list every cmake consumer shares, so
    // `NanoRosFeatureSet`'s validator and `nros-cpp`'s dispatch stop keeping
    // their own. Those two accepted `uorb` while `nros_rmw_dispatch` fatal'd on
    // it: three lists, two of which were right.
    out.push_str(&format!(
        "\n# Every rmw a descriptor in this checkout provides. DERIVED — see above.\n\
         set(NROS_RMW_KNOWN \"{}\" CACHE INTERNAL \"rmw names provided by nros-rmw.toml descriptors\")\n\
         \n\
         # nros_rmw_is_known(<name> <out_var>) — TRUE when a descriptor claims <name>.\n\
         function(nros_rmw_is_known name out_var)\n\
         \x20   if(name IN_LIST NROS_RMW_KNOWN)\n\
         \x20       set(${{out_var}} TRUE PARENT_SCOPE)\n\
         \x20   else()\n\
         \x20       set(${{out_var}} FALSE PARENT_SCOPE)\n\
         \x20   endif()\n\
         endfunction()\n",
        known_rmw().join(";")
    ));
    out
}

/// Repo-relative path of the committed CMake lowering of [`render_cmake_dispatch`].
pub const CMAKE_DISPATCH_REL_PATH: &str = "cmake/NanoRosRmwDispatch.cmake";

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

    fn cmake_dispatch_path() -> std::path::PathBuf {
        // CARGO_MANIFEST_DIR = packages/cli/cargo-nano-ros → repo root is ../../..
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join(CMAKE_DISPATCH_REL_PATH)
    }

    #[test]
    fn rmw_cmake_dispatch_is_current() {
        let want = render_cmake_dispatch();
        let path = cmake_dispatch_path();
        let got = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "cannot read {} ({e}); regenerate with `cargo test -p cargo-nano-ros \
                 regenerate_cmake_dispatch -- --ignored`",
                path.display()
            )
        });
        assert_eq!(
            got, want,
            "{} is stale — resolve_rmw() changed. Regenerate: `cargo test -p cargo-nano-ros \
             regenerate_cmake_dispatch -- --ignored`",
            CMAKE_DISPATCH_REL_PATH
        );
    }

    /// Writes the committed CMake lowering from `resolve_rmw()`. Ignored by default
    /// (it mutates a tracked file); run explicitly to regenerate after editing the
    /// dispatch data: `cargo test -p cargo-nano-ros regenerate_cmake_dispatch -- --ignored`.
    #[test]
    #[ignore = "writes a tracked file; run explicitly to regenerate"]
    fn regenerate_cmake_dispatch() {
        let path = cmake_dispatch_path();
        std::fs::write(&path, render_cmake_dispatch())
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
}
