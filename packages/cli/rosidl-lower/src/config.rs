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

/// A config entry value: either an integer (owned shorthand) or `{ cap, mode }`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
enum CapEntry {
    /// `field = 64` → `{ cap = 64, mode = "inline" }`.
    Int(usize),
    /// `field = { cap = 2_000_000, mode = "view" }`.
    Table {
        cap: usize,
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
            CapEntry::Table { cap, mode } => FieldStorage { cap, mode },
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
        Ok(Self {
            raw: toml::from_str(s)?,
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
        // 2 — per-field
        let field_key = format!("{package}/{message}.{field}");
        if let Some(e) = self.raw.fields.get(&field_key) {
            return e.resolve();
        }
        // 3 — per-type
        let type_key = format!("{package}/{message}");
        if let Some(e) = self.raw.types.get(&type_key).and_then(|l| l.pick(kind)) {
            return e.resolve();
        }
        // 4 — per-package
        if let Some(e) = self.raw.packages.get(package).and_then(|l| l.pick(kind)) {
            return e.resolve();
        }
        // 5 — global defaults
        if let Some(e) = self.raw.defaults.pick(kind) {
            return e.resolve();
        }
        // 6 — built-in
        FieldStorage {
            cap: kind.builtin_default(),
            mode: StorageMode::Inline,
        }
    }
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
}
