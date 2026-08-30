//! RFC-0049 / phase-290 — per-package hierarchical platform/board
//! configuration.
//!
//! One `nros-platform.toml` per platform package directory
//! (`config/<name>/`, or an out-of-tree dir named via
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
    /// phase-349 W1 / RFC-0072 — the names this platform answers to.
    ///
    /// Empty (the default) means "just my directory name", which is what every
    /// file meant before this existed. The first entry is canonical; the rest
    /// are aliases, which is how `freertos-lwip` keeps resolving after the
    /// directory became `freertos`.
    ///
    /// The stack does not belong in a platform's identity — zenoh-pico itself
    /// splits `system/freertos/{system.c, lwip/network.c,
    /// freertos_plus_tcp/network.c}` — so `freertos-lwip` is an alias to retire,
    /// not a name to keep. Matching the rmw and board descriptors also lets
    /// `check-provider-announcements.py` compare provisions against it with one
    /// more `FAMILIES` row rather than a new rule.
    #[serde(default)]
    pub names: Vec<String>,

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

/// `[build.*]` — per-vendored-component build blocks, keyed by COMPONENT NAME.
///
/// phase-347 W6 — this was `struct BuildSection { zenoh: Option<PlatformEntry> }`
/// with `deny_unknown_fields`, so `[build.cyclonedds]` was not merely absent, it
/// was REJECTED: a platform could describe exactly one backend's vendored C
/// build, and the one it could describe was named in core.
///
/// Only the KEY was ever backend-specific. `PlatformEntry` carries `defines`,
/// `include`, `extra_sources`, `arch`, `compile` … — generic vendored-library
/// build config with no zenoh-shaped field, already proven across the seven
/// `config/*/nros-platform.toml` files. So this is a keying change, not a schema
/// design: `[build.zenoh]` parses as the key `"zenoh"` and **none of those seven
/// files change**.
///
/// The second tenant is not hypothetical: `nros-rmw-xrce-cffi/build.rs` is ~500
/// lines hardcoding this same shape (`_DEFAULT_SOURCE`, `_POSIX_C_SOURCE`,
/// posix/embedded branching, a generated config header) because there was
/// nowhere to declare it.
pub type BuildSection = BTreeMap<String, PlatformEntry>;

/// `[knobs]` — typed policy. `zenoh.tx` is the first tenant
/// (phase-282); future tenants (`executor`, `log`, ring depths, …) are
/// additive fields here.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knobs {
    #[serde(default)]
    pub zenoh: ZenohKnobs,
    /// phase-400 W3 / RFC-0086 D1 — the transport tenant.
    ///
    /// The first knob that crosses all three descriptor axes: the backend knows
    /// how to SPELL an endpoint, the platform knows whether an IP stack EXISTS,
    /// the board knows which peripheral is WIRED. Stating it once is what lets
    /// the resolver derive the link and driver settings that a transport choice
    /// implies, instead of each image hand-writing them.
    #[serde(default)]
    pub transport: TransportKnobs,
}

/// `[knobs.transport]` — RFC-0086 D1.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportKnobs {
    /// `exactly-one-of` serial | tcp | udp. `None` defers to the rung below.
    pub kind: Option<String>,
    /// Opaque to this layer: the BACKEND lowers it into its own locator
    /// spelling, and the BOARD resolves a peripheral name. Carried, not parsed.
    pub endpoint: Option<String>,
}

/// The transports the resolver knows how to constrain. RFC-0086 D2 makes this
/// an `exactly-one-of` group, which is why an unknown value is an error rather
/// than a pass-through: a typo that silently selects nothing would leave every
/// implication unapplied and the image would build with the wrong links on.
pub const TRANSPORT_KINDS: &[&str] = &["serial", "tcp", "udp"];

/// Built-in default — level 1. `tcp` preserves the behaviour of every image
/// that predates this tenant, where `NROS_ZENOH_LINK_TCP` was `default y`.
pub const BUILTIN_TRANSPORT_KIND: &str = "tcp";

/// One knob set by an implication rather than by a ladder rung.
///
/// RFC-0086 D2: `implies` is Kconfig `imply` strength, NEVER `select` — a
/// higher rung still wins, and when it does the override is recorded here so
/// `nros config explain` can report it. A forcing verb would let
/// `transport.kind = "serial"` silently stamp out an explicitly requested TCP
/// link, which is the failure mode Kconfig's own documentation records for
/// `select`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Implied {
    /// Dotted knob name, e.g. `links.tcp` or `drivers.ethernet`.
    pub knob: String,
    /// The value the implication asks for.
    pub value: bool,
    /// The rule that asked, for the explain output.
    pub rule: String,
    /// Set when a higher rung contradicted the implication. The implication
    /// LOSES; this records that it happened.
    pub overridden_by: Option<KnobSource>,
}

