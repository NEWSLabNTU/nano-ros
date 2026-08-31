//! Per-field message capacity configuration (RFC-0033, Phase 229).
//!
//! Generated message bindings store unbounded sequence/string fields in
//! fixed-capacity containers. Historically the capacity was a single hardcoded
//! constant (`*_DEFAULT_SEQUENCE_CAPACITY` = 64, `*_DEFAULT_STRING_CAPACITY` =
//! 256) shared by the Rust, C, and C++ generators. This module reads a
//! `nros-codegen.toml` into one [`CapacityResolver`] that all three generators
//! consult — a single resolver / three emitters is what makes the configuration
//! language-agnostic.
//!
//! Only **unbounded** fields consult the resolver. Explicit `.msg` bounds
//! (`uint8[<=N]`, `string<=N`) are authoritative and resolved by the caller
//! before reaching [`CapacityResolver::resolve`].
//!
//! Precedence (highest wins): `.msg` bound (caller) → `[fields]` → `[types]` →
//! `[packages]` → `[defaults]` → built-in constant.
//!
//! # Two dimensions, not one (phase-403 W7)
//!
//! An entry carries `cap` AND `element_cap`. `cap` bounds the field; for a
//! `string[]` that is the sequence LENGTH, and a length alone bounds nothing.
//! `element_cap` bounds ONE ELEMENT, and it is a genuinely independent
//! dimension because `.msg` already treats it as one: ROS 2's parser strips the
//! array suffix and THEN parses the base type, so `string<=10[<=5]` is five
//! ten-byte strings and `string[<=5]` is five unbounded ones.
//!
//! The two walk the level chain above independently — see
//! [`CapacityResolver::declared_element_bound`] — and the element dimension is
//! lowered by REWRITING the field into the `.msg` shape
//! ([`with_element_bound`]), so every emitter's existing bounded-element path
//! applies unchanged.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Deserialize;

/// Default string capacity for nros heapless strings (the global fallback when
/// no per-field / per-type / per-package config entry applies).
pub const NROS_DEFAULT_STRING_CAPACITY: usize = 256;

/// Default sequence capacity for nros heapless vectors.
pub const NROS_DEFAULT_SEQUENCE_CAPACITY: usize = 64;

/// How a field's local storage is realized. See RFC-0033 "Storage modes".
///
/// # Why these names (phase-390)
///
/// The modes were `owned` / `heap` / `borrowed`, and two of the three named the
/// wrong thing.
///
/// `owned` did not distinguish anything: a `heap` field is ALSO owned by the
/// message — an `alloc::Vec<T>` dropped with the struct. What separates them is
/// where the bytes LIVE, inside the struct or outside it, so the axis is
/// `inline` vs `heap`.
///
/// `borrowed` named the lifetime and hid the cost. The fact that matters to a
/// user is not that the data is borrowed but that NOTHING WAS DESERIALIZED —
/// the field is a `&'a [T]` into the CDR receive buffer and the caller owns the
/// decode. `view` says that, maps 1:1 onto the types this mode already
/// generates (`{Msg}View<'a>`, `nros::StringView`, `Span<T>`), and resolves a
/// collision with [`nros_rmw::SlotBorrowing`], whose `try_borrow()` returns a
/// `View<'a>` — the same idea at whole-message granularity, previously spelled
/// with a second word.
///
/// The axis is deliberately not uniform: `inline`/`heap` name storage location,
/// `view` names access. A uniform `inline`/`heap`/`inplace` was dropped because
/// `inline` and `inplace` differ by two letters mid-word, in a config file,
/// where the failure mode is silent.
///
/// The old tokens still PARSE — see the [`Deserialize`] impl — so no existing
/// `nros-codegen.toml` breaks; they are reported through
/// [`CapacityResolver::deprecations`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageMode {
    /// `heapless::Vec<T, N>` / fixed `[N]` array — `N` elems always inline.
    #[default]
    Inline,
    /// `alloc::Vec<T>` — dynamic, needs `alloc`/`std` (Phase 229.5).
    Heap,
    /// `&'a [T]` into the CDR receive buffer — zero-copy (Phase 229.6 / issue 0007).
    View,
}

/// One superseded storage-mode token found in a config file.
///
/// Carried rather than printed because this crate is a library: the CLI decides
/// where a diagnostic goes. Empty for a config that uses only current tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeprecatedModeToken {
    /// The token as written (`"owned"` / `"borrowed"`).
    pub found: String,
    /// What it should say now.
    pub replacement: &'static str,
    /// Config file it came from, when parsed via [`CapacityResolver::from_file`].
    pub path: Option<std::path::PathBuf>,
}

impl<'de> Deserialize<'de> for StorageMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // OWNED `String`, not `&str`: `CapEntry` is `#[serde(untagged)]`, and
        // toml's untagged path buffers each variant attempt into a `Value`, so
        // a borrowed `&str` cannot be produced and every table-form entry fails
        // to match ANY variant — which surfaces as "data did not match any
        // variant of untagged enum CapEntry", naming the enum rather than the
        // field that could not deserialize.
        let token = String::deserialize(d)?;
        StorageMode::from_token(&token).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown storage mode `{token}`; expected one of `inline`, \
                 `heap`, `view` (or the superseded `owned` / `borrowed`)"
            ))
        })
    }
}

impl StorageMode {
    // Issue 0343 — `is_phase1_supported()` used to live here, claiming only
    // `inline` (then spelled `owned`) was supported. It had NO production callers (only its own unit
    // test asserted on it), and its claim had become false: messages DO
    // implement heap and view. A predicate nobody calls, asserting
    // something untrue, is worse than no predicate — it reads as a gate.
    //
    // The real support matrix, enforced where it can actually be honoured:
    //
    // | mode     | message (Rust) | message (C) | message (C++) | srv/action |
    // | -------- | -------------- | ----------- | ------------- | ---------- |
    // | `inline` | yes            | yes         | yes           | yes        |
    // | `heap`   | yes            | bridgeable shapes | primitive seqs | NO   |
    // | `view`   | yes            | yes         | yes           | NO         |
    //
    // Enforced by: `field_to_nros_field_with_mode` / `build_c_field` /
    // `cpp_storage_for_field` (per-language shape support, `UnsupportedStorageMode`)
    // and `ensure_owned_storage_for_payload` (srv/action, which have no
    // `is_heap` branches in their templates —
    // `UnsupportedStorageModeForPayload`). Covered by
    // `tests/srv_action_storage_mode_gate.rs`.

    /// Token used in config + diagnostics. Always the CURRENT spelling — a
    /// config written with a superseded token round-trips to the new one, which
    /// is what makes the deprecation actionable rather than sticky.
    pub fn as_str(self) -> &'static str {
        match self {
            StorageMode::Inline => "inline",
            StorageMode::Heap => "heap",
            StorageMode::View => "view",
        }
    }

    /// Parse a config token, accepting the superseded spellings.
    ///
    /// Kept beside [`Self::superseded_replacement`] so the accepted set and the
    /// deprecation table cannot drift — a second list of old names is exactly
    /// how one spelling survives a rename.
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "inline" => Some(StorageMode::Inline),
            "heap" => Some(StorageMode::Heap),
            "view" => Some(StorageMode::View),
            // phase-390 — superseded, still accepted.
            "owned" => Some(StorageMode::Inline),
            "borrowed" => Some(StorageMode::View),
            _ => None,
        }
    }

    /// The current spelling for a superseded token, or `None` if the token is
    /// already current (or not a mode at all).
    pub fn superseded_replacement(token: &str) -> Option<&'static str> {
        match token {
            "owned" => Some("inline"),
            "borrowed" => Some("view"),
            _ => None,
        }
    }

    /// phase-403 W0 — whether a `cap` in this mode is an upper bound on what
    /// this build can ever hold for the field, and may therefore be read as the
    /// field's serialized-size bound.
    ///
    /// This is NOT a policy choice made here. It is RFC-0033's own
    /// "What each mode GUARANTEES" table, which already answers it:
    ///
    /// | mode     | RFC-0033 says | cap bounds the wire |
    /// | -------- | ------------- | ------------------- |
    /// | `inline` | "bounded, statically provable, analysable — the size is in the type" | YES |
    /// | `heap`   | "`alloc::Vec<T>` (cap = hint)"; "nothing in the type says how much memory the field will want" | no |
    /// | `view`   | "a slice into the CDR receive buffer (no copy, NO FIXED CAPACITY)" | no |
    ///
    /// Read off the emitters rather than off the prose, because the prose could
    /// drift: an `inline` field deserializes through
    /// `heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)`
    /// (`packs/nros/nros_field.jinja`), so a sample above `cap` cannot be
    /// decoded into this type at all — the cap is CHECKED, every time, and the
    /// overrun is reported rather than truncated. A `heap` field decodes through
    /// `nros_core::heap::String::from(s)`, and `nros_type_for_field_heap` does
    /// not even take `cap`. A `view` field decodes through
    /// `reader.read_string()?` into a `&'a str` aliasing the receive buffer,
    /// with no length check anywhere on the path.
    ///
    /// So for `heap` and `view` a cap is a sizing HINT for local storage, and
    /// promoting it to a bound would put a number nothing enforces underneath a
    /// receive buffer. Unbounded is the safe answer there, and it fails at build
    /// time rather than on the wire.
    pub fn cap_bounds_the_wire(self) -> bool {
        match self {
            StorageMode::Inline => true,
            StorageMode::Heap | StorageMode::View => false,
        }
    }
}

