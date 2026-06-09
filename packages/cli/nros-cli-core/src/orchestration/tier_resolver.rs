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
