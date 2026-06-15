//! Capability/feature-axis lowering (Phase 252, design-of-record RFC-0031 §
//! "Generalization (Phase 250 / issue 0072)").
//!
//! A capability is a declared, language-agnostic axis the user toggles in config
//! (`[safety]`, `[param_services]`, future). It generalizes the RMW/platform
//! selection model: a declared axis **lowers** to up to three Rust build targets —
//! the entry `nros/<feat>` umbrella feature, the concrete BACKEND crate's own
//! feature (the capability's wire behaviour lives there, e.g. the CRC attach +
//! validate in `nros-rmw-zenoh/safety-e2e`), and the BOARD crate's forwarding
//! feature (the board is the selection point, RFC-0031 C5b). Cargo features do not
//! propagate upward, so reaching the backend is NOT automatic from
//! `nros/<feat>` — this table is the single source of truth for which targets a
//! given axis lowers to, and which backends carry the backend feature.
//!
//! It lives in `cargo-nano-ros` (the lower crate) so both the scaffolder here and
//! the orchestration `generate` in `nros-cli-core` share one mapping — mirroring
//! `rmw_resolver`.
//!
//! C/C++ (`cmake_token` / `c_define`) is reserved for a future wave — `safety-e2e`
//! is Rust-only today (the CRC machinery is feature-gated inside the zpico Rust
//! shim; no `NROS_SAFETY` define exists).

/// A declared capability axis lowered to its Rust build-feature targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// The declared config axis name, e.g. `"safety"` (the `[safety]` block) or
    /// `"param_services"` (the `[param_services]` block).
    pub declared: &'static str,
    /// The `nros` umbrella feature the axis enables on the generated entry
    /// (target 1) — e.g. `"safety-e2e"` → `nros/safety-e2e`. Always present.
    pub nros_feature: &'static str,
    /// The feature name on the concrete BACKEND crate (target 2, the direct
    /// `nros-rmw-<x>` dep on board-less native) AND the forwarding feature the
    /// BOARD crate carries (target 3). `None` ⇒ the axis is entry-umbrella-only
    /// (no backend wire behaviour), e.g. `param_services`.
    pub backend_feature: Option<&'static str>,
    /// The RMW backends that declare `backend_feature`. The backend feature is
    /// emitted only for these; others (no such feature) no-op. Empty when
    /// `backend_feature` is `None`.
    pub backends_supporting: &'static [&'static str],
}

impl Capability {
    /// `true` if this capability's backend feature applies to `backend` (a
    /// canonical RMW name, e.g. `"zenoh"`).
    pub fn backend_supports(&self, backend: &str) -> bool {
        self.backend_feature.is_some() && self.backends_supporting.contains(&backend)
    }
}

/// The capability axes nano-ros lowers today.
pub const CAPABILITIES: &[Capability] = &[
    // E2E message integrity. CRC attach (publisher) + validate (subscriber) live
    // behind the zenoh backend's own `safety-e2e`; xrce / cyclonedds carry no such
    // feature, so the axis no-ops there (issue 0072).
    Capability {
        declared: "safety",
        nros_feature: "safety-e2e",
        backend_feature: Some("safety-e2e"),
        backends_supporting: &["zenoh"],
    },
    // The external ROS 2 parameter SERVER. Entry-umbrella-only — the runtime
    // registers the services; there is no backend wire feature.
    Capability {
        declared: "param_services",
        nros_feature: "param-services",
        backend_feature: None,
        backends_supporting: &[],
    },
];

/// Look up a declared capability axis. `None` for an unknown axis.
pub fn capability(declared: &str) -> Option<&'static Capability> {
    CAPABILITIES.iter().find(|c| c.declared == declared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_lowers_to_all_three_targets_zenoh_only() {
        let c = capability("safety").expect("safety axis");
        assert_eq!(c.nros_feature, "safety-e2e");
        assert_eq!(c.backend_feature, Some("safety-e2e"));
        assert!(c.backend_supports("zenoh"));
        // xrce / cyclonedds have no CRC path → no backend feature.
        assert!(!c.backend_supports("xrce"));
        assert!(!c.backend_supports("cyclonedds"));
    }

    #[test]
    fn param_services_is_entry_umbrella_only() {
        let c = capability("param_services").expect("param_services axis");
        assert_eq!(c.nros_feature, "param-services");
        assert_eq!(c.backend_feature, None);
        assert!(!c.backend_supports("zenoh"));
    }

    #[test]
    fn unknown_axis_is_none() {
        assert!(capability("nope").is_none());
    }
}