/// Which kind of field is being resolved — selects the built-in default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Sequence,
    String,
}

impl FieldKind {
    fn builtin_default(self) -> usize {
        match self {
            FieldKind::Sequence => NROS_DEFAULT_SEQUENCE_CAPACITY,
            FieldKind::String => NROS_DEFAULT_STRING_CAPACITY,
        }
    }
}

/// Resolved storage for one field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldStorage {
    pub cap: usize,
    pub mode: StorageMode,
}

/// A config entry value: either an integer (owned shorthand) or
/// `{ cap, element_cap, mode }`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
enum CapEntry {
    /// `field = 64` → `{ cap = 64, mode = "inline" }`.
    Int(usize),
    /// `field = { cap = 2_000_000, mode = "view" }`.
    Table {
        cap: usize,
        /// phase-403 W7 — the bound on one ELEMENT of an array/sequence field,
        /// a SECOND and independent dimension from `cap`.
        ///
        /// `.msg` already says the two are independent: ROS 2's parser strips
        /// the array suffix and then parses the base type, so `string<=10[<=5]`
        /// is a 5-element sequence of 10-byte strings and `string[<=5]` is a
        /// 5-element sequence of unbounded ones. One config key carrying two
        /// numbers mirrors that, rather than inventing a second key namespace
        /// for elements that would have to be kept in step with the first.
        #[serde(default)]
        element_cap: Option<usize>,
        #[serde(default)]
        mode: StorageMode,
    },
}

impl CapEntry {
    fn resolve(self) -> FieldStorage {
        match self {
            CapEntry::Int(cap) => FieldStorage {
                cap,
                mode: StorageMode::Inline,
            },
            CapEntry::Table { cap, mode, .. } => FieldStorage { cap, mode },
        }
    }

    /// The element bound this entry states, or `None`.
    ///
    /// Deliberately NOT part of [`FieldStorage`]: the two dimensions resolve
    /// through the level chain INDEPENDENTLY (see
    /// [`CapacityResolver::declared_element_bound`]), so folding them into one
    /// resolved value would make a per-field `cap` shadow a `[defaults]`
    /// `element_cap` — capping one field's length would silently drop the
    /// element default the user set once.
    fn element_cap(self) -> Option<usize> {
        match self {
            CapEntry::Int(_) => None,
            CapEntry::Table { element_cap, .. } => element_cap,
        }
    }
}

/// `sequence` / `string` overrides at the `[defaults]`, `[packages.*]`, and
/// `[types.*]` levels. Each accepts the same int-or-table form as `[fields]`.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LevelCaps {
    #[serde(default)]
    sequence: Option<CapEntry>,
    #[serde(default)]
    string: Option<CapEntry>,
}

impl LevelCaps {
    fn pick(&self, kind: FieldKind) -> Option<CapEntry> {
        match kind {
            FieldKind::Sequence => self.sequence,
            FieldKind::String => self.string,
        }
    }

    /// Per-key deep merge: `over` wins where it specifies a value.
    fn merge_over(&mut self, over: LevelCaps) {
        if over.sequence.is_some() {
            self.sequence = over.sequence;
        }
        if over.string.is_some() {
            self.string = over.string;
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    defaults: LevelCaps,
    /// keyed by package, e.g. `"sensor_msgs"`.
    #[serde(default)]
    packages: BTreeMap<String, LevelCaps>,
    /// keyed by `"pkg/Msg"`, e.g. `"sensor_msgs/Image"`.
    #[serde(default)]
    types: BTreeMap<String, LevelCaps>,
    /// keyed by `"pkg/Msg.field"`, e.g. `"sensor_msgs/Image.data"`.
    #[serde(default)]
    fields: BTreeMap<String, CapEntry>,
}

impl RawConfig {
    /// Every `[defaults]` / `[packages.*]` / `[types.*]` entry with the config
    /// path it was written at, for diagnostics that are about the KEY rather
    /// than about any message.
    fn levels(&self) -> impl Iterator<Item = (String, LevelCaps)> + '_ {
        std::iter::once(("defaults".to_string(), self.defaults))
            .chain(
                self.packages
                    .iter()
                    .map(|(k, v)| (format!("packages.{k}"), *v)),
            )
            .chain(
                self.types
                    .iter()
                    .map(|(k, v)| (format!("types.\"{k}\""), *v)),
            )
    }

    /// Deep-merge `over` onto `self`; `over` (the app file) wins.
    fn merge_over(&mut self, over: RawConfig) {
        self.defaults.merge_over(over.defaults);
        for (k, v) in over.packages {
            self.packages.entry(k).or_default().merge_over(v);
        }
        for (k, v) in over.types {
            self.types.entry(k).or_default().merge_over(v);
        }
        // Fields are atomic entries: the app entry replaces the workspace entry.
        self.fields.extend(over.fields);
    }
}

/// The conventional config filename discovered by [`CapacityResolver::discover`].
pub const CODEGEN_CONFIG_FILENAME: &str = "nros-codegen.toml";

/// Error parsing or loading a `nros-codegen.toml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to parse codegen config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to read codegen config '{path}': {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    /// phase-390 — an unrecognised `mode = "..."`.
    ///
    /// Reported from a pre-parse scan rather than from `StorageMode`'s
    /// `Deserialize`, because `CapEntry` is `#[serde(untagged)]` and serde
    /// DISCARDS each variant's error to try the next one. A typo therefore
    /// surfaced as "data did not match any variant of untagged enum CapEntry",
    /// which names an internal type and not the token the user mistyped.
    #[error(
        "unknown storage mode `{token}` in codegen config; expected `inline`, \
         `heap` or `view` (the superseded `owned` / `borrowed` are still \
         accepted)"
    )]
    UnknownStorageMode { token: String },
    /// phase-403 W7 — `element_cap` on a `string` / `wstring` LEVEL entry.
    ///
    /// A level key (`[defaults] string = ...`) sets the default for a field
    /// whose whole shape is a string. Such a field has no elements, so an
    /// `element_cap` there can never apply to anything. Reported at parse time
    /// because it needs no field types to detect — it is wrong about the key it
    /// is written under, not about the messages it would reach.
    #[error(
        "`element_cap` under `{level}.string` in codegen config: a string field \
         has no elements. Put `element_cap` on the `sequence` key (it bounds one \
         element of an array/sequence), or on a `[fields]` entry naming an \
         array/sequence field"
    )]
    ElementCapOnStringLevel { level: String },
}

