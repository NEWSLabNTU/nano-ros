//! Manifest parser for `<kernel>_platforms.toml` files.
//!
//! Phase 136.1 / 136.4 landed the loader + per-platform resolver
//! inside `zpico-sys/build/`. Phase 152.5 lifted it into the
//! `nros-board-common` library so the per-kernel generic board
//! crates can share one canonical implementation.
//!
//! Schema carries every per-platform datum the cc-rs collapse
//! needs: SDK env vars (with help text + validation), conditional
//! include paths (interpolated `{env:VAR}` / `{nros}` / `{out}` /
//! `{src}` tokens; `when.target_match` / `when.target_not` /
//! `when.if_env` gates), extra source files (with `if_env` and
//! `with_define` modifiers), debug-env-driven defines, and an
//! `[arch.*]` table for reusable target-arch compiler-flag
//! profiles.
//!
//! Use from `build.rs`:
//! ```ignore
//! use nros_board_common::manifest::PlatformManifest;
//! let m = PlatformManifest::load("zenoh_platforms.toml".as_ref())?;
//! let resolved = m.for_platform("posix")?;
//! ```

use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Deserializer};

/// Accept either a single TOML string (`arch = "cortex-m3"`) or an
/// array (`arch = ["cortex-m3", "riscv32imc"]`). Returns the
/// normalised `Vec<String>`. Phase 148.
fn deserialize_arch_field<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        Scalar(String),
        Vec(Vec<String>),
    }
    Ok(match StringOrVec::deserialize(d)? {
        StringOrVec::Scalar(s) => vec![s],
        StringOrVec::Vec(v) => v,
    })
}

/// Top-level manifest: `[platform.<name>]` + `[arch.<name>]`.
#[derive(Debug, Deserialize)]
pub struct PlatformManifest {
    pub platform: BTreeMap<String, PlatformEntry>,
    #[serde(default)]
    pub arch: BTreeMap<String, ArchEntry>,
}

/// One `[platform.<name>]` block.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct PlatformEntry {
    /// Optional parent platform name. Parent fields are merged
    /// before this entry's fields override them.
    #[serde(default)]
    pub inherits: Option<String>,
    /// Preprocessor defines added unconditionally
    /// (`cc::Build::define(name, None)`).
    #[serde(default)]
    pub defines: Vec<String>,
    /// Key=value defines (`cc::Build::define(name, Some(value))`).
    #[serde(default)]
    pub defines_kv: BTreeMap<String, String>,
    /// Defines whose value comes from an env var with a literal
    /// default.
    #[serde(default)]
    pub defines_env: BTreeMap<String, EnvDefault>,
    /// Glob roots under `zenoh-pico/src/` for core protocol /
    /// system-common source selection. Drift gate (136.6)
    /// validates these.
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
    /// Per-link-feature overrides declared in the manifest.
    /// `LinkOverride::On(false)` forces off; `Mode("feature")`
    /// defers to `CARGO_FEATURE_LINK_<X>`.
    #[serde(default)]
    pub link: BTreeMap<String, LinkOverride>,
    /// Extra source files (paths interpolated; see `{nros}` /
    /// `{src}` tokens), optionally conditional on env presence
    /// and pulling in additional defines when included.
    #[serde(default)]
    pub extra_sources: Vec<ExtraSource>,
    /// Required env vars + help text + optional sub-dir
    /// validation. Build script panics loudly when absent.
    #[serde(default)]
    pub required_env: Vec<RequiredEnv>,
    /// Unconditional include paths (interpolated). Order matters
    /// — first wins for `#include` resolution.
    #[serde(default)]
    pub include_paths: Vec<String>,
    /// Include paths gated by a `when` matcher (target /
    /// env-presence).
    #[serde(default)]
    pub include_paths_conditional: Vec<ConditionalPath>,
    /// Optional `[arch.*]` profile(s) to apply (cflags + sysroot /
    /// errno-override hooks). Accepts a single arch name (scalar
    /// TOML string) or a list (TOML array); single-arch platforms
    /// stay readable while multi-arch platforms (bare-metal across
    /// cortex-m3 + riscv32imc) declare every arch they support and
    /// let `build.rs::build_zenoh_pico_unified` pick the first one
    /// whose `target_match` matches the build target. See Phase 148.
    #[serde(default, deserialize_with = "deserialize_arch_field")]
    pub arch: Vec<String>,
    /// Cross-compile compile-rs options (opt_level, warnings,
    /// extra cflags).
    #[serde(default)]
    pub compile: CompileSettings,
    /// `cc::Build::pic(bool)` override (NuttX flat builds use
    /// `false`; POSIX leaves the cc-rs default).
    #[serde(default)]
    pub pic: Option<bool>,
    /// Rerun-if-env-changed env vars to register beyond
    /// `required_env`. Set for env-gated debug knobs etc.
    #[serde(default)]
    pub rerun_if_env_changed: Vec<String>,
}

