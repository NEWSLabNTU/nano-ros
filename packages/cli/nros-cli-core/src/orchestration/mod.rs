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
// phase-440 W7 (RFC-0095 D8) — the launcher: read the pin, ensure that
// toolchain, `exec` it. Three jobs; a fourth is a bug.
//
// phase-443 W3 MOVED the file to the `nros-launcher` crate (RFC-0097 D4): the
// launcher is now its own binary, so the logic that selects a version can stop
// shipping inside the version being selected. Re-exported at its original path
// because this binary is still fronted on installed hosts and still calls
// `redispatch()` before clap — and because one parser of `nros-toolchain.toml`
// in the tree is the whole point of moving it rather than copying it.
pub use nros_launcher::dispatch;
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
// phase-447 A2 (RFC-0099 D3) — the four-rung ladder to the SDK root. One
// spelling, so a released toolchain's own `share/nano-ros` is reachable from
// every site that used to bail with "no nano-ros checkout found".
pub mod nano_ros_root;
pub mod nros_config;
pub mod params;
// phase-440 W7 (RFC-0095 D7/D9) — `nros-toolchain.toml`, the per-project pin.
// MOVED to `nros-launcher` by phase-443 W3, re-exported here; see `dispatch`
// above.
pub use nros_launcher::pin;
pub mod plan;
pub mod planner;

pub mod prereq_resolve;
/// RFC-0097 D7 / phase-443 W2 — `share/nros/manifest.toml`, what a release
/// DECLARES about itself instead of asserting three versions equal.
pub mod release_manifest;
/// RFC-0099 D8 / phase-447 D3 — the pinned, vendored rosdep snapshot that the
/// `<depend>` ladder falls back to below `[prereq.*]`.
pub mod rosdep_snapshot;
pub use nros_orchestration_ir::rtos_realizer;
/// phase-447 D1 (RFC-0099 D5) — is this host at or above a dist's floor?
pub mod host_floor;
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