/// phase-403 W7 — an `element_cap` that names a field it cannot apply to.
///
/// Separate from [`ConfigError`] because it is not a parse failure: detecting it
/// needs the message's FIELD TYPES, which arrive long after the config is read.
/// A `[defaults]` / `[packages.*]` / `[types.*]` `element_cap` never produces
/// one — those are defaults, and a default that does not apply to a given field
/// is how defaults work. Only a `[fields]` entry, which NAMES one field, does.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "`element_cap` in codegen config names `{package}/{message}.{field}`, which is \
     {shape}. An element bound applies only to an array or sequence whose ELEMENT \
     is a string (`string[]`, `string[<=N]`, `string[N]`); use `cap` for the \
     field's own bound"
)]
pub struct ElementCapShapeError {
    pub package: String,
    pub message: String,
    pub field: String,
    /// What the field actually is, as prose ("a string", "a sequence of int32").
    pub shape: String,
}

/// Resolves per-field storage from a merged `nros-codegen.toml`. One instance
/// feeds all three language backends.
#[derive(Debug, Clone, Default)]
pub struct CapacityResolver {
    raw: RawConfig,
    /// phase-390 — superseded `mode` tokens seen while parsing, in file order.
    deprecations: Vec<DeprecatedModeToken>,
}

impl CapacityResolver {
    /// An empty resolver — every field falls through to its built-in default,
    /// reproducing pre-Phase-229 output byte-for-byte.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Parse a single `nros-codegen.toml` body.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        // Scan BEFORE the typed parse: `CapEntry` is untagged, so a bad `mode`
        // makes the whole entry fail to match any variant and serde reports the
        // enum instead of the token. Checking first means the user sees the
        // token they mistyped.
        let scan = scan_mode_tokens(s, None);
        if let Some(token) = scan.unknown.into_iter().next() {
            return Err(ConfigError::UnknownStorageMode { token });
        }
        let raw: RawConfig = toml::from_str(s)?;
        // phase-403 W7 — an `element_cap` under a `string` LEVEL key can never
        // apply: the level entry sets the default for fields that ARE strings,
        // and a string has no elements. Caught here rather than ignored,
        // because the whole point of the shape error is that a key which cannot
        // do anything must not look like it did.
        for (level, caps) in raw.levels() {
            if caps.string.and_then(|e| e.element_cap()).is_some() {
                return Err(ConfigError::ElementCapOnStringLevel { level });
            }
        }
        Ok(Self {
            raw,
            deprecations: scan.superseded,
        })
    }

    /// Load a single `nros-codegen.toml` from `path`.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let body = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mut resolver = Self::from_toml_str(&body)?;
        // Re-scan WITH the path so a diagnostic can name the file. Cheaper than
        // threading an Option<&Path> through `from_toml_str`, whose signature is
        // public and used on bodies that have no path at all.
        resolver.deprecations = scan_mode_tokens(&body, Some(path)).superseded;
        Ok(resolver)
    }

    /// Superseded storage-mode tokens found while parsing, in file order.
    ///
    /// The library COLLECTS rather than prints: where a diagnostic goes is the
    /// CLI's decision, and a `eprintln!` here would fire inside every build
    /// script that resolves a capacity.
    pub fn deprecations(&self) -> &[DeprecatedModeToken] {
        &self.deprecations
    }

    /// Discover and merge `nros-codegen.toml` files by walking up from
    /// `start_dir` to the filesystem root (or until `stop_dir`, inclusive).
    /// Files are merged root-most → `start_dir`, so the **closest** file (the
    /// app) wins over ancestors (the workspace). Missing files are skipped; an
    /// absent chain yields an empty resolver (built-in defaults).
    pub fn discover(start_dir: &Path, stop_dir: Option<&Path>) -> Result<Self, ConfigError> {
        // Collect candidate dirs from start upward, then reverse so the
        // root-most is merged first and the closest file wins.
        let mut dirs: Vec<&Path> = Vec::new();
        let mut cur = Some(start_dir);
        while let Some(dir) = cur {
            dirs.push(dir);
            if stop_dir == Some(dir) {
                break;
            }
            cur = dir.parent();
        }

        let mut resolver = Self::empty();
        for dir in dirs.into_iter().rev() {
            let candidate = dir.join(CODEGEN_CONFIG_FILENAME);
            if candidate.is_file() {
                resolver = resolver.merged_with(Self::from_file(&candidate)?);
            }
        }
        Ok(resolver)
    }

    /// Build a resolver from an optional explicit config path plus discovery
    /// from `start_dir`. The explicit file (if any) is merged **last** so a
    /// `--codegen-config` flag wins over any discovered `nros-codegen.toml`.
    pub fn resolve_for(
        explicit: Option<&Path>,
        start_dir: &Path,
        stop_dir: Option<&Path>,
    ) -> Result<Self, ConfigError> {
        let mut resolver = Self::discover(start_dir, stop_dir)?;
        if let Some(path) = explicit {
            resolver = resolver.merged_with(Self::from_file(path)?);
        }
        Ok(resolver)
    }

    /// Merge another config on top of this one; `over` (e.g. the app file) wins
    /// over `self` (e.g. the workspace file).
    pub fn merged_with(mut self, over: CapacityResolver) -> Self {
        self.raw.merge_over(over.raw);
        // phase-390 — accumulate here rather than at each call site. Every path
        // that builds a resolver from more than one file (`discover`,
        // `resolve_for`) funnels through this method, so collecting here means
        // a superseded token in a WORKSPACE-level config is reported even when
        // the app-level file that overrides it is clean.
        self.deprecations.extend(over.deprecations);
        self
    }

    /// A ready-to-print report of superseded storage-mode tokens, or `None`
    /// when the config is clean.
    ///
    /// Deduplicated by (file, token): a config that sets `mode = "owned"` on
    /// forty fields is one mistake to fix, not forty lines of output.
    pub fn deprecation_report(&self) -> Option<String> {
        if self.deprecations.is_empty() {
            return None;
        }
        let mut seen = std::collections::BTreeSet::new();
        for d in &self.deprecations {
            let where_ = d
                .path
                .as_deref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<config>".to_string());
            seen.insert((where_, d.found.clone(), d.replacement));
        }
        let mut out = String::from(
            "nros-codegen.toml: storage mode renamed (phase-390); the old \
             spelling still works:\n",
        );
        for (where_, found, replacement) in seen {
            out.push_str(&format!(
                "  {where_}: mode = \"{found}\"  ->  mode = \"{replacement}\"\n"
            ));
        }
        Some(out)
    }

    /// Print [`Self::deprecation_report`] to stderr, if there is one.
    ///
    /// ONE spelling of "warn about this", called by every construction site in
    /// the CLI, rather than a `eprintln!` written out at each — a second
    /// formatting of the same warning is how one of them ends up naming the
    /// wrong replacement. Stderr, not stdout: `nros generate-*` output is piped.
    pub fn report_deprecations(&self) {
        if let Some(report) = self.deprecation_report() {
            eprint!("{report}");
        }
    }

    /// Resolve storage for an **unbounded** field. Bounded fields are resolved
    /// by the caller from the `.msg` bound and must not reach this method.
    pub fn resolve(
        &self,
        package: &str,
        message: &str,
        field: &str,
        kind: FieldKind,
    ) -> FieldStorage {
        self.resolve_configured(package, message, field, kind)
            // 6 — built-in
            .unwrap_or(FieldStorage {
                cap: kind.builtin_default(),
                mode: StorageMode::Inline,
            })
    }

    /// The storage a config FILE states for this field, or `None` when nothing
    /// in the chain matched and [`Self::resolve`] would fall through to the
    /// built-in constant.
    ///
    /// # Why the provenance has to be separable (phase-403 W0)
    ///
    /// [`Self::resolve`] always answers, which is right for an emitter — every
    /// field needs a container size. It is exactly wrong for the question
    /// "is this field bounded?", because the built-in 256/64 would answer YES
    /// for every unbounded string and sequence in the tree and quietly bound
    /// the whole world at a number nobody chose. phase-403 W0 made an unbounded
    /// type a build error precisely so that a bound is something a human
    /// STATED; a fallback silently satisfying that rule would delete the rule.
    ///
    /// `[defaults]` counts as stated: it is a line in a config file somebody
    /// wrote. The level-6 constant does not: it is what codegen does when told
    /// nothing.
    pub fn resolve_configured(
        &self,
        package: &str,
        message: &str,
        field: &str,
        kind: FieldKind,
    ) -> Option<FieldStorage> {
        // 2 — per-field
        let field_key = format!("{package}/{message}.{field}");
        if let Some(e) = self.raw.fields.get(&field_key) {
            return Some(e.resolve());
        }
        // 3 — per-type
        let type_key = format!("{package}/{message}");
        if let Some(e) = self.raw.types.get(&type_key).and_then(|l| l.pick(kind)) {
            return Some(e.resolve());
        }
        // 4 — per-package
        if let Some(e) = self.raw.packages.get(package).and_then(|l| l.pick(kind)) {
            return Some(e.resolve());
        }
        // 5 — global defaults
        if let Some(e) = self.raw.defaults.pick(kind) {
            return Some(e.resolve());
        }
        None
    }

    /// phase-403 W0 — the serialized-size bound a config `cap` STATES for one
    /// unbounded field, or `None` when the config states none this build can
    /// hold itself to.
    ///
    /// Two ways to get `None`, and they are the same answer here: nothing in the
    /// config named the field (so the built-in default applies, and a default is
    /// not a stated bound — see [`Self::resolve_configured`]), or the config
    /// named it in a mode whose cap is a hint rather than a limit (see
    /// [`StorageMode::cap_bounds_the_wire`]).
    ///
    /// `package` / `message` are the DECLARING type, never the top-level message
    /// being generated. That is what makes one entry for
    /// `std_msgs/Header.frame_id` bound the `header` of every message that
    /// nests a `Header`, instead of needing one entry per containing type.
    ///
    /// A `.msg` bound never reaches here: a bounded field is resolved from the
    /// interface by the caller and does not consult the resolver at all, so the
    /// interface stays authoritative and a config cap cannot widen or narrow it.
    pub fn declared_bound(
        &self,
        package: &str,
        message: &str,
        field: &str,
        kind: FieldKind,
    ) -> Option<usize> {
        let storage = self.resolve_configured(package, message, field, kind)?;
        storage.mode.cap_bounds_the_wire().then_some(storage.cap)
    }

    /// phase-403 W7 — the bound a config `element_cap` STATES for ONE ELEMENT of
    /// an array or sequence field, or `None`.
    ///
    /// # Why a second dimension at all
    ///
    /// A `cap` bounds the field. For a `string[]` that is the sequence LENGTH,
    /// and a length alone bounds nothing: 16 unbounded strings are still
    /// unbounded. Five stock ROS Humble types have exactly this shape and were
    /// the only ones the phase-403 measurement could not bound
    /// (`sensor_msgs/JointState.name` and four siblings). `.msg` has always had
    /// both dimensions — ROS 2's parser strips the array suffix and then parses
    /// the base type, so `string<=10[<=5]` bounds each — and this is the same
    /// two numbers in the config.
    ///
    /// # The rules, all four of them
    ///
    /// 1. **Shape.** Only an array/sequence whose element is an UNBOUNDED
    ///    `string`/`wstring` has an element dimension to bound. Anything else
    ///    answers `None`; a `[fields]` entry that names such a field is an
    ///    error, reported by [`Self::element_cap_shape_errors`] rather than here
    ///    (this method is called from paths that have no error channel, and a
    ///    silently ignored key is what the error exists to prevent).
    /// 2. **The `.msg` wins, per dimension.** `string<=10[]` capped with
    ///    `element_cap = 32` keeps 10, exactly as a `.msg`-bounded field keeps
    ///    its own `cap`. Enforced by construction: an already-bounded element is
    ///    not an unbounded string, so rule 1 answers `None` first.
    /// 3. **Only a bounding mode bounds.** The mode is the FIELD's — a `view`
    ///    field aliases the receive buffer and a `heap` field's cap is a hint,
    ///    so neither can hold an element to a size either
    ///    ([`StorageMode::cap_bounds_the_wire`]). A `.msg`-bounded sequence and
    ///    a fixed array are not configurable shapes at all, so their mode is
    ///    `inline` by construction.
    /// 4. **Its own level chain.** `element_cap` resolves through `[fields]` →
    ///    `[types]` → `[packages]` → `[defaults]` INDEPENDENTLY of `cap`, so
    ///    `[defaults] sequence = { cap = 16, element_cap = 32 }` still supplies
    ///    the element bound for a field whose length a `[fields]` entry
    ///    overrides. Folding both into one resolved entry would make capping one
    ///    field's length silently delete the element default.
    pub fn declared_element_bound(
        &self,
        package: &str,
        message: &str,
        field: &str,
        field_type: &rosidl_parser::ast::FieldType,
    ) -> Option<usize> {
        // 1 — shape.
        if !element_is_unbounded_string(field_type) {
            return None;
        }
        // 3 — mode. Only a plain `type[]` is a configurable shape; a fixed array
        // and a `.msg`-bounded sequence keep an inline container whatever the
        // config says, so nothing can move their mode off `inline`.
        let mode = match field_type {
            rosidl_parser::ast::FieldType::Sequence { .. } => self
                .resolve_configured(package, message, field, FieldKind::Sequence)
                .map(|s| s.mode)
                .unwrap_or_default(),
            _ => StorageMode::Inline,
        };
        if !mode.cap_bounds_the_wire() {
            return None;
        }
        // 4 — the element level chain, walked separately from `cap`'s.
        self.configured_element_cap(package, message, field)
    }

    /// `field_type` with any configured element bound applied -- the ONE call an
    /// emitter makes.
    ///
    /// Borrowed when nothing applies, so the overwhelmingly common field costs
    /// no clone. See [`with_element_bound`] for why this is a rewrite of the
    /// field's shape rather than a number passed alongside it.
    pub fn element_capped<'t>(
        &self,
        package: &str,
        message: &str,
        field: &str,
        field_type: &'t rosidl_parser::ast::FieldType,
    ) -> std::borrow::Cow<'t, rosidl_parser::ast::FieldType> {
        with_element_bound(
            field_type,
            self.declared_element_bound(package, message, field, field_type),
        )
    }

    /// The `element_cap` a config FILE states for this field, before the shape
    /// and mode rules of [`Self::declared_element_bound`] are applied.
    ///
    /// Level entries are read off the `sequence` key: an element belongs to a
    /// sequence/array, and `[defaults] string` is rejected at parse time
    /// ([`ConfigError::ElementCapOnStringLevel`]) precisely so there is only one
    /// place to look.
    fn configured_element_cap(&self, package: &str, message: &str, field: &str) -> Option<usize> {
        let field_key = format!("{package}/{message}.{field}");
        if let Some(e) = self
            .raw
            .fields
            .get(&field_key)
            .and_then(|e| e.element_cap())
        {
            return Some(e);
        }
        let type_key = format!("{package}/{message}");
        if let Some(e) = self
            .raw
            .types
            .get(&type_key)
            .and_then(|l| l.sequence)
            .and_then(|e| e.element_cap())
        {
            return Some(e);
        }
        if let Some(e) = self
            .raw
            .packages
            .get(package)
            .and_then(|l| l.sequence)
            .and_then(|e| e.element_cap())
        {
            return Some(e);
        }
        self.raw.defaults.sequence.and_then(|e| e.element_cap())
    }

    /// phase-403 W7 — every `[fields]` `element_cap` in this config that names a
    /// field of `message` it cannot apply to.
    ///
    /// A `Vec` and not the first offender, for the reason
    /// `TypeBound::Unbounded` carries all its members: one build should name
    /// everything the user has to fix.
    ///
    /// Only `[fields]` entries are checked. A `[defaults]`/`[packages]`/`[types]`
    /// `element_cap` reaching a plain `string` field is a default that does not
    /// apply, which is how a default is supposed to behave; a `[fields]` entry
    /// is a user pointing at one field and stating something about it, and if
    /// nothing can come of that they have to be told.
    pub fn element_cap_shape_errors(
        &self,
        package: &str,
        message: &str,
        fields: &[rosidl_parser::ast::Field],
    ) -> Vec<ElementCapShapeError> {
        fields
            .iter()
            .filter(|f| {
                self.raw
                    .fields
                    .get(&format!("{package}/{message}.{}", f.name))
                    .and_then(|e| e.element_cap())
                    .is_some()
            })
            .filter(|f| !element_is_unbounded_string(&f.field_type))
            .map(|f| ElementCapShapeError {
                package: package.to_string(),
                message: message.to_string(),
                field: f.name.clone(),
                shape: describe_shape(&f.field_type),
            })
            .collect()
    }
}

