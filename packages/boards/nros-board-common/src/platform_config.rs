//! RFC-0049 / phase-290 — per-package hierarchical platform/board
//! configuration.
//!
//! One `nros-platform.toml` per platform package directory
//! (`packages/platforms/<name>/`, or an out-of-tree dir named via
//! `NROS_PLATFORMS_DIR` + the platform name), carrying:
//!
//! ```toml
//! inherits = "generic"        # optional family chain (sibling dir name)
//!
//! [capabilities]              # software-stack FACTS (open vocabulary)
//! threads = true
//! per_fd_tx_ceiling = true
//!
//! [knobs.zenoh.tx]            # policy defaults (typed, deny_unknown_fields)
//! batch = true
//! split_lock = true
//! flush_ms = 50
//!
//! [build.zenoh]               # the former zenoh_platforms.toml block,
//! defines = ["ZENOH_GENERIC"] # keys verbatim (RFC-0049 open question 1:
//! # ...                       # verbatim relocation)
//!
//! [arch.cortex-m3]            # reusable compiler-flag profiles; may be
//! # ...                       # duplicated across files if byte-identical
//! ```
//!
//! Board packages carry the same `[capabilities]` / `[knobs.*]` tables in
//! their existing `nros-board.toml` (RFC-0042 descriptor) as deltas.
//!
//! Resolution ladder (RFC-0004 style — fixed, not an open merge):
//!
//! ```text
//! built-in default < platform file(s, via inherits) < board file < env
//! ```
//!
//! Env front-ends are tri-state: unset = defer to the ladder below; set
//! (including explicit `0`) = override. Every resolved knob remembers which
//! rung set it (`KnobSource`) so `nros config explain` can print the ladder.
//!
//! The schema home is this crate rather than `nros-platform` (the RFC's
//! first draft): `nros-platform` is a `no_std` runtime crate, while this
//! module is build-time tooling next to the existing manifest parser it
//! builds on.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::manifest::{ArchEntry, ManifestError, PlatformEntry, PlatformManifest};

/// Filename of the per-platform-package config file.
pub const PLATFORM_CONFIG_FILENAME: &str = "nros-platform.toml";

/// One `nros-platform.toml` file, parsed.
///
/// Every section is optional — an absent/empty file is valid and yields
/// pure built-in behavior (the byte-identity guarantee phase-290 W2.c
/// regression-tests).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformConfigFile {
    /// Optional parent platform (sibling directory name). The parent's
    /// `[build.zenoh]`, `[capabilities]` and `[knobs]` merge underneath
    /// this file's values.
    #[serde(default)]
    pub inherits: Option<String>,
    /// Software-stack facts. Open vocabulary by design — facts are
    /// consumed by name in capability checks; policy (knobs) is the
    /// typed, closed part of the schema.
    #[serde(default)]
    pub capabilities: BTreeMap<String, bool>,
    #[serde(default)]
    pub knobs: Knobs,
    #[serde(default)]
    pub build: BuildSection,
    /// Reusable compiler-flag profiles. Files may repeat a profile
    /// (e.g. `cortex-m3` in both `bare-metal` and `freertos-lwip`)
    /// only if the copies are identical; conflicting redefinition is a
    /// load error.
    #[serde(default)]
    pub arch: BTreeMap<String, ArchEntry>,
}

/// `[build.*]` — per-vendored-component build blocks.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSection {
    /// The former `zenoh_platforms.toml` `[platform.<name>]` block,
    /// keys verbatim (`defines`, `defines_kv`, `include`,
    /// `extra_sources`, `arch`, `compile`, …).
    #[serde(default)]
    pub zenoh: Option<PlatformEntry>,
}

/// `[knobs]` — typed policy. `zenoh.tx` is the first tenant
/// (phase-282); future tenants (`executor`, `log`, ring depths, …) are
/// additive fields here.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knobs {
    #[serde(default)]
    pub zenoh: ZenohKnobs,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZenohKnobs {
    #[serde(default)]
    pub tx: TxKnobs,
}

/// The phase-282 TX levers. All optional — `None` means "defer to the
/// rung below".
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TxKnobs {
    pub batch: Option<bool>,
    pub split_lock: Option<bool>,
    pub flush_ms: Option<u64>,
}

/// Which ladder rung produced a resolved value (for
/// `nros config explain` + capability-check diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobSource {
    Builtin,
    Platform,
    Board,
    Env,
}

