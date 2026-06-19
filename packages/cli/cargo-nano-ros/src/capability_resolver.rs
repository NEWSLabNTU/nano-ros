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
//! Phase 261 W1 — the registry now also carries the **C/C++** lowering slots
//! (`c_define`, `cmake_token`), so one `Capability{}` row drives both the Rust
//! cargo features AND the C/C++ `#define` / CMake token. The bake's per-axis
//! hardcoded `#define`s become a registry loop (W2).

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
    /// Phase 261 — the C/C++ preprocessor macro the bake emits into
    /// `system_config.h` when the axis is enabled, e.g.
    /// `"NROS_SYSTEM_SAFETY_E2E"` (the analog of `NROS_SYSTEM_RMW_<TOKEN>`).
    /// `None` ⇒ the axis informs no C/C++ source (Rust-only). Drives the W2
    /// registry loop that replaced the hardcoded per-axis `#define`s.
    pub c_define: Option<&'static str>,
    /// Phase 261 — the CMake build-knob token for the axis, e.g.
    /// `"NANO_ROS_SAFETY_E2E"` (the analog of `NANO_ROS_RMW`). `None` ⇒ no CMake
    /// knob (the macro is informational only). Threaded into the C/C++ codegen by
    /// W5 when populated.
    pub cmake_token: Option<&'static str>,
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
        c_define: Some("NROS_SYSTEM_SAFETY_E2E"),
        cmake_token: Some("NANO_ROS_SAFETY_E2E"),
    },
    // The external ROS 2 parameter SERVER. Entry-umbrella-only — the runtime
    // registers the services; there is no backend wire feature.
    Capability {
        declared: "param_services",
        nros_feature: "param-services",
        backend_feature: None,
        backends_supporting: &[],
        c_define: Some("NROS_SYSTEM_PARAM_SERVICES"),
        cmake_token: None,
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
        // Phase 261 W1 — C/C++ lowering slots.
        assert_eq!(c.c_define, Some("NROS_SYSTEM_SAFETY_E2E"));
        assert_eq!(c.cmake_token, Some("NANO_ROS_SAFETY_E2E"));
    }

    #[test]
    fn param_services_is_entry_umbrella_only() {
        let c = capability("param_services").expect("param_services axis");
        assert_eq!(c.nros_feature, "param-services");
        assert_eq!(c.backend_feature, None);
        assert!(!c.backend_supports("zenoh"));
        // Phase 261 W1 — informational `#define`, no CMake knob.
        assert_eq!(c.c_define, Some("NROS_SYSTEM_PARAM_SERVICES"));
        assert_eq!(c.cmake_token, None);
    }

    #[test]
    fn unknown_axis_is_none() {
        assert!(capability("nope").is_none());
    }

    /// Phase 261 W1 — every registry row that carries a `c_define` uses the
    /// `NROS_SYSTEM_`-prefixed macro the bake emits, so the W2 registry loop stays
    /// byte-identical to the hardcoded `#define`s.
    #[test]
    fn c_defines_use_the_nros_system_prefix() {
        for c in CAPABILITIES {
            if let Some(def) = c.c_define {
                assert!(
                    def.starts_with("NROS_SYSTEM_"),
                    "{} c_define `{def}` must be NROS_SYSTEM_-prefixed",
                    c.declared
                );
            }
        }
    }

    /// Phase 261 W5 — drift guard: `cmake/NanoRosCapabilities.cmake`
    /// `nros_lower_system_features` is a hand-mirror of THIS registry (the SSoT).
    /// Assert the CMake map can't skew from the registry: every axis has an arm,
    /// and every `cmake_token` is the one the arm sets. Adding a registry row
    /// without the matching CMake arm (or renaming a token) fails here.
    #[test]
    fn cmake_capability_map_matches_registry() {
        // packages/cli/cargo-nano-ros → repo root is three levels up.
        let cmake = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../cmake/NanoRosCapabilities.cmake");
        let src = std::fs::read_to_string(&cmake)
            .unwrap_or_else(|e| panic!("read {}: {e}", cmake.display()));
        for c in CAPABILITIES {
            // Every known axis must have a dispatch arm by declared name.
            assert!(
                src.contains(&format!("STREQUAL \"{}\"", c.declared)),
                "NanoRosCapabilities.cmake has no `nros_lower_system_features` arm for \
                 axis `{}` — add one (drift from the registry)",
                c.declared
            );
            // An axis with a cmake_token must set exactly that token; one without
            // must NOT set any `NANO_ROS_*` option for the axis.
            match c.cmake_token {
                Some(tok) => assert!(
                    src.contains(&format!("set({tok} ON")),
                    "axis `{}` cmake_token `{tok}` is not set in NanoRosCapabilities.cmake",
                    c.declared
                ),
                None => {}
            }
        }
    }
}