/// The element of an array/sequence field, or `None` for any other shape.
fn element_of(ty: &rosidl_parser::ast::FieldType) -> Option<&rosidl_parser::ast::FieldType> {
    use rosidl_parser::ast::FieldType as F;
    match ty {
        F::Array { element_type, .. }
        | F::Sequence { element_type }
        | F::BoundedSequence { element_type, .. } => Some(element_type),
        _ => None,
    }
}

/// Whether `ty` is an array/sequence whose element is an UNBOUNDED string --
/// the one shape an `element_cap` can bound.
///
/// An already-bounded element (`string<=10[]`) answers `false`, which is how the
/// "`.msg` wins per dimension" rule is enforced: there is no second precedence
/// table to keep in step, the interface bound simply removes the dimension the
/// config could have spoken about.
fn element_is_unbounded_string(ty: &rosidl_parser::ast::FieldType) -> bool {
    use rosidl_parser::ast::FieldType as F;
    matches!(element_of(ty), Some(F::String | F::WString))
}

/// What a field is, as prose for [`ElementCapShapeError`].
fn describe_shape(ty: &rosidl_parser::ast::FieldType) -> String {
    use rosidl_parser::ast::FieldType as F;
    let base = |t: &F| -> String {
        match t {
            F::Primitive(p) => format!("{p:?}").to_lowercase(),
            F::String => "string".into(),
            F::WString => "wstring".into(),
            F::BoundedString(n) => format!("string<={n}"),
            F::BoundedWString(n) => format!("wstring<={n}"),
            F::NamespacedType { package, name } => match package {
                Some(p) => format!("{p}/{name}"),
                None => name.clone(),
            },
            other => format!("{other:?}"),
        }
    };
    match ty {
        F::Array { element_type, .. } => format!("an array of {}", base(element_type)),
        F::Sequence { element_type } => format!("a sequence of {}", base(element_type)),
        F::BoundedSequence { element_type, .. } => {
            format!("a bounded sequence of {}", base(element_type))
        }
        other => format!("a {}", base(other)),
    }
}