/// `[arch.<name>]` block — reusable target-arch compiler-flag
/// profile shared across platforms.
#[derive(Debug, Default, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ArchEntry {
    /// Substring that must be in the target triple for the arch
    /// block to apply.
    #[serde(default)]
    pub target_match: Option<String>,
    /// Substring that, if present in the target triple, vetoes
    /// this arch block. Used to disambiguate Cortex-M3 (thumbv7m)
    /// from Cortex-M4 (thumbv7em).
    #[serde(default)]
    pub target_exclude: Option<String>,
    /// Compiler flags appended to `cc::Build`.
    #[serde(default)]
    pub cflags: Vec<String>,
    /// Whether the build should add the picolibc sysroot's
    /// `include/` to the search path (RISC-V bare-metal).
    #[serde(default)]
    pub needs_picolibc: bool,
    /// Whether the build should generate + prepend the
    /// errno-override shadow header (RISC-V picolibc TLS-errno
    /// workaround).
    #[serde(default)]
    pub needs_errno_override: bool,
    /// Whether the build needs `detect_riscv_compiler` cross-cc
    /// probe (cargo doesn't auto-set CC for bare-metal RISC-V).
    #[serde(default)]
    pub needs_riscv_compiler: bool,
}

/// Per-link-feature override declared in `zenoh_platforms.toml`.
/// `bool` collapses to On / Off; a string like `"feature"` defers
/// to the matching `CARGO_FEATURE_*` env var. Distinct from the
/// build-script `LinkPolicy` struct in `build/policy.rs`.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum LinkOverride {
    On(bool),
    Mode(String),
}

/// Env-var-backed define: value comes from `env`, falls back to
/// `default` literal.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct EnvDefault {
    pub env: String,
    pub default: String,
}

/// Extra C source compiled into the zenoh-pico archive.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ExtraSource {
    /// Interpolated path (`{nros}` / `{src}` / `{out}` /
    /// `{env:VAR}`).
    pub path: String,
    /// If set, only include when the named env var is present.
    #[serde(default)]
    pub if_env: Option<String>,
    /// If set, `cc::Build::define(name, Some(value))` whenever
    /// this source is included.
    #[serde(default)]
    pub with_define: Option<Vec<String>>,
}

/// One required env var.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct RequiredEnv {
    pub name: String,
    pub help: String,
    /// Optional sub-directory that must exist under the env's
    /// value for the build to proceed (loud panic otherwise).
    #[serde(default)]
    pub validate_subdir: Option<String>,
}

/// One conditional include path.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ConditionalPath {
    /// Interpolated path.
    pub path: String,
    /// Matcher table; see `WhenMatcher`.
    pub when: WhenMatcher,
}

/// Gate that decides whether a conditional item applies.
/// Forms (`target_match` / `target_not` / `if_env`) compose: each
/// non-`None` field must match for the matcher to return `true`.
#[derive(Debug, Default, Deserialize, Clone)]
#[allow(dead_code)]
pub struct WhenMatcher {
    /// Substring that must appear in the target triple.
    #[serde(default)]
    pub target_match: Option<String>,
    /// Substring that must NOT appear in the target triple.
    /// Special value `"embedded"` means "target_os is one of the
    /// known embedded RTOSes". Build-script consumer expands.
    #[serde(default)]
    pub target_not: Option<String>,
    /// Env var that must be set (any value).
    #[serde(default)]
    pub if_env: Option<String>,
}