impl KnobSource {
    pub fn as_str(self) -> &'static str {
        match self {
            KnobSource::Builtin => "builtin",
            KnobSource::Platform => "platform",
            KnobSource::Board => "board",
            KnobSource::Env => "env",
        }
    }
}

/// A resolved knob value + the rung that set it.
#[derive(Debug, Clone, Copy)]
pub struct Resolved<T> {
    pub value: T,
    pub source: KnobSource,
}

/// The fully-resolved `zenoh.tx` knob set.
#[derive(Debug, Clone)]
pub struct ResolvedTxKnobs {
    pub batch: Resolved<bool>,
    pub split_lock: Resolved<bool>,
    pub flush_ms: Resolved<u64>,
}

/// Built-in defaults — level 1 of the ladder. MUST equal the historical
/// hardcoded env defaults so an empty tree changes nothing (W2.c).
pub const BUILTIN_TX_BATCH: bool = false;
pub const BUILTIN_TX_SPLIT_LOCK: bool = false;
pub const BUILTIN_TX_FLUSH_MS: u64 = 50;

/// A loaded tree of platform config files (`<root>/<name>/nros-platform.toml`).
#[derive(Debug, Default)]
pub struct PlatformsTree {
    files: BTreeMap<String, PlatformConfigFile>,
    /// Merged `[arch.*]` table across all files (identical duplicates
    /// tolerated).
    arch: BTreeMap<String, ArchEntry>,
    root: PathBuf,
}

