//! `nros build` — the colcon-like builder (RFC-0065, phase-383).
//!
//! Five stages, and the last one is a process replacement:
//!
//! ```text
//!   1. DISCOVER   walk package.xml ∪ cargo members → topological order
//!   2. RESOLVE    the image: argument > [system] default_images > list+fail
//!   3. PREFLIGHT  toolchains / SDKs / sources present?
//!   4. GENERATE   msg bindings + system model + the ROOT BUILD FILE
//!   5. EXEC       cmake --build / cargo build / west build — stderr untouched
//! ```
//!
//! Stage 4 emits a root build file ONLY where one would otherwise be
//! hand-written (RFC-0065 D3): west and ESP-IDF apps already have complete
//! drivers, and a copy-out leaf ships its own root by contract (RFC-0026).
//! Those targets go 1→2→3→5 with no generation at all.
//!
//! Everything stage 4 writes lives under `build/` (RFC-0098 D1/D9): the cmake
//! root at `build/<coord>/CMakeLists.txt`, and for cargo the entry package
//! `build/<coord>/<entry>/` — its own cargo root — with its settings file
//! beside it ([`cargo_config`]). A workspace has no root build file.

pub mod cargo_config;
pub mod cargo_root;
pub mod cmake_configure;
pub mod cmake_root;
pub mod discover;
pub mod dist;
pub mod entry;
pub mod handoff;
pub mod host_tool;
pub mod jobserver;
pub mod materialize;
pub mod paths;
pub mod plan;
pub mod preflight;
pub mod shared_target;
pub mod west_app;
pub mod zephyr;

/// The configure argument that tells a generated project's
/// `find_package(nano_ros)` which root to import (issue 1304).
///
/// `nano_rosConfig.cmake` sits at the SDK root, and since RFC-0103 D6 nothing
/// exports `nano_ros_ROOT`, so every road that configures a project naming
/// `find_package(nano_ros)` passes this. One spelling, because the west road
/// was the one that did not (issue 1998).
#[must_use]
pub fn nano_ros_root_arg(nano_ros_root: &std::path::Path) -> String {
    format!("-Dnano_ros_ROOT={}", nano_ros_root.display())
}

/// The configure argument that names the WORKSPACE to a west-built Rust
/// entry's `nros::main!` (issue 2314).
///
/// The cargo roads hand cargo `NROS_WORKSPACE_ROOT` in their settings file
/// ([`cargo_config`]); the west road cannot, because zephyr-lang-rust owns that
/// cargo command line. So it rides the configure line instead, and the Zephyr
/// module appends it to the Rust lane's env — the one carrier that lane has
/// (`_nros_rust_lane_env`, issue 1662). It is a `-D`, so it is CACHED: a later
/// bare `west build` of the same build dir keeps it. Absolute, and it lives
/// only in the build dir, so nothing `nros materialize` copies carries it.
#[must_use]
pub fn workspace_root_arg(workspace_root: &std::path::Path) -> String {
    format!("-DNROS_WORKSPACE_ROOT={}", workspace_root.display())
}