/// phase-403 W7 — rewrite an array/sequence field so its ELEMENT carries
/// `bound`, exactly as if the `.msg` had spelled `string<=N[...]`.
///
/// # Why a rewrite and not a parameter
///
/// Every emitter in the tree -- the Rust container
/// (`heapless::String<N>`, whose `try_from` returns `CapacityExceeded` above
/// `N`), the C `char[N]` that `nros_cdr_read_string` sizes with `sizeof`, the
/// C++ `nros::FixedString<N>`, the schema value, and the emitted
/// `Message::FIELDS` -- ALREADY handles a bounded element correctly, because a
/// `.msg` has always been able to say `string<=10[]`. Threading a separate
/// "element cap" parameter through all five would be a second way to spell a
/// thing they can already spell, and the two spellings would drift.
///
/// So the config's element dimension is lowered into the SAME shape the
/// interface uses, once, and nothing downstream needs to know a config was
/// involved. This is also why the claim is honest: the bound is enforced at
/// deserialize by the container the rewrite produces, which is the test
/// phase-403 applied to `cap` (`StorageMode::cap_bounds_the_wire`).
///
/// `None`, and any shape without an unbounded string element, returns the input
/// unchanged.
pub fn with_element_bound(
    ty: &rosidl_parser::ast::FieldType,
    bound: Option<usize>,
) -> std::borrow::Cow<'_, rosidl_parser::ast::FieldType> {
    use rosidl_parser::ast::FieldType as F;
    use std::borrow::Cow;
    let Some(n) = bound else {
        return Cow::Borrowed(ty);
    };
    let bounded = match element_of(ty) {
        Some(F::String) => F::BoundedString(n),
        Some(F::WString) => F::BoundedWString(n),
        _ => return Cow::Borrowed(ty),
    };
    Cow::Owned(match ty {
        F::Array { size, .. } => F::Array {
            element_type: Box::new(bounded),
            size: *size,
        },
        F::Sequence { .. } => F::Sequence {
            element_type: Box::new(bounded),
        },
        F::BoundedSequence { max_size, .. } => F::BoundedSequence {
            element_type: Box::new(bounded),
            max_size: *max_size,
        },
        _ => return Cow::Borrowed(ty),
    })
}

/// What one walk of a config body found in its `mode` keys.
struct ModeScan {
    superseded: Vec<DeprecatedModeToken>,
    unknown: Vec<String>,
}

