//! Build-time orchestration schemas.
//!
//! Schema modules are data contracts only. Planner modules consume those
//! contracts and host-side launch artifacts; generated target code remains in
//! the Phase 126.D surface.

pub mod ament;
pub mod board_descriptor;
pub mod board_projection;
pub mod bridge_gen;
pub mod cargo_metadata_schema;
pub mod cmake_preset;
pub mod config;
/// phase-440 W7 (RFC-0095 D8) — the launcher: read the pin, ensure that
/// toolchain, `exec` it. Three jobs; a fourth is a bug.
pub mod dispatch;
pub mod facade;
/// phase-383 W1 — `[image.<id>]`, the buildable unit (RFC-0065 D6).
pub mod image;
pub mod launch_synth;
pub mod manifest;
// W5.13 follow-up — relocated to nros-orchestration-ir (shared with the macro);
// re-exported so `crate::orchestration::mapper_input::…` paths keep resolving.
pub use nros_orchestration_ir::mapper_input;
// phase-330 W3.b — the shared SystemModel search order, re-exported the same
// way as the other orchestration-ir modules so consumers reach it through
// `orchestration::`.
pub use nros_orchestration_ir::model_location;
pub mod metadata_build;
pub mod metadata_probe_cmake;
pub mod metadata_refresh;
pub mod model_ingest;
pub mod names;
pub mod nros_config;
pub mod params;
/// phase-440 W7 (RFC-0095 D7/D9) — `nros-toolchain.toml`, the per-project pin.
pub mod pin;
pub mod plan;
pub mod planner;
pub mod prereq_resolve;
pub use nros_orchestration_ir::rtos_realizer;
pub mod schema;
pub mod sdk_index;
pub mod sdk_store;
/// phase-351 W1 — the SITE half of a deploy target (RFC-0072 §5).
pub mod site_config;
pub mod source_metadata;
/// phase-440 W6 — the store's inventory, pin predicate and gc plan (RFC-0095
/// D2 + D11). `sdk_store` is where a version GOES; this is what is there.
pub mod store;
pub mod tier_resolver;
pub mod workspace;

pub use cargo_nano_ros::{
    capability_resolver,
    capability_resolver::{Capability, capability},
    rmw_resolver,
    rmw_resolver::{ResolvedRmw, UnknownRmw, resolve_rmw},
};
pub use config::ComponentConfig;
pub use nros_config::{
    BringupPackageEntry, BringupSource, ComponentPackageEntry, NrosConfig, NrosConfigError,
};
pub use plan::NrosPlan;
pub use source_metadata::SourceMetadata;
pub use workspace::{ComponentDeclaration, Package, Workspace};
