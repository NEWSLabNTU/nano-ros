//! Phase 136.1 — `zenoh_platforms.toml` parser.
//!
//! 136.1 lands the loader + per-platform resolver only. The build
//! script does not yet consume the resolved data — that happens in
//! 136.3 / 136.4 when `build_zenoh_pico_native` and the per-RTOS
//! cc-rs functions collapse into one. Keeping the parser separate
//! lets it land + test independently of the cc-rs rewrite.
//!
//! Include from `build.rs` with:
//! ```ignore
//! #[path = "build/manifest.rs"]
//! mod manifest;
//! ```

use std::{collections::BTreeMap, fs, path::Path};

use serde::Deserialize;

/// Top-level manifest: `[platform.<name>]` blocks keyed by name.
#[derive(Debug, Deserialize)]
pub struct PlatformManifest {
    pub platform: BTreeMap<String, PlatformEntry>,
}

/// One `[platform.<name>]` block as it appears in the TOML.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct PlatformEntry {
    /// Optional parent platform name. The parent's fields are
    /// merged before this entry's fields override them.
    #[serde(default)]
    pub inherits: Option<String>,
    /// Preprocessor defines added unconditionally
    /// (`cc::Build::define(name, None)`).
    #[serde(default)]
    pub defines: Vec<String>,
    /// Glob roots under `zenoh-pico/src/` for source selection.
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob roots under `zenoh-pico/src/` to exclude from
    /// `include` matches.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// `cargo:rustc-link-lib=` system libraries (POSIX hosts).
    #[serde(default)]
    pub system_libs: Vec<String>,
    /// Where mbedTLS comes from when `link-tls` is active:
    /// `pkg-config`, `vendored`, or `none`.
    #[serde(default)]
    pub mbedtls: Option<String>,
    /// Per-link-feature policy overrides.
    #[serde(default)]
    pub link: BTreeMap<String, LinkOverride>,
}

/// Per-link-feature override declared in `zenoh_platforms.toml`.
/// `bool` collapses to On / Off; a string like `"feature"` defers
/// to the matching `CARGO_FEATURE_*` env var. Distinct from the
/// build-script `LinkPolicy` struct in `build/policy.rs`, which is
/// the resolved mask passed into `LinkFeatures::apply`.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum LinkOverride {
    On(bool),
    Mode(String),
}

/// Resolved view of one platform after `inherits` chain merge.
/// What `build.rs` will consume in 136.3 / 136.4. The fields are
/// read only by tests + future-phase code; `dead_code` is silenced
/// for the 136.1 window.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ResolvedPlatform {
    pub name: String,
    pub defines: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub system_libs: Vec<String>,
    pub mbedtls: Option<String>,
    pub link: BTreeMap<String, LinkOverride>,
}

impl PlatformManifest {
    /// Parse the manifest from a TOML file on disk.
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let text = fs::read_to_string(path).map_err(|e| ManifestError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Self::parse(&text)
    }

    /// Parse the manifest from an in-memory TOML string. Useful for
    /// unit tests; production builds use `load`.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        toml::from_str(text).map_err(ManifestError::Parse)
    }

    /// Resolve one `[platform.<name>]` block, walking the
    /// `inherits` chain. Child fields win when both parent and
    /// child set the same key; `defines` / `include` / `exclude` /
    /// `system_libs` are unioned (parent first, then child); `link`
    /// merges per-key (child overrides parent for matching keys).
    pub fn for_platform(&self, name: &str) -> Result<ResolvedPlatform, ManifestError> {
        let mut seen = std::collections::BTreeSet::new();
        let entry = self.resolve(name, &mut seen)?;
        Ok(ResolvedPlatform {
            name: name.to_string(),
            defines: entry.defines,
            include: entry.include,
            exclude: entry.exclude,
            system_libs: entry.system_libs,
            mbedtls: entry.mbedtls,
            link: entry.link,
        })
    }

    fn resolve(
        &self,
        name: &str,
        seen: &mut std::collections::BTreeSet<String>,
    ) -> Result<PlatformEntry, ManifestError> {
        if !seen.insert(name.to_string()) {
            return Err(ManifestError::InheritCycle(name.to_string()));
        }
        let entry = self
            .platform
            .get(name)
            .ok_or_else(|| ManifestError::UnknownPlatform(name.to_string()))?
            .clone();
        let parent = match entry.inherits.as_deref() {
            Some(parent_name) => Some(self.resolve(parent_name, seen)?),
            None => None,
        };
        Ok(merge(parent, entry))
    }
}

fn merge(parent: Option<PlatformEntry>, mut child: PlatformEntry) -> PlatformEntry {
    let Some(parent) = parent else {
        child.inherits = None;
        return child;
    };

    let mut defines = parent.defines;
    defines.extend(child.defines.drain(..));
    let mut include = parent.include;
    include.extend(child.include.drain(..));
    let mut exclude = parent.exclude;
    exclude.extend(child.exclude.drain(..));
    let mut system_libs = parent.system_libs;
    system_libs.extend(child.system_libs.drain(..));
    let mbedtls = child.mbedtls.or(parent.mbedtls);
    let mut link = parent.link;
    link.extend(std::mem::take(&mut child.link));
    PlatformEntry {
        inherits: None,
        defines,
        include,
        exclude,
        system_libs,
        mbedtls,
        link,
    }
}

#[derive(Debug)]
pub enum ManifestError {
    Io {
        path: String,
        source: std::io::Error,
    },
    Parse(toml::de::Error),
    UnknownPlatform(String),
    InheritCycle(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "read {path}: {source}"),
            Self::Parse(e) => write!(f, "parse zenoh_platforms.toml: {e}"),
            Self::UnknownPlatform(name) => write!(f, "unknown platform: {name}"),
            Self::InheritCycle(name) => write!(f, "inherits cycle through {name}"),
        }
    }
}

impl std::error::Error for ManifestError {}

// Note: the loader is exercised at build time — `build.rs` parses
// `zenoh_platforms.toml` + resolves every platform on every cargo
// build. A typo, broken `inherits` chain, or shape regression
// surfaces as a build-script panic. There is no separate
// `#[test]`-style suite because `cargo test` doesn't link build
// scripts; the build-time invariant is the real gate.