/// The fully-resolved transport tenant.
#[derive(Debug, Clone)]
pub struct ResolvedTransport {
    pub kind: Resolved<String>,
    pub endpoint: Resolved<Option<String>>,
    /// Every knob this transport choice implies, in declaration order.
    pub implied: Vec<Implied>,
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
    /// phase-400 W3 — `transport.kind` outside the `exactly-one-of` group.
    UnknownTransport {
        platform: String,
        kind: String,
        known: String,
    },
    /// phase-400 W3 — `requires` failed: the transport needs a capability the
    /// platform does not declare. Named at CONFIG time, which is the whole
    /// point: the alternative is a link error against `AF_INET` much later.
    TransportCapabilityMissing {
        platform: String,
        kind: String,
        capability: String,
        source: String,
    },
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
            ConfigError::UnknownTransport {
                platform,
                kind,
                known,
            } => write!(
                f,
                "platform `{platform}`: knobs.transport.kind = `{kind}` is not one of: {known}"
            ),
            ConfigError::TransportCapabilityMissing {
                platform,
                kind,
                capability,
                source,
            } => write!(
                f,
                "platform `{platform}`: transport.kind = `{kind}` (from {source}) requires \
                 capabilities.{capability}, which this platform does not declare. Either pick a \
                 transport the platform supports, or add the capability to its nros-platform.toml \
                 if the fact is simply missing."
            ),
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
            // phase-347 W6 — `[build.<component>]` by key. Still "zenoh" here: this
            // assembles the zenoh-pico system-layer view. A second component
            // reads its own key without a schema change.
            let mut entry = file.build.get("zenoh").cloned().unwrap_or_default();
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

    /// The directory a platform NAME resolves to, following `[names]` aliases.
    ///
    /// phase-349 W1. A file's directory name always resolves to itself, so a
    /// tree whose files declare no `names` behaves exactly as before. Aliases
    /// are additive on top.
    ///
    /// Returns the input unchanged when nothing claims it, so the caller's
    /// existing "unknown platform" error still names what the user asked for
    /// rather than something this function invented.
    pub fn resolve_alias<'a>(&'a self, name: &'a str) -> &'a str {
        if self.files.contains_key(name) {
            return name;
        }
        self.files
            .iter()
            .find(|(_, f)| f.names.iter().any(|n| n == name))
            .map(|(dir, _)| dir.as_str())
            .unwrap_or(name)
    }

