//! Tier resolver — re-export shim (Phase 228.E).
//!
//! The resolver + its result types moved to the shared
//! [`nros_orchestration_ir`] crate so the `nros::main!()` proc-macro can run
//! the exact same resolution at compile time (the CLI can't be a proc-macro
//! dep). This module re-exports them so existing `orchestration::tier_resolver::*`
//! references stay valid.
//!
//! The one shape change: [`resolve_tiers`] now takes the decomposed
//! `system.toml` pieces (`tiers`, `node_overrides`, `component_names`,
//! `callback_groups`) instead of a whole `SystemToml`, so the leaf crate stays
//! free of the full CLI config type. See [`crate::cmd::codegen_system`] for the
//! call site that adapts a `SystemToml` to it.

pub use nros_orchestration_ir::{
    DEFAULT_TIER, ResolvedTier, ResolvedTierTable, TierResolveError, resolve_tiers,
};

use super::{
    cargo_metadata_schema::{CallbackGroupDecl, SystemComponentEntry, SystemToml},
    nros_config::NrosConfig,
};
use std::collections::{BTreeMap, BTreeSet};

/// Phase 228 / 256 W4.2 / 273 W2 — collect each system component's declared
/// callback-group → tier bindings, keyed by the system `[[component]].name`
/// (the instance name `ResolvedTier.members` use).
/// Shared by the C bake (`codegen_system`) and the Rust codegen (`generate`).
///
/// Phase 273 (RFC-0047 W2): `[[component]].group_tiers` is the new source of
/// truth for the group → tier binding. When present it takes priority over the
/// package manifest's `callback_groups` tier field. When absent the manifest is
/// honoured for one release (deprecated path — emit a warning so workspaces can
/// migrate to `system.toml group_tiers`).
pub fn collect_callback_groups(
    cfg: &NrosConfig,
    components: &[SystemComponentEntry],
) -> BTreeMap<String, Vec<CallbackGroupDecl>> {
    let mut map = BTreeMap::new();
    for c in components {
        // Phase 273 (W2): prefer system.toml [[component]].group_tiers (RFC-0047).
        if !c.group_tiers.is_empty() {
            let groups: Vec<CallbackGroupDecl> = c
                .group_tiers
                .iter()
                .map(|(id, tier)| CallbackGroupDecl {
                    id: id.clone(),
                    r#type: "MutuallyExclusive".to_string(),
                    tier: tier.clone(),
                })
                .collect();
            map.insert(c.name.clone(), groups);
            continue;
        }

        // Fallback: package-manifest `callback_groups` tier (deprecated — move to
        // [[component]].group_tiers in system.toml).
        let Some(pkg) = cfg.component_packages.get(&c.pkg) else {
            continue;
        };
        // Single-node pkg → node_or_component; multi-node pkg → match by name/class.
        let groups = pkg
            .nros
            .node_or_component()
            .filter(|m| !m.callback_groups.is_empty())
            .map(|m| m.callback_groups.clone())
            .or_else(|| {
                pkg.nros
                    .nodes_or_components()
                    .values()
                    .find(|m| {
                        m.name.as_deref() == Some(c.name.as_str())
                            || m.class.as_deref() == Some(c.class.as_str())
                    })
                    .map(|m| m.callback_groups.clone())
            })
            .unwrap_or_default();
        let has_pkg_tiers = groups.iter().any(|g| g.tier != DEFAULT_TIER);
        if has_pkg_tiers {
            eprintln!(
                "[WARN] nros: package `{}` (component `{}`) has `callback_groups` with a tier \
                 assignment in Cargo.toml. Move it to `system.toml [[component]].group_tiers` \
                 (Phase 273 / RFC-0047). Package-level tier binding is deprecated.",
                c.pkg, c.name
            );
        }
        if !groups.is_empty() {
            map.insert(c.name.clone(), groups);
        }
    }
    map
}

