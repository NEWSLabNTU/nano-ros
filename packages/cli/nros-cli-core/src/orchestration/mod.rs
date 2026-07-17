//! Build-time orchestration schemas.
//!
//! Schema modules are data contracts only. Planner modules consume those
//! contracts and host-side launch artifacts; generated target code remains in
//! the Phase 126.D surface.

pub mod ament;
pub mod board_descriptor;
pub mod board_metadata;
pub mod bridge_gen;
pub mod cargo_metadata_schema;
pub mod cmake_preset;
pub mod config;
pub mod launch_synth;
pub mod manifest;
pub mod metadata_build;
pub mod model_ingest;
pub mod names;
pub mod nros_config;
pub mod params;
pub mod plan;
pub mod planner;
pub mod schema;
pub mod sdk_index;
pub mod sdk_store;
pub mod source_metadata;
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
