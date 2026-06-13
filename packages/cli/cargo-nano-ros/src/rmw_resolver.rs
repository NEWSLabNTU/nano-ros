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

/// The RMW backends nano-ros supports today. `dust-dds` was retired (Phase 169).
pub const KNOWN_RMW: &[&str] = &["zenoh", "xrce", "cyclonedds"];

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
}

/// A declared RMW value that is not one of [`KNOWN_RMW`].
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
            KNOWN_RMW.join(", ")
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
pub fn canonical_rmw(input: &str) -> Option<&'static str> {
    match input {
        "zenoh" | "rmw-zenoh" | "rmw-zenoh-cffi" => Some("zenoh"),
        "xrce" | "rmw-xrce" | "rmw-xrce-cffi" => Some("xrce"),
        "cyclonedds" | "rmw-cyclonedds" | "rmw-cyclonedds-cffi" => Some("cyclonedds"),
        _ => None,
    }
}

/// Lower a declared RMW string to its per-language build forms.
///
/// Accepts the canonical name or any alias ([`canonical_rmw`]). An unknown
/// value is an error — caught early in the loader so a typo fails with a clear
/// message rather than producing a broken build.
pub fn resolve_rmw(declared: &str) -> Result<ResolvedRmw, UnknownRmw> {
    match canonical_rmw(declared) {
        Some("zenoh") => Ok(ResolvedRmw {
            declared: "zenoh",
            cargo_feature: "rmw-zenoh",
            cmake_value: "zenoh",
            c_define_token: "ZENOH",
        }),
        Some("xrce") => Ok(ResolvedRmw {
            declared: "xrce",
            cargo_feature: "rmw-xrce",
            cmake_value: "xrce",
            c_define_token: "XRCE",
        }),
        Some("cyclonedds") => Ok(ResolvedRmw {
            declared: "cyclonedds",
            cargo_feature: "rmw-cyclonedds",
            cmake_value: "cyclonedds",
            c_define_token: "CYCLONEDDS",
        }),
        _ => Err(UnknownRmw {
            declared: declared.to_string(),
        }),
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
        for name in KNOWN_RMW {
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
        for name in KNOWN_RMW {
            let r = resolve_rmw(name).unwrap();
            assert_eq!(r.cargo_feature, format!("rmw-{name}"));
        }
    }
}