/// `cc::Build` compile settings.
#[derive(Debug, Default, Deserialize, Clone)]
#[allow(dead_code)]
pub struct CompileSettings {
    pub opt_level: Option<u32>,
    #[serde(default)]
    pub warnings: Option<bool>,
    #[serde(default)]
    pub cflags: Vec<String>,
}

/// Resolved view of one platform after `inherits` chain merge.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ResolvedPlatform {
    pub name: String,
    pub defines: Vec<String>,
    pub defines_kv: BTreeMap<String, String>,
    pub defines_env: BTreeMap<String, EnvDefault>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub system_libs: Vec<String>,
    pub mbedtls: Option<String>,
    pub link: BTreeMap<String, LinkOverride>,
    pub extra_sources: Vec<ExtraSource>,
    pub required_env: Vec<RequiredEnv>,
    pub include_paths: Vec<String>,
    pub include_paths_conditional: Vec<ConditionalPath>,
    pub arch: Vec<String>,
    pub compile: CompileSettings,
    pub pic: Option<bool>,
    pub rerun_if_env_changed: Vec<String>,
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

    /// Parse the manifest from an in-memory TOML string.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        toml::from_str(text).map_err(ManifestError::Parse)
    }

    /// Resolve one `[platform.<name>]` block, walking the
    /// `inherits` chain. Child fields win when both parent and
    /// child set the same key; list-shaped fields are unioned
    /// (parent first, then child); maps merge per-key with child
    /// override.
    pub fn for_platform(&self, name: &str) -> Result<ResolvedPlatform, ManifestError> {
        let mut seen = std::collections::BTreeSet::new();
        let entry = self.resolve(name, &mut seen)?;
        Ok(ResolvedPlatform {
            name: name.to_string(),
            defines: entry.defines,
            defines_kv: entry.defines_kv,
            defines_env: entry.defines_env,
            include: entry.include,
            exclude: entry.exclude,
            system_libs: entry.system_libs,
            mbedtls: entry.mbedtls,
            link: entry.link,
            extra_sources: entry.extra_sources,
            required_env: entry.required_env,
            include_paths: entry.include_paths,
            include_paths_conditional: entry.include_paths_conditional,
            arch: entry.arch,
            compile: entry.compile,
            pic: entry.pic,
            rerun_if_env_changed: entry.rerun_if_env_changed,
        })
    }

    /// Look up an `[arch.*]` block by name.
    #[allow(dead_code)]
    pub fn arch_for(&self, name: &str) -> Option<&ArchEntry> {
        self.arch.get(name)
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
    defines.append(&mut child.defines);
    let mut defines_kv = parent.defines_kv;
    defines_kv.extend(std::mem::take(&mut child.defines_kv));
    let mut defines_env = parent.defines_env;
    defines_env.extend(std::mem::take(&mut child.defines_env));
    let mut include = parent.include;
    include.append(&mut child.include);
    let mut exclude = parent.exclude;
    exclude.append(&mut child.exclude);
    let mut system_libs = parent.system_libs;
    system_libs.append(&mut child.system_libs);
    let mbedtls = child.mbedtls.or(parent.mbedtls);
    let mut link = parent.link;
    link.extend(std::mem::take(&mut child.link));
    let mut extra_sources = parent.extra_sources;
    extra_sources.append(&mut child.extra_sources);
    let mut required_env = parent.required_env;
    required_env.append(&mut child.required_env);
    let mut include_paths = parent.include_paths;
    include_paths.append(&mut child.include_paths);
    let mut include_paths_conditional = parent.include_paths_conditional;
    include_paths_conditional.append(&mut child.include_paths_conditional);
    // Child's arch list overrides parent's when non-empty; otherwise
    // inherit. Mirrors the Option<String>.or semantics now extended
    // to multi-arch platforms (Phase 148).
    let arch = if child.arch.is_empty() {
        parent.arch
    } else {
        child.arch
    };
    let compile = CompileSettings {
        opt_level: child.compile.opt_level.or(parent.compile.opt_level),
        warnings: child.compile.warnings.or(parent.compile.warnings),
        cflags: {
            let mut c = parent.compile.cflags;
            c.extend(child.compile.cflags);
            c
        },
    };
    let pic = child.pic.or(parent.pic);
    let mut rerun_if_env_changed = parent.rerun_if_env_changed;
    rerun_if_env_changed.append(&mut child.rerun_if_env_changed);

    PlatformEntry {
        inherits: None,
        defines,
        defines_kv,
        defines_env,
        include,
        exclude,
        system_libs,
        mbedtls,
        link,
        extra_sources,
        required_env,
        include_paths,
        include_paths_conditional,
        arch,
        compile,
        pic,
        rerun_if_env_changed,
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

// ----------------------------------------------------------------
// Interpolation + matcher (consumed by build.rs's unified driver).
// ----------------------------------------------------------------

/// Tokens available for interpolation in any `path` / `defines_env`
/// field. Build-script populates this context once + threads it
/// through.
#[allow(dead_code)]
pub struct InterpContext<'a> {
    /// `CARGO_MANIFEST_DIR` (`zpico-sys/`).
    pub nros: &'a Path,
    /// `OUT_DIR`.
    pub out: &'a Path,
    /// `zenoh-pico/src` (relative to `nros`).
    pub src: &'a Path,
}

/// Replace every `{nros}` / `{out}` / `{src}` / `{env:VAR}` token
/// in `input`. Missing env vars produce `None` so the caller can
/// emit a helpful panic.
#[allow(dead_code)]
pub fn interpolate(input: &str, ctx: &InterpContext<'_>) -> Result<String, InterpError> {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    loop {
        let Some(start) = rest.find('{') else {
            out.push_str(rest);
            return Ok(out);
        };
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else {
            return Err(InterpError::UnterminatedToken(input.to_string()));
        };
        let token = &rest[..end];
        rest = &rest[end + 1..];
        let value: String = if token == "nros" {
            ctx.nros.display().to_string()
        } else if token == "out" {
            ctx.out.display().to_string()
        } else if token == "src" {
            ctx.src.display().to_string()
        } else if let Some(var) = token.strip_prefix("env:") {
            std::env::var(var).map_err(|_| InterpError::MissingEnv(var.to_string()))?
        } else {
            return Err(InterpError::UnknownToken(token.to_string()));
        };
        out.push_str(&value);
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum InterpError {
    UnknownToken(String),
    UnterminatedToken(String),
    MissingEnv(String),
}

impl std::fmt::Display for InterpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownToken(t) => write!(f, "unknown interpolation token `{{{t}}}`"),
            Self::UnterminatedToken(s) => write!(f, "unterminated `{{` in `{s}`"),
            Self::MissingEnv(v) => write!(f, "env var `{v}` not set"),
        }
    }
}