/// Adapt a full [`SystemToml`] + its components' `callback_groups` to the
/// decomposed [`resolve_tiers`] signature.
pub fn resolve_system_tiers(
    system: &SystemToml,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
    target_rtos: &str,
) -> Result<ResolvedTierTable, TierResolveError> {
    let component_names: BTreeSet<&str> =
        system.components.iter().map(|c| c.name.as_str()).collect();
    resolve_tiers(
        &system.tiers,
        &system.node_overrides,
        &component_names,
        callback_groups,
        target_rtos,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::{
        cargo_metadata_schema::{SystemComponentEntry, SystemToml, TierDef, TierRtosSpec},
        nros_config::NrosConfig,
    };

    /// Phase 273 (W2) — `collect_callback_groups` prefers `[[component]].group_tiers`
    /// over the package-manifest `callback_groups` tier. An empty `NrosConfig`
    /// (no component packages) is sufficient because the `group_tiers` path never
    /// touches `cfg.component_packages`.
    #[test]
    fn collect_prefers_group_tiers_over_pkg_manifest() {
        let cfg = NrosConfig::default();
        let mut gt = BTreeMap::new();
        gt.insert("ctrl".to_string(), "high".to_string());
        gt.insert("telem".to_string(), "low".to_string());
        let components = vec![SystemComponentEntry {
            pkg: "ctrl_pkg".to_string(),
            class: "ctrl_pkg::Ctrl".to_string(),
            name: "ctrl_node".to_string(),
            group_tiers: gt,
        }];
        let map = collect_callback_groups(&cfg, &components);
        let decls = map.get("ctrl_node").expect("ctrl_node must be in map");
        assert_eq!(decls.len(), 2);
        // Both groups resolved from group_tiers.
        let by_id: BTreeMap<&str, &str> = decls
            .iter()
            .map(|d| (d.id.as_str(), d.tier.as_str()))
            .collect();
        assert_eq!(by_id["ctrl"], "high");
        assert_eq!(by_id["telem"], "low");
    }

    /// Phase 273 (W2) — `resolve_system_tiers` with `group_tiers` produces the
    /// expected `(component, group) → sched_context` mapping without needing
    /// `[[node_overrides]]`.
    #[test]
    fn resolve_system_tiers_from_group_tiers() {
        let system_toml_str = r#"
[system]
name = "test"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "ctrl_pkg"
class = "ctrl_pkg::Ctrl"
name = "ctrl_node"
group_tiers = { ctrl = "high" }

[[component]]
pkg = "telem_pkg"
class = "telem_pkg::Telem"
name = "telem_node"
group_tiers = { telem = "low" }

[tiers.high]
spin_period_us = 10000
[tiers.high.posix]
priority = 80

[tiers.low]
spin_period_us = 100000
[tiers.low.posix]
priority = 10
"#;
        let system: SystemToml = toml::from_str(system_toml_str).expect("parse system.toml");
        let cfg = NrosConfig::default();
        let callback_groups = collect_callback_groups(&cfg, &system.components);
        let table =
            resolve_system_tiers(&system, &callback_groups, "posix").expect("resolve_system_tiers");
        assert!(
            !table.is_single_tier(),
            "two-tier plan must not be single-tier"
        );
        // highest-priority-first: high (80) = idx 0, low (10) = idx 1.
        assert_eq!(table.tiers[0].name, "high");
        assert_eq!(table.tiers[1].name, "low");
        assert!(
            table.tiers[0]
                .members
                .contains(&("ctrl_node".to_string(), "ctrl".to_string())),
            "ctrl_node/ctrl must be in high tier"
        );
        assert!(
            table.tiers[1]
                .members
                .contains(&("telem_node".to_string(), "telem".to_string())),
            "telem_node/telem must be in low tier"
        );
    }
    /// Phase 273 W4 (RFC-0047) — sub-node: ONE component with TWO groups on TWO
    /// tiers must resolve without error (NodeSpansTiers v1 constraint lifted).
    #[test]
    fn resolve_system_tiers_sub_node_two_groups_two_tiers() {
        let system_toml_str = r#"
[system]
name = "test_subnode"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "subnode_pkg"
class = "subnode_pkg::SubNode"
name = "sub_node"
group_tiers = { ctrl = "high", telem = "low" }

[tiers.high]
spin_period_us = 10000
[tiers.high.posix]
priority = 80

[tiers.low]
spin_period_us = 100000
[tiers.low.posix]
priority = 10
"#;
        let system: SystemToml = toml::from_str(system_toml_str).expect("parse system.toml");
        let cfg = NrosConfig::default();
        let callback_groups = collect_callback_groups(&cfg, &system.components);
        let table = resolve_system_tiers(&system, &callback_groups, "posix")
            .expect("sub-node must resolve");
        assert!(!table.is_single_tier(), "must be multi-tier");
        let high = table.tiers.iter().find(|t| t.name == "high").unwrap();
        let low = table.tiers.iter().find(|t| t.name == "low").unwrap();
        // Same node, both groups resolved to different tiers (the RFC-0047 sub-node proof).
        assert!(
            high.members
                .contains(&("sub_node".to_string(), "ctrl".to_string())),
            "sub_node/ctrl must be in high tier"
        );
        assert!(
            low.members
                .contains(&("sub_node".to_string(), "telem".to_string())),
            "sub_node/telem must be in low tier"
        );
    }
}

/// Best-effort RTOS name for tier resolution from the selected deploy target.
/// Defaults to `posix` (native); embedded targets refine it from the deploy
/// `board`/`kind` hint.
pub fn derive_target_rtos(system: &SystemToml, target: Option<&str>) -> String {
    target
        .and_then(|t| system.deploy.get(t))
        .and_then(|d| d.board.as_deref().or(d.kind.as_deref()))
        .map(|hint| {
            for rtos in ["freertos", "zephyr", "threadx", "nuttx"] {
                if hint.contains(rtos) {
                    return rtos.to_string();
                }
            }
            "posix".to_string()
        })
        .unwrap_or_else(|| "posix".to_string())
}
