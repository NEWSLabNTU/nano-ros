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

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::types::{NROS_DEFAULT_SEQUENCE_CAPACITY, NROS_DEFAULT_STRING_CAPACITY};

/// How a field's local storage is realized. See RFC-0033 "Storage modes".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
    /// `heapless::Vec<T, N>` / fixed `[N]` array — `N` elems always inline.
    #[default]
    Owned,
    /// `alloc::Vec<T>` — dynamic, needs `alloc`/`std` (Phase 229.5).
    Heap,
    /// `&'a [T]` into the CDR receive buffer — zero-copy (Phase 229.6 / issue 0007).
    Borrowed,
}

impl StorageMode {
    /// Phase-1 supports only `owned`; `heap`/`borrowed` land in 229.5 / 229.6.
    pub fn is_phase1_supported(self) -> bool {
        matches!(self, StorageMode::Owned)
    }

    /// Token used in config + diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            StorageMode::Owned => "owned",
            StorageMode::Heap => "heap",
            StorageMode::Borrowed => "borrowed",
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
    /// `field = 64` → `{ cap = 64, mode = "owned" }`.
    Int(usize),
    /// `field = { cap = 2_000_000, mode = "borrowed" }`.
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
                mode: StorageMode::Owned,
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

/// Error parsing a `nros-codegen.toml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to parse codegen config: {0}")]
    Parse(#[from] toml::de::Error),
}

/// Resolves per-field storage from a merged `nros-codegen.toml`. One instance
/// feeds all three language backends.
#[derive(Debug, Clone, Default)]
pub struct CapacityResolver {
    raw: RawConfig,
}

impl CapacityResolver {
    /// An empty resolver — every field falls through to its built-in default,
    /// reproducing pre-Phase-229 output byte-for-byte.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Parse a single `nros-codegen.toml` body.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        Ok(Self {
            raw: toml::from_str(s)?,
        })
    }

    /// Merge another config on top of this one; `over` (e.g. the app file) wins
    /// over `self` (e.g. the workspace file).
    pub fn merged_with(mut self, over: CapacityResolver) -> Self {
        self.raw.merge_over(over.raw);
        self
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
            mode: StorageMode::Owned,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEQ: FieldKind = FieldKind::Sequence;
    const STR: FieldKind = FieldKind::String;

    #[test]
    fn empty_config_yields_builtin_defaults() {
        let r = CapacityResolver::empty();
        let s = r.resolve("std_msgs", "String", "data", STR);
        assert_eq!(s.cap, NROS_DEFAULT_STRING_CAPACITY);
        assert_eq!(s.mode, StorageMode::Owned);
        let q = r.resolve("sensor_msgs", "Image", "data", SEQ);
        assert_eq!(q.cap, NROS_DEFAULT_SEQUENCE_CAPACITY);
        assert_eq!(q.mode, StorageMode::Owned);
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
                mode: StorageMode::Owned
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
                mode: StorageMode::Borrowed
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
    fn mode_phase1_gate() {
        assert!(StorageMode::Owned.is_phase1_supported());
        assert!(!StorageMode::Heap.is_phase1_supported());
        assert!(!StorageMode::Borrowed.is_phase1_supported());
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