/// Load / resolution errors. `Manifest` wraps the underlying shared
/// parser's error type for the `[build.zenoh]` payload.
#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: String,
        source: std::io::Error,
    },
    Parse {
        path: String,
        source: toml::de::Error,
    },
    Manifest(ManifestError),
    UnknownPlatform {
        name: String,
        root: String,
    },
    InheritsCycle {
        name: String,
    },
    ArchConflict {
        name: String,
        file_a: String,
        file_b: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io { path, source } => write!(f, "{path}: {source}"),
            ConfigError::Parse { path, source } => write!(f, "{path}: {source}"),
            ConfigError::Manifest(e) => write!(f, "{e}"),
            ConfigError::UnknownPlatform { name, root } => write!(
                f,
                "unknown platform `{name}`: no {root}/{name}/{PLATFORM_CONFIG_FILENAME}"
            ),
            ConfigError::InheritsCycle { name } => {
                write!(f, "platform `{name}`: `inherits` cycle")
            }
            ConfigError::ArchConflict {
                name,
                file_a,
                file_b,
            } => write!(
                f,
                "arch profile `{name}` defined differently in {file_a} and {file_b} \
                 — profiles duplicated across platform files must be identical"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<ManifestError> for ConfigError {
    fn from(e: ManifestError) -> Self {
        ConfigError::Manifest(e)
    }
}

impl PlatformsTree {
    /// Load every `<root>/*/nros-platform.toml`. Directories without the
    /// file are skipped (a platform package may predate its config file).
    pub fn load(root: &Path) -> Result<Self, ConfigError> {
        let mut tree = PlatformsTree {
            root: root.to_path_buf(),
            ..Default::default()
        };
        let entries = fs::read_dir(root).map_err(|e| ConfigError::Io {
            path: root.display().to_string(),
            source: e,
        })?;
        let mut arch_origin: BTreeMap<String, String> = BTreeMap::new();
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let file = dir.join(PLATFORM_CONFIG_FILENAME);
            if !file.is_file() {
                continue;
            }
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let text = fs::read_to_string(&file).map_err(|e| ConfigError::Io {
                path: file.display().to_string(),
                source: e,
            })?;
            let parsed: PlatformConfigFile =
                toml::from_str(&text).map_err(|e| ConfigError::Parse {
                    path: file.display().to_string(),
                    source: e,
                })?;
            for (arch_name, entry) in &parsed.arch {
                match tree.arch.get(arch_name) {
                    None => {
                        tree.arch.insert(arch_name.clone(), entry.clone());
                        arch_origin.insert(arch_name.clone(), name.clone());
                    }
                    Some(existing) => {
                        // Identical duplicates tolerated (shared profiles
                        // like cortex-m3); conflicting redefinition is a
                        // drift bug.
                        if format!("{existing:?}") != format!("{entry:?}") {
                            return Err(ConfigError::ArchConflict {
                                name: arch_name.clone(),
                                file_a: arch_origin.get(arch_name).cloned().unwrap_or_default(),
                                file_b: name.clone(),
                            });
                        }
                    }
                }
            }
            tree.files.insert(name, parsed);
        }
        Ok(tree)
    }

    /// Platform names present in the tree.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// The merged `[arch.*]` table.
    pub fn arch_table(&self) -> &BTreeMap<String, ArchEntry> {
        &self.arch
    }

    /// Assemble the legacy [`PlatformManifest`] view (the `[build.zenoh]`
    /// payloads keyed by platform name, `inherits` preserved) so the
    /// existing `for_platform` inheritance/merge logic — and every
    /// downstream consumer of `ResolvedPlatform` — keeps working
    /// unchanged.
    pub fn as_platform_manifest(&self) -> PlatformManifest {
        let mut platform = BTreeMap::new();
        for (name, file) in &self.files {
            let mut entry = file.build.zenoh.clone().unwrap_or_default();
            // `inherits` lives at file top level in the new format; the
            // legacy resolver reads it from the entry.
            if entry.inherits.is_none() {
                entry.inherits = file.inherits.clone();
            }
            platform.insert(name.clone(), entry);
        }
        PlatformManifest {
            platform,
            arch: self.arch.clone(),
        }
    }

    /// Walk the `inherits` chain (child-first list: `[name, parent, …]`).
    fn chain(&self, name: &str) -> Result<Vec<&PlatformConfigFile>, ConfigError> {
        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut cur = Some(name.to_string());
        while let Some(n) = cur {
            if !seen.insert(n.clone()) {
                return Err(ConfigError::InheritsCycle { name: n });
            }
            let file = self
                .files
                .get(&n)
                .ok_or_else(|| ConfigError::UnknownPlatform {
                    name: n.clone(),
                    root: self.root.display().to_string(),
                })?;
            cur = file.inherits.clone();
            out.push(file);
        }
        Ok(out)
    }

    /// Resolve one platform's capabilities (inherits-merged, child wins).
    pub fn capabilities(&self, name: &str) -> Result<BTreeMap<String, bool>, ConfigError> {
        let chain = self.chain(name)?;
        let mut caps = BTreeMap::new();
        // Parent-first application so the child overrides.
        for file in chain.iter().rev() {
            for (k, v) in &file.capabilities {
                caps.insert(k.clone(), *v);
            }
        }
        Ok(caps)
    }

    /// Resolve one platform's `[knobs]` (inherits-merged, child wins,
    /// field-level).
    fn platform_tx_knobs(&self, name: &str) -> Result<TxKnobs, ConfigError> {
        let chain = self.chain(name)?;
        let mut tx = TxKnobs::default();
        for file in chain.iter().rev() {
            let t = &file.knobs.zenoh.tx;
            if t.batch.is_some() {
                tx.batch = t.batch;
            }
            if t.split_lock.is_some() {
                tx.split_lock = t.split_lock;
            }
            if t.flush_ms.is_some() {
                tx.flush_ms = t.flush_ms;
            }
        }
        Ok(tx)
    }

    /// Resolve the `zenoh.tx` knob set for `platform`, applying the full
    /// ladder: builtin < platform < `board` deltas < `env` overrides.
    ///
    /// `env` is an accessor (injected for tests): `env(name)` returns the
    /// raw env value if SET (tri-state front-end — a set `"0"` overrides
    /// an on-default).
    pub fn resolve_tx(
        &self,
        platform: &str,
        board: Option<&TxKnobs>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<ResolvedTxKnobs, ConfigError> {
        let plat = self.platform_tx_knobs(platform)?;

        fn rung<T: Copy>(builtin: T, plat: Option<T>, board: Option<T>) -> (T, KnobSource) {
            match (board, plat) {
                (Some(v), _) => (v, KnobSource::Board),
                (None, Some(v)) => (v, KnobSource::Platform),
                (None, None) => (builtin, KnobSource::Builtin),
            }
        }

        let (mut batch, mut batch_src) =
            rung(BUILTIN_TX_BATCH, plat.batch, board.and_then(|b| b.batch));
        let (mut split, mut split_src) = rung(
            BUILTIN_TX_SPLIT_LOCK,
            plat.split_lock,
            board.and_then(|b| b.split_lock),
        );
        let (mut flush, mut flush_src) = rung(
            BUILTIN_TX_FLUSH_MS,
            plat.flush_ms,
            board.and_then(|b| b.flush_ms),
        );

        // Env front-end — top rung, tri-state.
        if let Some(v) = env("ZPICO_TX_BATCH") {
            batch = v.trim().parse::<u64>().map(|n| n != 0).unwrap_or(false);
            batch_src = KnobSource::Env;
        }
        if let Some(v) = env("ZPICO_TX_SPLIT_LOCK") {
            split = v.trim().parse::<u64>().map(|n| n != 0).unwrap_or(false);
            split_src = KnobSource::Env;
        }
        if let Some(v) = env("ZPICO_TX_BATCH_FLUSH_MS")
            && let Ok(n) = v.trim().parse::<u64>()
        {
            flush = n;
            flush_src = KnobSource::Env;
        }

        Ok(ResolvedTxKnobs {
            batch: Resolved {
                value: batch,
                source: batch_src,
            },
            split_lock: Resolved {
                value: split,
                source: split_src,
            },
            flush_ms: Resolved {
                value: flush,
                source: flush_src,
            },
        })
    }

    /// RFC-0049 capability cross-check: policy that contradicts fact is
    /// downgraded, never silently shipped. Returns warning lines (the
    /// build script prints them as `cargo:warning=`).
    pub fn capability_check(
        &self,
        platform: &str,
        tx: &mut ResolvedTxKnobs,
    ) -> Result<Vec<String>, ConfigError> {
        let caps = self.capabilities(platform)?;
        let mut warnings = Vec::new();
        let threads = caps.get("threads").copied().unwrap_or(false);
        if tx.split_lock.value && !threads {
            warnings.push(format!(
                "platform `{platform}`: knobs.zenoh.tx.split_lock = true (from {}) but \
                 capabilities.threads is not true — split locking needs a flush thread; \
                 downgrading split_lock to off",
                tx.split_lock.source.as_str()
            ));
            tx.split_lock = Resolved {
                value: false,
                source: KnobSource::Builtin,
            };
        }
        Ok(warnings)
    }
}

/// One board package's knob deltas, read from the `[knobs]` table of its
/// existing `nros-board.toml` (RFC-0042 descriptor — the rest of that
/// file is parsed elsewhere and unknown keys there are NOT this module's
/// concern, so this parse is deliberately tolerant of sibling tables).
#[derive(Debug, Default, Deserialize)]
pub struct BoardKnobsFile {
    #[serde(default)]
    pub capabilities: BTreeMap<String, bool>,
    #[serde(default)]
    pub knobs: Knobs,
    // The rest of nros-board.toml (board descriptor tables) is ignored
    // here — parsed by the board registry, not the knob ladder.
    #[serde(flatten)]
    _rest: BTreeMap<String, toml::Value>,
}

impl BoardKnobsFile {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path).map_err(|e| ConfigError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        toml::from_str(&text).map_err(|e| ConfigError::Parse {
            path: path.display().to_string(),
            source: e,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (name, body) in files {
            let dir = tmp.path().join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(PLATFORM_CONFIG_FILENAME), body).unwrap();
        }
        tmp
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn empty_tree_yields_builtins() {
        let tmp = write_tree(&[("zephyr", "")]);
        let tree = PlatformsTree::load(tmp.path()).unwrap();
        let tx = tree.resolve_tx("zephyr", None, &no_env).unwrap();
        assert!(!tx.batch.value);
        assert_eq!(tx.batch.source, KnobSource::Builtin);
        assert_eq!(tx.flush_ms.value, BUILTIN_TX_FLUSH_MS);
    }

    #[test]
    fn ladder_platform_board_env_order() {
        let tmp = write_tree(&[(
            "zephyr",
            "[capabilities]\nthreads = true\n[knobs.zenoh.tx]\nbatch = true\nflush_ms = 40\n",
        )]);
        let tree = PlatformsTree::load(tmp.path()).unwrap();

        // platform rung
        let tx = tree.resolve_tx("zephyr", None, &no_env).unwrap();
        assert!(tx.batch.value);
        assert_eq!(tx.batch.source, KnobSource::Platform);
        assert_eq!(tx.flush_ms.value, 40);

        // board rung overrides platform
        let board = TxKnobs {
            batch: Some(false),
            split_lock: None,
            flush_ms: None,
        };
        let tx = tree.resolve_tx("zephyr", Some(&board), &no_env).unwrap();
        assert!(!tx.batch.value);
        assert_eq!(tx.batch.source, KnobSource::Board);
        assert_eq!(tx.flush_ms.value, 40); // untouched delta falls through

        // env rung overrides board — including explicit re-enable
        let env = |k: &str| (k == "ZPICO_TX_BATCH").then(|| "1".to_string());
        let tx = tree.resolve_tx("zephyr", Some(&board), &env).unwrap();
        assert!(tx.batch.value);
        assert_eq!(tx.batch.source, KnobSource::Env);
    }

    #[test]
    fn explicit_env_zero_overrides_on_default() {
        let tmp = write_tree(&[(
            "zephyr",
            "[knobs.zenoh.tx]\nbatch = true\nsplit_lock = true\n",
        )]);
        let tree = PlatformsTree::load(tmp.path()).unwrap();
        let env = |k: &str| (k == "ZPICO_TX_BATCH").then(|| "0".to_string());
        let tx = tree.resolve_tx("zephyr", None, &env).unwrap();
        assert!(!tx.batch.value, "set env 0 must beat an on-default");
        assert_eq!(tx.batch.source, KnobSource::Env);
        assert!(
            tx.split_lock.value,
            "untouched knob keeps the platform rung"
        );
    }

    #[test]
    fn inherits_chain_merges_child_wins() {
        let tmp = write_tree(&[
            (
                "generic",
                "[capabilities]\nthreads = true\n[knobs.zenoh.tx]\nflush_ms = 30\n",
            ),
            (
                "orin-spe",
                "inherits = \"generic\"\n[knobs.zenoh.tx]\nflush_ms = 60\n",
            ),
        ]);
        let tree = PlatformsTree::load(tmp.path()).unwrap();
        let tx = tree.resolve_tx("orin-spe", None, &no_env).unwrap();
        assert_eq!(tx.flush_ms.value, 60);
        let caps = tree.capabilities("orin-spe").unwrap();
        assert_eq!(caps.get("threads"), Some(&true));
    }

    #[test]
    fn unknown_knob_key_fails_loud() {
        let tmp = write_tree(&[("zephyr", "[knobs.zenoh.tx]\nbatchh = true\n")]);
        let err = PlatformsTree::load(tmp.path()).unwrap_err();
        assert!(format!("{err}").contains("batchh"), "{err}");
    }

    #[test]
    fn capability_check_downgrades_split_without_threads() {
        let tmp = write_tree(&[(
            "bare-metal",
            "[knobs.zenoh.tx]\nbatch = true\nsplit_lock = true\n",
        )]);
        let tree = PlatformsTree::load(tmp.path()).unwrap();
        let mut tx = tree.resolve_tx("bare-metal", None, &no_env).unwrap();
        let warnings = tree.capability_check("bare-metal", &mut tx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(!tx.split_lock.value);
        assert!(tx.batch.value, "batch itself survives (spin-driven flush)");
    }

    #[test]
    fn arch_identical_duplicate_ok_conflict_errors() {
        let arch = "[arch.cortex-m3]\ntarget_match = \"thumbv7m\"\n";
        let tmp = write_tree(&[("a", arch), ("b", arch)]);
        assert!(PlatformsTree::load(tmp.path()).is_ok());

        let tmp = write_tree(&[
            ("a", arch),
            ("b", "[arch.cortex-m3]\ntarget_match = \"thumbv7em\"\n"),
        ]);
        assert!(matches!(
            PlatformsTree::load(tmp.path()),
            Err(ConfigError::ArchConflict { .. })
        ));
    }

    #[test]
    fn legacy_manifest_view_resolves_build_zenoh() {
        let tmp = write_tree(&[
            ("generic", "[build.zenoh]\ndefines = [\"A\"]\n"),
            (
                "orin-spe",
                "inherits = \"generic\"\n[build.zenoh]\ndefines = [\"B\"]\n",
            ),
        ]);
        let tree = PlatformsTree::load(tmp.path()).unwrap();
        let manifest = tree.as_platform_manifest();
        let resolved = manifest.for_platform("orin-spe").unwrap();
        assert_eq!(resolved.defines, vec!["A".to_string(), "B".to_string()]);
    }
}
