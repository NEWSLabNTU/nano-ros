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

/// Phase 228 / 256 W4.2 — collect each system component's declared
/// `callback_groups` from the component-package metadata in `cfg`, keyed by the
/// system `[[component]].name` (the instance name `ResolvedTier.members` use).
/// Shared by the C bake (`codegen_system`) and the Rust codegen (`generate`).
pub fn collect_callback_groups(
    cfg: &NrosConfig,
    components: &[SystemComponentEntry],
) -> BTreeMap<String, Vec<CallbackGroupDecl>> {
    let mut map = BTreeMap::new();
    for c in components {
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
