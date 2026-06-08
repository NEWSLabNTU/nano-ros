//! RMW selection lowering (Phase 227.2, design-of-record RFC-0031).
//!
//! RMW is a declared, language-agnostic value (`system.toml [system].rmw` /
//! `[deploy.<t>].rmw` / CLI flag). This module is the single place that
//! validates a declared RMW string and **lowers** it to each language's build
//! mechanism — a Rust cargo feature and a CMake `-DNANO_ROS_RMW` value — plus the
//! C `#define` token the bake already emits. The cargo feature / CMake var are
//! lowering targets, not the user-facing knob.

use thiserror::Error;

/// The RMW backends nano-ros supports today. `dust-dds` was retired (Phase 169).
pub const KNOWN_RMW: &[&str] = &["zenoh", "xrce", "cyclonedds"];

/// A declared RMW value lowered to its per-language build forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRmw {
    /// The canonical declared name, e.g. `"zenoh"`.
    pub declared: &'static str,
    /// The `nros` facade cargo feature, e.g. `"rmw-zenoh"`.
    pub cargo_feature: &'static str,
    /// The `-DNANO_ROS_RMW` CMake value, e.g. `"zenoh"`.
    pub cmake_value: &'static str,
    /// The C `#define NROS_SYSTEM_RMW_<TOKEN>` token, e.g. `"ZENOH"`.
    pub c_define_token: &'static str,
}

/// A declared RMW value that is not one of [`KNOWN_RMW`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown rmw `{declared}` (known: {})", KNOWN_RMW.join(", "))]
pub struct UnknownRmw {
    pub declared: String,
}

/// Lower a declared RMW string to its per-language build forms.
///
/// Matching is exact and case-sensitive (the schema field is authored, not
/// free-text). An unknown value is an error — caught early in the loader so a
/// typo fails with a clear message rather than producing a broken build.
pub fn resolve_rmw(declared: &str) -> Result<ResolvedRmw, UnknownRmw> {
    match declared {
        "zenoh" => Ok(ResolvedRmw {
            declared: "zenoh",
            cargo_feature: "rmw-zenoh",
            cmake_value: "zenoh",
            c_define_token: "ZENOH",
        }),
        "xrce" => Ok(ResolvedRmw {
            declared: "xrce",
            cargo_feature: "rmw-xrce",
            cmake_value: "xrce",
            c_define_token: "XRCE",
        }),
        "cyclonedds" => Ok(ResolvedRmw {
            declared: "cyclonedds",
            cargo_feature: "rmw-cyclonedds",
            cmake_value: "cyclonedds",
            c_define_token: "CYCLONEDDS",
        }),
        other => Err(UnknownRmw {
            declared: other.to_string(),
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
    fn cargo_feature_matches_nros_facade_naming() {
        // Guards against drift from packages/core/nros/Cargo.toml [features].
        for name in KNOWN_RMW {
            let r = resolve_rmw(name).unwrap();
            assert_eq!(r.cargo_feature, format!("rmw-{name}"));
        }
    }
}