/// Find every `mode = "..."` in a parsed config body, split into superseded
/// tokens and unrecognised ones.
///
/// Walks the parsed TOML rather than grepping the text. A substring search for
/// `"owned"` would also hit a comment, a field NAMED owned, or a capacity table
/// key — and this diagnostic tells a user to edit their file, so a false
/// positive sends them looking for something that is not there.
fn scan_mode_tokens(body: &str, path: Option<&Path>) -> ModeScan {
    fn walk(value: &toml::Value, path: Option<&Path>, out: &mut ModeScan) {
        match value {
            toml::Value::Table(t) => {
                for (k, v) in t {
                    if k == "mode"
                        && let Some(token) = v.as_str()
                    {
                        if let Some(replacement) = StorageMode::superseded_replacement(token) {
                            out.superseded.push(DeprecatedModeToken {
                                found: token.to_string(),
                                replacement,
                                path: path.map(Path::to_path_buf),
                            });
                        } else if StorageMode::from_token(token).is_none() {
                            out.unknown.push(token.to_string());
                        }
                    }
                    walk(v, path, out);
                }
            }
            toml::Value::Array(a) => a.iter().for_each(|v| walk(v, path, out)),
            _ => {}
        }
    }

    let mut out = ModeScan {
        superseded: Vec::new(),
        unknown: Vec::new(),
    };
    // A body that does not parse at all is reported by the caller's
    // `toml::from_str`; there is nothing useful to say about its modes.
    if let Ok(value) = body.parse::<toml::Value>() {
        walk(&value, path, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    // ========================================================================
    // phase-390 — the rename, and what it must not break
    // ========================================================================

    /// A config written before the rename must keep working. This is the whole
    /// reason `from_token` accepts five spellings for three modes.
    #[test]
    fn superseded_tokens_still_parse_to_the_same_modes() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "a/B.x" = { cap = 8, mode = "owned" }
            "a/B.y" = { cap = 8, mode = "borrowed" }
            "a/B.z" = { cap = 8, mode = "heap" }
            "#,
        )
        .unwrap();
        assert_eq!(
            r.resolve("a", "B", "x", FieldKind::Sequence).mode,
            StorageMode::Inline
        );
        assert_eq!(
            r.resolve("a", "B", "y", FieldKind::Sequence).mode,
            StorageMode::View
        );
        assert_eq!(
            r.resolve("a", "B", "z", FieldKind::Sequence).mode,
            StorageMode::Heap
        );
    }

    #[test]
    fn current_tokens_parse() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "a/B.x" = { cap = 8, mode = "inline" }
            "a/B.y" = { cap = 8, mode = "view" }
            "#,
        )
        .unwrap();
        assert_eq!(
            r.resolve("a", "B", "x", FieldKind::Sequence).mode,
            StorageMode::Inline
        );
        assert_eq!(
            r.resolve("a", "B", "y", FieldKind::Sequence).mode,
            StorageMode::View
        );
    }

    /// A superseded token round-trips to the CURRENT spelling. Without this the
    /// deprecation is sticky: every diagnostic downstream would keep echoing
    /// the old word back at the user who was told to stop using it.
    #[test]
    fn a_superseded_token_round_trips_to_the_new_spelling() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "a/B.x" = { cap = 8, mode = "owned" }
            "#,
        )
        .unwrap();
        assert_eq!(
            r.resolve("a", "B", "x", FieldKind::Sequence).mode.as_str(),
            "inline"
        );
    }

    #[test]
    fn superseded_tokens_are_reported_with_their_replacement() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "a/B.x" = { cap = 8, mode = "owned" }
            "a/B.y" = { cap = 8, mode = "borrowed" }
            "#,
        )
        .unwrap();
        let found: Vec<_> = r
            .deprecations()
            .iter()
            .map(|d| (d.found.as_str(), d.replacement))
            .collect();
        assert_eq!(found, vec![("owned", "inline"), ("borrowed", "view")]);
    }

    /// A superseded token in a WORKSPACE-level file must survive being merged
    /// under a clean app-level one. Collecting at each parse instead of at the
    /// merge is how the outer file's mistake goes unreported.
    #[test]
    fn merging_keeps_deprecations_from_both_files() {
        let workspace = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "a/B.x" = { cap = 8, mode = "owned" }
            "#,
        )
        .unwrap();
        let app = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "a/B.y" = { cap = 8, mode = "view" }
            "#,
        )
        .unwrap();
        let merged = workspace.merged_with(app);
        assert_eq!(merged.deprecations().len(), 1);
        assert_eq!(merged.deprecations()[0].replacement, "inline");
    }

    /// One mistake, one line — a config that sets the old token on forty fields
    /// is still one edit, and forty lines of output would bury it.
    #[test]
    fn the_report_deduplicates_repeated_tokens() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "a/B.x" = { cap = 8, mode = "owned" }
            "a/B.y" = { cap = 8, mode = "owned" }
            "a/B.z" = { cap = 8, mode = "owned" }
            "#,
        )
        .unwrap();
        let report = r.deprecation_report().expect("should report");
        assert_eq!(report.matches("mode = \"owned\"").count(), 1, "{report}");
        assert!(report.contains("mode = \"inline\""), "{report}");
    }

    #[test]
    fn a_clean_config_has_no_report() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "a/B.x" = { cap = 8, mode = "inline" }
            "#,
        )
        .unwrap();
        assert!(r.deprecation_report().is_none());
    }

    #[test]
    fn current_tokens_produce_no_deprecations() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "a/B.x" = { cap = 8, mode = "inline" }
            "a/B.y" = { cap = 8, mode = "view" }
            "a/B.z" = { cap = 8, mode = "heap" }
            "#,
        )
        .unwrap();
        assert!(r.deprecations().is_empty(), "{:?}", r.deprecations());
    }

    /// The walker reads PARSED toml, so a superseded word in a comment or in a
    /// non-`mode` position must not be reported. A diagnostic that tells a user
    /// to edit a line that is already correct sends them hunting for nothing.
    #[test]
    fn a_superseded_word_outside_a_mode_key_is_not_reported() {
        let r = CapacityResolver::from_toml_str(
            r#"
            # these pixels are owned by the driver, not borrowed
            [fields]
            "a/Owned.borrowed" = { cap = 8, mode = "inline" }
            "#,
        )
        .unwrap();
        assert!(r.deprecations().is_empty(), "{:?}", r.deprecations());
    }

    #[test]
    fn an_unknown_mode_names_every_accepted_token() {
        let err = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "a/B.x" = { cap = 8, mode = "inplace" }
            "#,
        )
        .unwrap_err()
        .to_string();
        for token in ["inline", "heap", "view"] {
            assert!(err.contains(token), "error should name `{token}`: {err}");
        }
    }

    use super::*;

    const SEQ: FieldKind = FieldKind::Sequence;
    const STR: FieldKind = FieldKind::String;

    #[test]
    fn empty_config_yields_builtin_defaults() {
        let r = CapacityResolver::empty();
        let s = r.resolve("std_msgs", "String", "data", STR);
        assert_eq!(s.cap, NROS_DEFAULT_STRING_CAPACITY);
        assert_eq!(s.mode, StorageMode::Inline);
        let q = r.resolve("sensor_msgs", "Image", "data", SEQ);
        assert_eq!(q.cap, NROS_DEFAULT_SEQUENCE_CAPACITY);
        assert_eq!(q.mode, StorageMode::Inline);
    }

    #[test]
    fn int_shorthand_is_owned() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "std_msgs/String.data" = 64
            "#,
        )
        .unwrap();
        let s = r.resolve("std_msgs", "String", "data", STR);
        assert_eq!(
            s,
            FieldStorage {
                cap: 64,
                mode: StorageMode::Inline
            }
        );
    }

    #[test]
    fn table_form_carries_mode() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "sensor_msgs/Image.data" = { cap = 2000000, mode = "borrowed" }
            "sensor_msgs/LaserScan.ranges" = { cap = 1080, mode = "heap" }
            "#,
        )
        .unwrap();
        assert_eq!(
            r.resolve("sensor_msgs", "Image", "data", SEQ),
            FieldStorage {
                cap: 2_000_000,
                mode: StorageMode::View
            }
        );
        assert_eq!(
            r.resolve("sensor_msgs", "LaserScan", "ranges", SEQ),
            FieldStorage {
                cap: 1080,
                mode: StorageMode::Heap
            }
        );
    }

    #[test]
    fn precedence_field_beats_type_beats_package_beats_defaults() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [defaults]
            sequence = 100

            [packages."sensor_msgs"]
            sequence = 200

            [types."sensor_msgs/Image"]
            sequence = 300

            [fields]
            "sensor_msgs/Image.data" = 400
            "#,
        )
        .unwrap();
        // field wins
        assert_eq!(r.resolve("sensor_msgs", "Image", "data", SEQ).cap, 400);
        // no field entry → type wins
        assert_eq!(r.resolve("sensor_msgs", "Image", "other", SEQ).cap, 300);
        // no type entry → package wins
        assert_eq!(
            r.resolve("sensor_msgs", "PointCloud2", "data", SEQ).cap,
            200
        );
        // different package → defaults
        assert_eq!(r.resolve("nav_msgs", "Path", "poses", SEQ).cap, 100);
    }

    #[test]
    fn sequence_and_string_defaults_are_independent() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [defaults]
            sequence = 4096
            string = 16
            "#,
        )
        .unwrap();
        assert_eq!(r.resolve("p", "M", "f", SEQ).cap, 4096);
        assert_eq!(r.resolve("p", "M", "f", STR).cap, 16);
    }

    #[test]
    fn within_one_message_big_seq_and_small_string_coexist() {
        // The motivating case: big image data, small string field, same message.
        let r = CapacityResolver::from_toml_str(
            r#"
            [fields]
            "my_msgs/Frame.pixels" = { cap = 921600, mode = "heap" }
            "my_msgs/Frame.label"  = 32
            "#,
        )
        .unwrap();
        assert_eq!(r.resolve("my_msgs", "Frame", "pixels", SEQ).cap, 921_600);
        assert_eq!(r.resolve("my_msgs", "Frame", "label", STR).cap, 32);
    }

    #[test]
    fn deep_merge_app_overrides_workspace() {
        let workspace = CapacityResolver::from_toml_str(
            r#"
            [defaults]
            sequence = 64
            string = 256

            [fields]
            "a/B.c" = 10
            "a/B.d" = 20
            "#,
        )
        .unwrap();
        let app = CapacityResolver::from_toml_str(
            r#"
            [defaults]
            sequence = 128

            [fields]
            "a/B.c" = 99
            "#,
        )
        .unwrap();
        let r = workspace.merged_with(app);
        // app default overrides workspace default for sequence...
        assert_eq!(r.resolve("z", "Z", "z", SEQ).cap, 128);
        // ...but string default survives from workspace (app didn't set it)
        assert_eq!(r.resolve("z", "Z", "z", STR).cap, 256);
        // app field entry overrides
        assert_eq!(r.resolve("a", "B", "c", SEQ).cap, 99);
        // workspace-only field entry survives
        assert_eq!(r.resolve("a", "B", "d", SEQ).cap, 20);
    }

    #[test]
    fn discover_walks_up_and_closest_wins() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        let app = ws.join("pkgs").join("app");
        fs::create_dir_all(&app).unwrap();

        // Workspace-root config: default + a field.
        fs::write(
            ws.join(CODEGEN_CONFIG_FILENAME),
            r#"
            [defaults]
            sequence = 64
            string = 256
            [fields]
            "a/B.c" = 10
            "a/B.d" = 20
            "#,
        )
        .unwrap();
        // App config: overrides one default + one field.
        fs::write(
            app.join(CODEGEN_CONFIG_FILENAME),
            r#"
            [defaults]
            sequence = 128
            [fields]
            "a/B.c" = 99
            "#,
        )
        .unwrap();

        let r = CapacityResolver::discover(&app, Some(ws)).unwrap();
        assert_eq!(r.resolve("z", "Z", "z", SEQ).cap, 128); // app default wins
        assert_eq!(r.resolve("z", "Z", "z", STR).cap, 256); // workspace default survives
        assert_eq!(r.resolve("a", "B", "c", SEQ).cap, 99); // app field wins
        assert_eq!(r.resolve("a", "B", "d", SEQ).cap, 20); // workspace-only survives
    }

    #[test]
    fn discover_empty_chain_is_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        let r = CapacityResolver::discover(tmp.path(), Some(tmp.path())).unwrap();
        assert_eq!(
            r.resolve("p", "M", "f", SEQ).cap,
            NROS_DEFAULT_SEQUENCE_CAPACITY
        );
    }

    #[test]
    fn explicit_config_wins_over_discovered() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(
            dir.join(CODEGEN_CONFIG_FILENAME),
            "[fields]\n\"a/B.c\" = 10\n",
        )
        .unwrap();
        let explicit = dir.join("override.toml");
        fs::write(&explicit, "[fields]\n\"a/B.c\" = 77\n").unwrap();

        let r = CapacityResolver::resolve_for(Some(&explicit), dir, Some(dir)).unwrap();
        assert_eq!(r.resolve("a", "B", "c", SEQ).cap, 77);
    }

    #[test]
    fn unknown_top_level_key_is_rejected() {
        let err = CapacityResolver::from_toml_str(
            r#"
            [defualts]   # typo
            sequence = 1
            "#,
        );
        assert!(err.is_err());
    }

    // ── phase-403 W0 — a stated cap vs the built-in fallback ────────────────

    /// `resolve` and `resolve_configured` must answer the same thing wherever a
    /// config entry matches — one precedence chain, not two — and differ ONLY
    /// at the level-6 fallthrough.
    #[test]
    fn a_configured_cap_and_the_resolved_cap_are_the_same_answer() {
        let r = CapacityResolver::from_toml_str(
            r#"
            [defaults]
            string = 16
            [packages.pkg]
            sequence = 32
            [types."pkg/M"]
            string = 48
            [fields]
            "pkg/M.f" = 64
            "#,
        )
        .unwrap();
        for (msg, field, kind) in [
            ("M", "f", STR),
            ("M", "g", STR),
            ("Other", "g", SEQ),
            ("Other", "g", STR),
        ] {
            assert_eq!(
                r.resolve_configured("pkg", msg, field, kind),
                Some(r.resolve("pkg", msg, field, kind)),
                "{msg}.{field}"
            );
        }
    }

    /// The one case where they differ, and the reason the split exists: nothing
    /// matched, so `resolve` hands back the built-in constant and
    /// `resolve_configured` says so.
    #[test]
    fn nothing_configured_is_reported_as_nothing_not_as_the_builtin() {
        let r = CapacityResolver::from_toml_str("[fields]\n\"other/T.x\" = 8\n").unwrap();
        assert_eq!(r.resolve_configured("pkg", "M", "f", STR), None);
        assert_eq!(
            r.resolve("pkg", "M", "f", STR).cap,
            NROS_DEFAULT_STRING_CAPACITY
        );
        // And therefore no bound: the fallback is what codegen does when told
        // nothing, never a claim about the wire.
        assert_eq!(r.declared_bound("pkg", "M", "f", STR), None);
    }

    /// RFC-0033's guarantee table, as code. `inline` promises the size is in the
    /// type; `heap` calls its cap a hint; `view` has no fixed capacity at all.
    #[test]
    fn only_an_inline_cap_is_a_bound() {
        assert!(StorageMode::Inline.cap_bounds_the_wire());
        assert!(!StorageMode::Heap.cap_bounds_the_wire());
        assert!(!StorageMode::View.cap_bounds_the_wire());

        for (mode, want) in [("inline", Some(24)), ("heap", None), ("view", None)] {
            let r = CapacityResolver::from_toml_str(&format!(
                "[fields]\n\"pkg/M.f\" = {{ cap = 24, mode = \"{mode}\" }}\n"
            ))
            .unwrap();
            // The STORAGE cap is 24 in every mode — that is unchanged, and it
            // is what makes this a bound question rather than a cap question.
            assert_eq!(r.resolve("pkg", "M", "f", STR).cap, 24);
            assert_eq!(r.declared_bound("pkg", "M", "f", STR), want, "mode {mode}");
        }
    }

    /// The int shorthand is `{ cap = n, mode = "inline" }`, so it bounds.
    #[test]
    fn the_integer_shorthand_bounds_because_it_means_inline() {
        let r = CapacityResolver::from_toml_str("[fields]\n\"pkg/M.f\" = 24\n").unwrap();
        assert_eq!(r.declared_bound("pkg", "M", "f", STR), Some(24));
    }

    /// A `[defaults]` line is a stated bound — someone wrote it. Only the
    /// built-in constant is not.
    #[test]
    fn a_defaults_entry_states_a_bound() {
        let r = CapacityResolver::from_toml_str("[defaults]\nstring = 16\n").unwrap();
        assert_eq!(r.declared_bound("any", "M", "f", STR), Some(16));
        // ...and it is kind-specific: nothing was said about sequences.
        assert_eq!(r.declared_bound("any", "M", "f", SEQ), None);
    }

    // ========================================================================
    // phase-403 W7 — `element_cap`, the second dimension
    // ========================================================================

    use rosidl_parser::ast::FieldType as F;

    fn string_seq() -> F {
        F::Sequence {
            element_type: Box::new(F::String),
        }
    }

    /// The headline: one key carries two numbers, and they land on the two
    /// dimensions `.msg` already distinguishes.
    #[test]
    fn one_entry_states_the_length_and_the_element_bound_separately() {
        let r = CapacityResolver::from_toml_str(
            "[fields]\n\"pkg/M.name\" = { cap = 16, element_cap = 32 }\n",
        )
        .unwrap();
        assert_eq!(r.declared_bound("pkg", "M", "name", SEQ), Some(16));
        assert_eq!(
            r.declared_element_bound("pkg", "M", "name", &string_seq()),
            Some(32)
        );
    }

    /// The `.msg` wins PER DIMENSION. `string<=10[]` capped with
    /// `element_cap = 32` keeps 10 — and it keeps it without a precedence rule,
    /// because a bounded element is no longer a dimension the config can name.
    #[test]
    fn a_msg_element_bound_wins_over_element_cap() {
        let r = CapacityResolver::from_toml_str(
            "[fields]\n\"pkg/M.name\" = { cap = 16, element_cap = 32 }\n",
        )
        .unwrap();
        let already_bounded = F::Sequence {
            element_type: Box::new(F::BoundedString(10)),
        };
        assert_eq!(
            r.declared_element_bound("pkg", "M", "name", &already_bounded),
            None,
            "the interface bound removes the dimension the config could speak about"
        );
        // The rewrite is therefore a no-op, and the shape keeps the .msg's 10.
        assert!(matches!(
            r.element_capped("pkg", "M", "name", &already_bounded).as_ref(),
            F::Sequence { element_type } if **element_type == F::BoundedString(10)
        ));
        // The LENGTH dimension is untouched by any of this.
        assert_eq!(r.declared_bound("pkg", "M", "name", SEQ), Some(16));
    }

    /// Only a mode whose cap is ENFORCED may bound, and that governs the element
    /// dimension exactly as it governs the field's own — a `view` field aliases
    /// the receive buffer, so nothing checks an element length there either.
    #[test]
    fn only_a_bounding_mode_bounds_the_element() {
        for (mode, want) in [("inline", Some(32)), ("heap", None), ("view", None)] {
            let r = CapacityResolver::from_toml_str(&format!(
                "[fields]\n\"pkg/M.name\" = {{ cap = 16, element_cap = 32, mode = \"{mode}\" }}\n"
            ))
            .unwrap();
            assert_eq!(
                r.declared_element_bound("pkg", "M", "name", &string_seq()),
                want,
                "mode {mode}"
            );
        }
    }

    /// A fixed array and a `.msg`-bounded sequence are not configurable shapes,
    /// so no `mode` key can reach them and their element bound is always
    /// enforced by the inline container they are emitted as. A `heap` entry
    /// naming one must not silently disable its element bound.
    #[test]
    fn a_non_configurable_shape_keeps_its_element_bound_whatever_the_mode_says() {
        let r = CapacityResolver::from_toml_str(
            "[fields]\n\"pkg/M.name\" = { cap = 16, element_cap = 32, mode = \"heap\" }\n",
        )
        .unwrap();
        for ty in [
            F::Array {
                element_type: Box::new(F::String),
                size: 4,
            },
            F::BoundedSequence {
                element_type: Box::new(F::String),
                max_size: 4,
            },
        ] {
            assert_eq!(
                r.declared_element_bound("pkg", "M", "name", &ty),
                Some(32),
                "{ty:?}"
            );
        }
    }

    /// The two dimensions walk the level chain INDEPENDENTLY. Naming a field to
    /// override its length must not silently delete the element default set
    /// once at `[defaults]` — that would make the layering a trap rather than a
    /// convenience.
    #[test]
    fn a_per_field_cap_does_not_shadow_a_defaults_element_cap() {
        let r = CapacityResolver::from_toml_str(
            "[defaults]\n\
             sequence = { cap = 8, element_cap = 32 }\n\
             [fields]\n\
             \"pkg/M.name\" = 64\n",
        )
        .unwrap();
        assert_eq!(
            r.declared_bound("pkg", "M", "name", SEQ),
            Some(64),
            "the field entry wins the LENGTH"
        );
        assert_eq!(
            r.declared_element_bound("pkg", "M", "name", &string_seq()),
            Some(32),
            "and the default still supplies the ELEMENT"
        );
    }

    /// Every level supplies an element default, closest wins — the same chain
    /// `cap` walks.
    #[test]
    fn the_element_bound_resolves_through_every_level() {
        let r = CapacityResolver::from_toml_str(
            "[defaults]\n\
             sequence = { cap = 8, element_cap = 4 }\n\
             [packages.pkg]\n\
             sequence = { cap = 8, element_cap = 8 }\n\
             [types.\"pkg/M\"]\n\
             sequence = { cap = 8, element_cap = 16 }\n\
             [fields]\n\
             \"pkg/M.named\" = { cap = 8, element_cap = 32 }\n",
        )
        .unwrap();
        let e = |msg: &str, f: &str| r.declared_element_bound("pkg", msg, f, &string_seq());
        assert_eq!(e("M", "named"), Some(32), "field");
        assert_eq!(e("M", "other"), Some(16), "type");
        assert_eq!(e("N", "other"), Some(8), "package");
        assert_eq!(
            r.declared_element_bound("elsewhere", "N", "other", &string_seq()),
            Some(4),
            "defaults"
        );
    }

    /// A `[fields]` `element_cap` on a field with no element dimension is an
    /// ERROR naming the field. Silence here is the failure mode the key exists
    /// to remove: the user believes they bounded a type that codegen still
    /// reports unbounded.
    #[test]
    fn element_cap_on_a_field_with_no_elements_is_an_error_naming_it() {
        let r = CapacityResolver::from_toml_str(
            "[fields]\n\
             \"pkg/M.label\" = { cap = 16, element_cap = 32 }\n\
             \"pkg/M.counts\" = { cap = 16, element_cap = 32 }\n\
             \"pkg/M.name\" = { cap = 16, element_cap = 32 }\n",
        )
        .unwrap();
        let fields = vec![
            rosidl_parser::ast::Field {
                name: "label".into(),
                field_type: F::String,
                default_value: None,
            },
            rosidl_parser::ast::Field {
                name: "counts".into(),
                field_type: F::Sequence {
                    element_type: Box::new(F::Primitive(rosidl_parser::ast::PrimitiveType::Int32)),
                },
                default_value: None,
            },
            rosidl_parser::ast::Field {
                name: "name".into(),
                field_type: string_seq(),
                default_value: None,
            },
        ];
        let errs = r.element_cap_shape_errors("pkg", "M", &fields);
        // EVERY offender, not the first: fixing a config one build at a time is
        // the loop phase-403 W0 removed for unbounded members.
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert_eq!(errs[0].field, "label");
        assert!(errs[0].to_string().contains("pkg/M.label"), "{}", errs[0]);
        assert!(errs[0].to_string().contains("a string"), "{}", errs[0]);
        assert_eq!(errs[1].field, "counts");
        assert!(
            errs[1].to_string().contains("a sequence of int32"),
            "{}",
            errs[1]
        );
    }

    /// A LEVEL entry is a default, and a default that does not apply to a given
    /// field is how defaults work. Only a `[fields]` entry NAMES one field, so
    /// only it can be wrong about that field.
    #[test]
    fn a_level_element_cap_reaching_a_string_field_is_not_an_error() {
        let r = CapacityResolver::from_toml_str(
            "[defaults]\nsequence = { cap = 8, element_cap = 32 }\n",
        )
        .unwrap();
        let fields = vec![rosidl_parser::ast::Field {
            name: "label".into(),
            field_type: F::String,
            default_value: None,
        }];
        assert!(r.element_cap_shape_errors("pkg", "M", &fields).is_empty());
        assert_eq!(
            r.declared_element_bound("pkg", "M", "label", &F::String),
            None
        );
    }

    /// `element_cap` under a `string` LEVEL key can never apply — a string field
    /// has no elements — so it is rejected at parse time, where it needs no
    /// message to detect.
    #[test]
    fn element_cap_under_a_string_level_key_is_rejected_at_parse() {
        for body in [
            "[defaults]\nstring = { cap = 16, element_cap = 32 }\n",
            "[packages.pkg]\nstring = { cap = 16, element_cap = 32 }\n",
            "[types.\"pkg/M\"]\nstring = { cap = 16, element_cap = 32 }\n",
        ] {
            match CapacityResolver::from_toml_str(body) {
                Err(ConfigError::ElementCapOnStringLevel { level }) => {
                    assert!(!level.is_empty())
                }
                other => panic!("expected ElementCapOnStringLevel for {body:?}, got {other:?}"),
            }
        }
        // The same key on `sequence` is exactly where it belongs.
        assert!(
            CapacityResolver::from_toml_str(
                "[defaults]\nsequence = { cap = 16, element_cap = 32 }\n"
            )
            .is_ok()
        );
    }

    /// The rewrite produces the shape the `.msg` would have produced, and
    /// nothing else — the claim `with_element_bound`'s doc makes, checked
    /// against every shape it can be handed.
    #[test]
    fn the_rewrite_spells_the_bound_the_msg_would_have() {
        assert_eq!(
            *with_element_bound(&string_seq(), Some(32)),
            F::Sequence {
                element_type: Box::new(F::BoundedString(32))
            }
        );
        assert_eq!(
            *with_element_bound(
                &F::BoundedSequence {
                    element_type: Box::new(F::WString),
                    max_size: 5
                },
                Some(32)
            ),
            F::BoundedSequence {
                element_type: Box::new(F::BoundedWString(32)),
                max_size: 5
            },
            "the array suffix survives untouched -- the dimensions are independent"
        );
        // No bound, a non-string element, and a non-array shape are all no-ops.
        for (ty, bound) in [
            (string_seq(), None),
            (
                F::Sequence {
                    element_type: Box::new(F::Primitive(rosidl_parser::ast::PrimitiveType::Int32)),
                },
                Some(32),
            ),
            (F::String, Some(32)),
        ] {
            assert!(matches!(
                with_element_bound(&ty, bound),
                std::borrow::Cow::Borrowed(_)
            ));
        }
    }
}