impl std::error::Error for InterpError {}

/// Returns `true` when every populated field in `m` matches the
/// current target / env state. Empty matcher = always true.
/// `target_not == "embedded"` is the special-case "target_os is
/// one of the known RTOSes" gate; build-script supplies the
/// `is_embedded` flag pre-computed.
#[allow(dead_code)]
pub fn matches(m: &WhenMatcher, target: &str, is_embedded: bool) -> bool {
    if let Some(needle) = m.target_match.as_deref()
        && !match_target(target, needle)
    {
        return false;
    }
    if let Some(needle) = m.target_not.as_deref() {
        let hit = if needle == "embedded" {
            is_embedded
        } else {
            match_target(target, needle)
        };
        if hit {
            return false;
        }
    }
    if let Some(var) = m.if_env.as_deref()
        && std::env::var(var).is_err()
    {
        return false;
    }
    true
}

/// Match a target-triple needle. Supports trailing `*` glob:
/// `riscv64-*` matches anything starting with `riscv64-`.
fn match_target(target: &str, needle: &str) -> bool {
    if let Some(prefix) = needle.strip_suffix('*') {
        target.starts_with(prefix)
    } else {
        target.contains(needle)
    }
}

// Note: the loader is exercised at build time — `build.rs` parses
// `zenoh_platforms.toml` + resolves every platform on every cargo
// build. A typo, broken `inherits` chain, or shape regression
// surfaces as a build-script panic. There is no separate
// `#[test]`-style suite because `cargo test` doesn't link build
// scripts; the build-time invariant is the real gate.
