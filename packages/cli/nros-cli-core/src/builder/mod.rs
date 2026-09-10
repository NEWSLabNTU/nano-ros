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
pub mod cmake_root;
pub mod discover;
pub mod dist;
pub mod entry;
pub mod handoff;
pub mod materialize;
pub mod paths;
pub mod plan;
pub mod preflight;
pub mod zephyr;
