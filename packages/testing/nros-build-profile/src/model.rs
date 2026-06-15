//! Normalized cross-backend build-profile model.
//!
//! Collectors parse backend-native artifacts into [`RawUnit`]s; the normalizer
//! folds those into a single [`BuildProfile`] (stages + units) that the reporter
//! and the JSON writer consume. See `docs/superpowers/specs/2026-06-16-build-profiling-design.md`.

use serde::Serialize;

/// Which build backend produced the timing data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    /// `west build` (Zephyr) — ninja under the hood.
    NinjaWest,
    /// Plain `cmake` C/C++ — ninja.
    NinjaCmake,
    /// `idf.py` (esp32-idf) — cmake + ninja.
    NinjaIdf,
    /// Generic ninja whose driver could not be identified.
    Ninja,
    /// `cargo` (native, esp32 bare-metal, all cross targets).
    Cargo,
    /// Both a ninja log and cargo timings were found.
    Mixed,
}

impl Backend {
    /// Human label for the report header.
    pub fn label(self) -> &'static str {
        match self {
            Backend::NinjaWest => "ninja (west)",
            Backend::NinjaCmake => "ninja (cmake)",
            Backend::NinjaIdf => "ninja (idf.py)",
            Backend::Ninja => "ninja",
            Backend::Cargo => "cargo",
            Backend::Mixed => "mixed (ninja + cargo)",
        }
    }
}

/// Coarse stage a unit belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Code generation — cargo build scripts, rosidl/codegen outputs.
    Codegen,
    /// Compilation of a translation unit / crate.
    Compile,
    /// Linking / archiving / image assembly.
    Link,
    /// Flashing or image post-processing.
    Flash,
    /// Anything not otherwise classified.
    Other,
}

impl Kind {
    /// Stage-table display order.
    pub const ORDER: [Kind; 5] = [
        Kind::Codegen,
        Kind::Compile,
        Kind::Link,
        Kind::Flash,
        Kind::Other,
    ];

    /// Lowercase stage name for the table.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Codegen => "codegen",
            Kind::Compile => "compile",
            Kind::Link => "link",
            Kind::Flash => "flash",
            Kind::Other => "other",
        }
    }
}

/// One timed work item parsed from a backend artifact.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RawUnit {
    /// Display name (crate name, output path basename, …).
    pub name: String,
    /// Stage classification.
    pub kind: Kind,
    /// Wall-clock duration in seconds.
    pub dur_s: f64,
    /// Start offset in seconds from the build's first event (for span/total).
    pub start_s: f64,
    /// `true` when the unit is a C/C++/archive output (no Rust incremental).
    pub is_native: bool,
}

/// A unit as it appears in the normalized profile (start dropped).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unit {
    pub name: String,
    pub kind: Kind,
    pub dur_s: f64,
    pub is_native: bool,
}

/// Aggregated per-stage timing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stage {
    pub name: &'static str,
    pub dur_s: f64,
    /// Percentage of `total_s` (0–100), rounded for display elsewhere.
    pub pct: f64,
}

/// The full normalized build profile — the single artifact every component
/// downstream of the collectors operates on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuildProfile {
    pub backend: Backend,
    pub total_s: f64,
    pub stages: Vec<Stage>,
    pub units: Vec<Unit>,
    /// `false` when only coarse/wall-clock data was available (e.g. cargo built
    /// without `--timings`) — the reporter then suppresses the deep drill-down.
    pub captured_deep: bool,
    /// Non-fatal parse notes (skipped malformed lines, missing artifacts).
    pub notes: Vec<String>,
}