    /// Every name the tree answers to, directory names and aliases alike.
    /// Used to make an unknown-platform error list the real options.
    pub fn all_names(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .files
            .iter()
            .flat_map(|(dir, f)| std::iter::once(dir.clone()).chain(f.names.iter().cloned()))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Walk the `inherits` chain (child-first list: `[name, parent, …]`).
    ///
    /// Alias resolution happens HERE, at the one point every public lookup
    /// (`capabilities`, `resolve_tx`, `capability_check`) funnels through —
    /// rather than at each caller, which is how half of them would end up
    /// alias-blind.
    fn chain(&self, name: &str) -> Result<Vec<&PlatformConfigFile>, ConfigError> {
        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut cur = Some(self.resolve_alias(name).to_string());
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
            cur = file
                .inherits
                .as_deref()
                .map(|p| self.resolve_alias(p).to_string());
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

    /// Same inheritance walk as [`Self::platform_tx_knobs`], for the transport
    /// tenant: a child platform overrides its parent field by field, and an
    /// unset field defers rather than resetting to the default.
    fn platform_transport_knobs(&self, name: &str) -> Result<TransportKnobs, ConfigError> {
        let chain = self.chain(name)?;
        let mut t = TransportKnobs::default();
        for file in chain.iter().rev() {
            let f = &file.knobs.transport;
            if f.kind.is_some() {
                t.kind = f.kind.clone();
            }
            if f.endpoint.is_some() {
                t.endpoint = f.endpoint.clone();
            }
        }
        Ok(t)
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

    /// phase-400 W3 / RFC-0086 D1+D2 — resolve the transport tenant and the
    /// knobs it implies.
    ///
    /// Three verbs, with the strengths the surveyed prior art settles:
    ///
    /// * `exactly-one-of` (Gentoo `REQUIRED_USE`) — `kind` is one of
    ///   [`TRANSPORT_KINDS`]. An unrecognised value is an error, not a silent
    ///   pass-through: it would leave every implication unapplied and build an
    ///   image with the wrong links enabled.
    /// * `requires` (hard) — the platform must declare the capability the
    ///   transport needs. Failure names both the platform and the asked-for
    ///   kind, at CONFIG time, rather than surfacing later as a link error
    ///   against `AF_INET`.
    /// * `implies` (weak, Kconfig `imply` strength) — the dependent link and
    ///   driver knobs. `env_overrides` lets a higher rung win; the override is
    ///   recorded, never silently dropped.
    pub fn resolve_transport(
        &self,
        platform: &str,
        board: Option<&TransportKnobs>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<ResolvedTransport, ConfigError> {
        let plat = self.platform_transport_knobs(platform)?;

        let (kind, kind_src) = match (
            env("NROS_TRANSPORT_KIND"),
            board.and_then(|b| b.kind.clone()),
            plat.kind.clone(),
        ) {
            (Some(v), _, _) => (v.trim().to_string(), KnobSource::Env),
            (None, Some(v), _) => (v, KnobSource::Board),
            (None, None, Some(v)) => (v, KnobSource::Platform),
            (None, None, None) => (BUILTIN_TRANSPORT_KIND.to_string(), KnobSource::Builtin),
        };

        if !TRANSPORT_KINDS.contains(&kind.as_str()) {
            return Err(ConfigError::UnknownTransport {
                platform: platform.to_string(),
                kind,
                known: TRANSPORT_KINDS.join(", "),
            });
        }

        let (endpoint, endpoint_src) = match (
            env("NROS_TRANSPORT_ENDPOINT"),
            board.and_then(|b| b.endpoint.clone()),
            plat.endpoint.clone(),
        ) {
            (Some(v), _, _) => (Some(v), KnobSource::Env),
            (None, Some(v), _) => (Some(v), KnobSource::Board),
            (None, None, Some(v)) => (Some(v), KnobSource::Platform),
            (None, None, None) => (None, KnobSource::Builtin),
        };

        // `requires` — hard. A capability is a FACT declared by the platform;
        // policy that contradicts fact must not ship (RFC-0049), and for a
        // transport that means failing here rather than at link time.
        let caps = self.capabilities(platform)?;
        let needed = match kind.as_str() {
            "tcp" | "udp" => Some("ip_stack"),
            "serial" => Some("serial"),
            _ => None,
        };
        // Absent is NOT the same as false. `[capabilities]` is an open, young
        // vocabulary -- only one of the seven in-tree platform files declared
        // anything when this landed -- so an UNDECLARED fact means "nobody has
        // described this platform yet" and must not fail a build that works.
        // An EXPLICIT `false` is a described impossibility and is hard.
        //
        // The undeclared case is reported by `transport_warnings` rather than
        // swallowed, so the gap is visible and gets closed by declaration.
        if let Some(cap) = needed
            && caps.get(cap) == Some(&false)
        {
            return Err(ConfigError::TransportCapabilityMissing {
                platform: platform.to_string(),
                kind: kind.clone(),
                capability: cap.to_string(),
                source: kind_src.as_str().to_string(),
            });
        }

        // `implies` — weak. Every knob a transport choice settles, so no image
        // has to hand-write them. The env front-end still wins; when it does,
        // the implication is marked overridden rather than discarded.
        let rule = format!("transport.kind={kind}");
        let mut implied = Vec::new();
        let mut imply = |knob: &str, value: bool, env_key: &str| {
            let overridden_by = env(env_key).and_then(|v| {
                let want = v.trim().parse::<u64>().map(|n| n != 0).unwrap_or(false);
                (want != value).then_some(KnobSource::Env)
            });
            implied.push(Implied {
                knob: knob.to_string(),
                value,
                rule: rule.clone(),
                overridden_by,
            });
        };

        let serial = kind == "serial";
        imply("links.tcp", kind == "tcp", "NROS_ZENOH_LINK_TCP");
        imply("links.udp", kind == "udp", "NROS_ZENOH_LINK_UDP_UNICAST");
        imply("links.serial", serial, "NROS_ZENOH_LINK_SERIAL");
        // The MAC/MDIO/PHY drivers are `default y` behind devicetree nodes the
        // board has enabled, so they arrive on their own and must be turned off
        // by name. This is the fifteen-line hand-written block RFC-0086 exists
        // to delete.
        imply("drivers.ethernet", !serial, "NROS_DRIVER_ETHERNET");
        imply("drivers.phy", !serial, "NROS_DRIVER_PHY");
        imply("drivers.mdio", !serial, "NROS_DRIVER_MDIO");
        imply("net.ip_stack", !serial, "NROS_NET_IP_STACK");

        Ok(ResolvedTransport {
            kind: Resolved {
                value: kind,
                source: kind_src,
            },
            endpoint: Resolved {
                value: endpoint,
                source: endpoint_src,
            },
            implied,
        })
    }

    /// phase-400 W3 — facts this transport choice relied on but the platform
    /// never declared. Not an error (see `resolve_transport`), but printed, so
    /// an undescribed platform is visible rather than silently assumed.
    pub fn transport_warnings(
        &self,
        platform: &str,
        t: &ResolvedTransport,
    ) -> Result<Vec<String>, ConfigError> {
        let caps = self.capabilities(platform)?;
        let needed = match t.kind.value.as_str() {
            "tcp" | "udp" => Some("ip_stack"),
            "serial" => Some("serial"),
            _ => None,
        };
        let mut out = Vec::new();
        if let Some(cap) = needed
            && !caps.contains_key(cap)
        {
            out.push(format!(
                "platform `{platform}`: transport.kind = `{}` needs capabilities.{cap}, which \
                 this platform's nros-platform.toml does not declare either way — permitted, but \
                 the fact should be stated",
                t.kind.value
            ));
        }
        Ok(out)
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

    /// phase-347 W6 — a SECOND `[build.<component>]` key parses.
    ///
    /// This is the whole point of the keying change and the only thing that
    /// could regress it: before, `BuildSection` was a struct with one `zenoh`
    /// field and `deny_unknown_fields`, so this input was a hard parse ERROR —
    /// a platform could describe exactly one backend's vendored C build.
    ///
    /// Asserts both keys survive independently, so a future component's block
    /// cannot silently overwrite or shadow zenoh's.
    /// phase-347 W6 — the seven REAL platform files still load, and each still
    /// exposes its zenoh block under the new keying. Behaviour-preserving is
    /// the claim; this is the check.
    #[test]
    fn real_config_tree_still_loads_zenoh_blocks() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap()
            .join("config");
        if !root.is_dir() {
            return; // out-of-tree consumer; nothing to check
        }
        let tree = PlatformsTree::load(&root).expect("the real config/ tree loads");
        assert!(!tree.files.is_empty(), "config/ yielded no platform files");
        let with_zenoh = tree
            .files
            .values()
            .filter(|f| f.build.contains_key("zenoh"))
            .count();
        assert!(
            with_zenoh >= 7,
            "expected >=7 platform files carrying a [build.zenoh] block, saw {with_zenoh}"
        );
    }

    /// issue 0534 — exactly which platforms keep cargo out of the vendored C
    /// build, asserted against the real tree.
    ///
    /// This is a TRIPWIRE, not a preference. `compiled_by = "platform"` is the
    /// difference between "the zpico resolver names this platform" and "a build
    /// script cc-compiles its `system/<platform>/*.c`", and on Zephyr the second
    /// cannot work: those sources need Zephyr's generated `version.h`. The
    /// property lived in a comment for one release and #529 walked straight
    /// through it, so it is checked in BOTH directions — a platform gaining the
    /// key and Zephyr losing it are equally a regression.
    #[test]
    fn only_zephyr_delegates_its_c_build_to_the_platform() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap()
            .join("config");
        if !root.is_dir() {
            return; // out-of-tree consumer; nothing to check
        }
        let tree = PlatformsTree::load(&root).expect("the real config/ tree loads");
        let manifest = tree.as_platform_manifest();
        let mut delegating: Vec<String> = Vec::new();
        let mut checked = 0usize;
        for name in tree.files.keys() {
            let Ok(resolved) = manifest.for_platform(name) else {
                continue;
            };
            checked += 1;
            if resolved.compiled_by == crate::manifest::CompiledBy::Platform {
                delegating.push(name.clone());
            }
        }
        assert!(
            checked >= 7,
            "resolved only {checked} platforms — this assertion would be vacuous"
        );
        delegating.sort();
        assert_eq!(
            delegating,
            vec!["zephyr".to_string()],
            "the set of platforms whose own build system compiles zenoh-pico changed. \
             Adding one is fine — say so here. Losing zephyr is issue 0534 returning: \
             cargo will cc-compile system/zephyr/*.c and fail on a missing version.h."
        );
    }

    /// phase-349 W1 — a platform answers to its aliases, and the directory
    /// name always works whether or not `names` is declared.
    #[test]
    fn an_alias_resolves_to_its_directory() {
        let tmp = write_tree(&[
            ("freertos", "names = [\"freertos\", \"freertos-lwip\"]\n"),
            ("posix", ""),
        ]);
        let tree = PlatformsTree::load(tmp.path()).expect("loads");

        assert_eq!(tree.resolve_alias("freertos-lwip"), "freertos");
        assert_eq!(tree.resolve_alias("freertos"), "freertos");
        // A file declaring no names still answers to its directory — that is
        // what every file meant before `names` existed.
        assert_eq!(tree.resolve_alias("posix"), "posix");
        // An unclaimed name comes back unchanged, so the caller's own
        // "unknown platform" error names what the USER asked for.
        assert_eq!(tree.resolve_alias("nope"), "nope");
    }

    /// The alias must work through the public lookups, not merely in
    /// `resolve_alias` — the point is that callers need no alias awareness.
    #[test]
    fn public_lookups_accept_an_alias() {
        let tmp = write_tree(&[(
            "freertos",
            "names = [\"freertos\", \"freertos-lwip\"]\n\
             [capabilities]\nthreads = true\n",
        )]);
        let tree = PlatformsTree::load(tmp.path()).expect("loads");

        let by_alias = tree.capabilities("freertos-lwip").expect("alias resolves");
        let by_dir = tree.capabilities("freertos").expect("dir resolves");
        assert_eq!(by_alias, by_dir);
        assert_eq!(by_alias.get("threads"), Some(&true));

        let no_env = |_: &str| None;
        assert!(tree.resolve_tx("freertos-lwip", None, &no_env).is_ok());
    }

    /// `inherits` names a sibling, and an alias there must resolve too —
    /// otherwise renaming a directory silently breaks its children.
    #[test]
    fn inherits_accepts_an_alias() {
        let tmp = write_tree(&[
            (
                "freertos",
                "names = [\"freertos\", \"freertos-lwip\"]\n\
                 [capabilities]\nthreads = true\n",
            ),
            ("child", "inherits = \"freertos-lwip\"\n"),
        ]);
        let tree = PlatformsTree::load(tmp.path()).expect("loads");
        let caps = tree.capabilities("child").expect("inherits via alias");
        assert_eq!(caps.get("threads"), Some(&true));
    }

    #[test]
    fn build_section_accepts_a_second_component() {
        let tmp = write_tree(&[(
            "zephyr",
            "[build.zenoh]\ndefines = [\"Z_ONE\"]\n\
             [build.xrce]\ndefines = [\"X_ONE\", \"X_TWO\"]\n",
        )]);
        let tree = PlatformsTree::load(tmp.path()).expect("a second [build.*] key must parse");
        let file = tree.files.get("zephyr").expect("zephyr file loaded");
        assert_eq!(
            file.build.get("zenoh").expect("zenoh block").defines,
            vec!["Z_ONE".to_string()],
        );
        assert_eq!(
            file.build.get("xrce").expect("xrce block").defines,
            vec!["X_ONE".to_string(), "X_TWO".to_string()],
        );
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
                "child",
                "inherits = \"generic\"\n[knobs.zenoh.tx]\nflush_ms = 60\n",
            ),
        ]);
        let tree = PlatformsTree::load(tmp.path()).unwrap();
        let tx = tree.resolve_tx("child", None, &no_env).unwrap();
        assert_eq!(tx.flush_ms.value, 60);
        let caps = tree.capabilities("child").unwrap();
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
                "child",
                "inherits = \"generic\"\n[build.zenoh]\ndefines = [\"B\"]\n",
            ),
        ]);
        let tree = PlatformsTree::load(tmp.path()).unwrap();
        let manifest = tree.as_platform_manifest();
        let resolved = manifest.for_platform("child").unwrap();
        assert_eq!(resolved.defines, vec!["A".to_string(), "B".to_string()]);
    }
}
