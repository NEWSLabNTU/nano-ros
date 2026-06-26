//! Phase 212.B — `[workspace.metadata.nros]` + `[package.metadata.nros]` +
//! `[package.metadata.ament]` + `<bringup>/system.toml` data contracts.
//!
//! These are the *user-authored* TOML surfaces introduced by Phase 212. They
//! live in standard cargo manifest tables (`[workspace.metadata.…]` /
//! `[package.metadata.…]`) so that cargo treats them as opaque user data and
//! pure-cargo workflows (no CLI build wrapper) keep working. The
//! `<bringup>/system.toml` is the per-system declarative file owned by the
//! `<system>_bringup` package.
//!
//! Vocabulary discipline (per the Phase 212 doc): every field name is a strict
//! subset of names that already appear in `nros-sdk-index.toml`,
//! `app_config.h`, or the existing planner schema. No second TOML dialect.
//!
//! Every struct here uses `#[serde(deny_unknown_fields)]` so typos surface as
//! parse errors at the user's terminal instead of being silently dropped.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// Phase 228.E — the per-tier schema types live in the shared
// `nros-orchestration-ir` crate (single source of truth, also consumed by the
// `nros::main!()` proc-macro). Re-exported here so the rest of the CLI keeps
// referencing them through `cargo_metadata_schema::` unchanged.
pub use nros_orchestration_ir::{
    CallbackGroupDecl, CallbackGroupOverride, NodeOverride, TierDef, TierRtosSpec,
};

use super::schema::RemapRule;

// ---------------------------------------------------------------------------
// Workspace-root metadata: `[workspace.metadata.nros]`
// ---------------------------------------------------------------------------

/// `[workspace.metadata.nros]` in a workspace-root `Cargo.toml`.
///
/// Thin pointer (see `docs/design/0024-multi-node-workspace-layout.md` §5). The
/// authoritative system spec lives in `<bringup>/system.toml`; this table
/// only disambiguates which bringup the workspace defaults to plus a small
/// set of rarely-used workspace-wide overrides.
///
/// Per the Phase 212.L.7 redesign, `default_system` may name EITHER a
/// bringup package (`<system>_bringup`) OR a Node/Entry pkg that eats its
/// own Entry role (single-pkg `cargo run` dev loop). The launcher walks
/// the workspace and resolves the pointer against either category.
///
/// All fields are optional; an absent `[workspace.metadata.nros]` table
/// parses as `WorkspaceMetadataNros::default()`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMetadataNros {
    /// Bringup package name (`<system>_bringup`) OR Entry pkg name
    /// (Phase 212.L.7 self-entry shape). `nros plan` /
    /// `nros codegen-system` with no `--bringup` hint resolves the
    /// system via this pointer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_system: Option<String>,
    /// Optional workspace-wide RMW override — rare, intended for
    /// `nros plan --override` workflows. Values are `"zenoh"` /
    /// `"xrce"` / `"cyclonedds"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rmw_override: Option<String>,
    /// Optional workspace-wide `ROS_DOMAIN_ID` override. When present,
    /// `nros plan` / `nros codegen-system` propagate it into the
    /// generated `system_config.h` instead of the per-deploy /
    /// `[system].domain_id` value. Rare — used for one-off bring-ups
    /// against a shared ROS 2 graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_id_override: Option<u32>,
}

// ---------------------------------------------------------------------------
// Per-package metadata: `[package.metadata.nros]`
// ---------------------------------------------------------------------------

/// `[package.metadata.nros]` in a component / application package's
/// `Cargo.toml`.
///
/// Three top-level shapes (mutually exclusive):
///
/// * Single-node crate — `[package.metadata.nros.node]` describes the
///   one node the crate exposes. (The Phase 212.N.12 rename made
///   `node` the canonical key; `[package.metadata.nros.component]`
///   remains accepted as a deprecated alias — declaring both is a
///   hard error. See `nros_config::parse_package_metadata_nros`.)
/// * Multi-component crate — `[package.metadata.nros.components.<Name>]`
///   table-of-tables enumerates each.
/// * Application crate — `[package.metadata.nros.application]` describes a
///   native-only application pkg (per Phase 212.L.2).
///
/// Phase 212.L.7 also adds an optional per-target deploy table at
/// `[package.metadata.nros.deploy.<target>]`, used both by application pkgs
/// and by self-bringup component pkgs (component pkg w/ `[deploy.*]` and no
/// sibling bringup acts as its own bringup).
///
/// At most one of `component` / `components` / `application` may be present;
/// the loader validates this after deserialization (a serde untagged enum
/// would lose the precise `deny_unknown_fields` error, so we keep the fields
/// flat and reject conflicts in [`PackageMetadataNros::validate`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMetadataNros {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<ComponentMetadata>,
    /// Phase 212.N.12 (in-flight) — `node` is the forward-looking spelling of
    /// the `component` shape. The reader accepts BOTH spellings during the
    /// in-flight rename (Phase 212.B.2 task spec). Mutually exclusive with
    /// `component` / `components` / `nodes` / `application` (validated below).
    /// The shape is identical to [`ComponentMetadata`] so codegen can treat
    /// the two interchangeably until N.12 retires the `component` spelling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<ComponentMetadata>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: BTreeMap<String, ComponentMetadata>,
    /// Phase 212.N.12 in-flight — `nodes` is the forward-looking spelling
    /// of the `components` (multi-shape) table. Same shape, accepted as an
    /// alias during the rename. Mutually exclusive with `components`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nodes: BTreeMap<String, ComponentMetadata>,
    /// Phase 212.L.2 — `[package.metadata.nros.application]`. Application
    /// pkgs are native-only orchestration roots; they MUST NOT name an RTOS
    /// in their `deploy = […]` allow-list. (The `nros check` lint enforces
    /// the no-RTOS rule; the schema only accepts the field.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application: Option<ApplicationMetadata>,
    /// Phase 212.N.7 — `[package.metadata.nros.entry]`. Entry pkgs declare
    /// which `[deploy.<target>]` block they run on (the firmware bin pulls
    /// in the per-board shim + emits `run_plan(runtime)`). Strict schema —
    /// `deploy = "<board>"` is the only field today.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<EntryMetadata>,
    /// Phase 212.L.7 / L.8 — per-target deploy tables, keyed by target name
    /// (`native`, `qemu-mps2-an385`, …). Populates both application pkgs and
    /// self-bringup component pkgs (component pkg w/ `[deploy.*]` and no
    /// sibling bringup eats its own bringup role).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deploy: BTreeMap<String, DeployTargetMetadata>,
    /// Phase 212.B.2 stub (`[package.metadata.nros.domain]`) — opaque
    /// pass-through during the schema in-flight window. The full typed shape
    /// lands with system.toml's F.4 work. Captured as `toml::Value` so
    /// `deny_unknown_fields` still surfaces typos elsewhere while letting
    /// users author the table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<toml::Value>,
    /// Phase 212.B.2 stub (`[package.metadata.nros.bridge]`) — opaque
    /// pass-through, same rationale as `domain`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<toml::Value>,
    /// Phase 212.B.2 stub (`[package.metadata.nros.embedded]`) — opaque
    /// pass-through. Will eventually hold board-specific embedded knobs
    /// (`linker_script` / `stack_size` / …); kept opaque so the reader
    /// doesn't break the moment a board author authors the table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedded: Option<toml::Value>,
}

impl PackageMetadataNros {
    /// Reject manifests that combine more than one of `component` /
    /// `node` / `components` / `nodes` / `application` — the shapes are
    /// mutually exclusive per the Phase 212.L design doc plus the Phase
    /// 212.N.12 rename (the `node` / `nodes` spellings are aliases of
    /// `component` / `components`, not new categories).
    pub fn validate(&self) -> Result<(), String> {
        let has_component = self.component.is_some();
        let has_node = self.node.is_some();
        let has_components = !self.components.is_empty();
        let has_nodes = !self.nodes.is_empty();
        let has_application = self.application.is_some();
        let count = [
            has_component,
            has_node,
            has_components,
            has_nodes,
            has_application,
        ]
        .into_iter()
        .filter(|b| *b)
        .count();
        if count > 1 {
            return Err("`[package.metadata.nros]` carries more than one of \
                 `component` / `node` / `components` / `nodes` / `application` — pick exactly \
                 one shape (Phase 212.L.2 / L.7; N.12 rename in flight — `node` / \
                 `nodes` are the forward-looking spellings of `component` / `components`)"
                .to_string());
        }
        Ok(())
    }

    /// Phase 212.N.12 in-flight rename — `component` and `node` are aliases.
    /// Returns the present shape, preferring `node` (the forward spelling)
    /// over `component`. Callers reading the per-pkg shape go through this
    /// accessor so the N.12 sweep can later flip the storage field without
    /// touching every read site.
    pub fn node_or_component(&self) -> Option<&ComponentMetadata> {
        self.node.as_ref().or(self.component.as_ref())
    }

    /// Phase 212.N.12 in-flight rename — multi-shape accessor. Returns
    /// `nodes` when present, else `components`. Empty if neither populated.
    pub fn nodes_or_components(&self) -> &BTreeMap<String, ComponentMetadata> {
        if !self.nodes.is_empty() {
            &self.nodes
        } else {
            &self.components
        }
    }

    /// True when this manifest is a *self-bringup-eligible* component or
    /// application pkg (Phase 212.L.7): it declares its component/application
    /// surface AND at least one `[package.metadata.nros.deploy.<target>]`
    /// table. The planner / codegen path treats such a pkg as its own
    /// degenerate 1-component bringup when no sibling bringup pkg points at
    /// it.
    pub fn is_self_bringup_eligible(&self) -> bool {
        let has_role = self.component.is_some()
            || self.node.is_some()
            || !self.components.is_empty()
            || !self.nodes.is_empty()
            || self.application.is_some();
        has_role && !self.deploy.is_empty()
    }
}

/// `[package.metadata.nros.entry]` — Phase 212.N.7.
///
/// Marks an Entry pkg (the firmware bin) so the planner can route it to the
/// right `[deploy.<target>]` block. Today the only field is `deploy =
/// "<board>"` (the deploy-target key in the workspace deploy map). The
/// reader keeps this strict so a typo on `deploy =` surfaces immediately.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryMetadata {
    /// Board / deploy-target key (e.g. `"freertos"`, `"zephyr"`).
    pub deploy: String,
}

/// `[package.metadata.nros.node]` (single shape, canonical post Phase
/// 212.N.12) — or `[package.metadata.nros.component]` (deprecated alias)
/// — or `[package.metadata.nros.components.<Name>]` (multi shape).
///
/// Pure deployment intent — no build-system knobs (Cargo + CMake own those).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentMetadata {
    /// Phase 212.L.4 — fully-qualified class name (`<pkg-dir>::<UserClass>`).
    /// Lint-enforced by `nros check` to match the host pkg name; codegen
    /// uses it to land at the right Rust type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Short component instance name (used as the planner / codegen
    /// instance identifier when the pkg is its own self-bringup, per
    /// Phase 212.L.7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Default namespace the component is mounted at. Absent → `/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_namespace: Option<String>,
    /// Raw ROS parameter declarations. Values stay as `toml::Value` here so
    /// the planner can do its own type-aware lowering (mirrors the existing
    /// `params::ParameterTable` resolution path).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, toml::Value>,
    /// `from` → `to` topic / service remaps, mirroring rclpy / rclcpp.
    /// Aliased to [`RemapRule`] (already `{from, to}`-shaped in
    /// `super::schema`) to avoid creating a duplicate type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remaps: Vec<RemapRule>,
    /// Phase 228.A (RFC-0015) — callback groups the node declares (it owns its
    /// callbacks). Each names a symbolic `tier` the system's `[tiers.*]` maps to
    /// an RTOS task/priority. Empty → all callbacks default to the `"default"`
    /// tier (the single-task degenerate case).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callback_groups: Vec<CallbackGroupDecl>,
    /// phase-267 W1c/C3a — topics this node PUBLISHES, declared by the node
    /// author (`[[package.metadata.nros.node.publishes]] topic=… type=…`). Read
    /// as SYNTHETIC metadata (pre-build, no sidecar) so the planner can resolve a
    /// `[[bridge]]`'s topic NAMES to their ROS types without building. Empty ⇒ the
    /// node declares no publishers in metadata (entity resolution falls back to
    /// the post-build sidecar, as before).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publishes: Vec<TopicDecl>,
    /// phase-267 W1c/C3a — topics this node SUBSCRIBES to. Same shape + role as
    /// [`publishes`](Self::publishes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscribes: Vec<TopicDecl>,
}

/// phase-267 W1c/C3a — one declared topic endpoint in node Cargo metadata:
/// `{ topic = "/chatter", type = "std_msgs/Int32" }`. The `type` is the ROS type
/// name (`<pkg>/<Msg>` or `<pkg>/msg/<Msg>`); the planner resolves a bridge's
/// topic name to it pre-build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopicDecl {
    pub topic: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// `[package.metadata.nros.application]` — Phase 212.L.2.
///
/// Application pkgs are native-only orchestration roots: they wire several
/// component pkgs together but MUST NOT name an RTOS target in their
/// `deploy = […]` allow-list. The allow-list semantics are enforced by
/// `nros check`; the schema only accepts the field.
///
/// The application's per-target deploy block lives on the outer
/// `[package.metadata.nros.deploy.<target>]` (shared with self-bringup
/// component pkgs), not nested under `[application]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationMetadata {
    /// Allow-list of deploy targets — keys into the outer
    /// `[package.metadata.nros.deploy.<target>]` map. Must not include an
    /// RTOS target name (lint-enforced by `nros check`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deploy: Vec<String>,
    /// Optional short app name; falls back to the Cargo `[package].name`
    /// when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// `[package.metadata.nros.deploy.<target>]` — Phase 212.L.8.
///
/// Per-target deploy parameters baked into the bringup tree by `nros
/// codegen-system` (and recorded into `nros-plan.json` by `nros plan`).
/// Used by application pkgs and by self-bringup component pkgs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeployTargetMetadata {
    /// Board identifier (e.g. `native_sim/native/64`, `mps2-an385`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    /// RMW backend (`zenoh` / `xrce` / `cyclonedds`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rmw: Option<String>,
    /// Baked ROS_DOMAIN_ID — embedded targets bake at build time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<u32>,
    /// Optional RMW locator URI (e.g. `tcp/127.0.0.1:7447`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
}

/// Convenience alias: spec calls these `RemapEntry`. The existing
/// `RemapRule` already has the right shape, so we expose both names.
pub type RemapEntry = RemapRule;

// ---------------------------------------------------------------------------
// Per-package metadata: `[package.metadata.ament]`
// ---------------------------------------------------------------------------

/// `[package.metadata.ament]` — the source of truth for `nros emit
/// package-xml` (Phase 212.G). Mirrors ament/colcon's `package.xml`
/// vocabulary 1-to-1.
///
/// Vocabulary (Phase 212.B.4):
///
/// * `description` / `license` — passthrough to `<description>` /
///   `<license>` in the emitted `package.xml`. When absent the emitter
///   falls back to a synthesised description + `"Apache-2.0"`.
/// * `maintainer = { name, email }` — populates the single
///   `<maintainer email="…">…</maintainer>` row. Multiple maintainers
///   are not modelled yet — ROS allows several `<maintainer>` rows,
///   but every in-tree fixture authors at most one.
/// * `build_depend` / `exec_depend` / `test_depend` /
///   `buildtool_depend` — each row emits a corresponding `<*_depend>`
///   entry. Sorted + deduped at emit time so list ordering doesn't
///   drift between edits.
/// * `build_type` — `<export><build_type>…</build_type></export>`.
///   Defaults differ between component pkgs (`ament_cargo`) and
///   bringup pkgs (`ament_cmake`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMetadataAment {
    /// `<description>` body. Absent → emitter synthesises a generic
    /// "`nano-ros component package <name>`" line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `<license>` body (SPDX identifier). Absent → `"Apache-2.0"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// `<maintainer email="…">name</maintainer>` row. Absent →
    /// emitter falls back to a placeholder `Developer <dev@example.com>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintainer: Option<AmentMaintainer>,
    /// `<build_depend>` rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_depend: Vec<String>,
    /// `<buildtool_depend>` rows (e.g. `"ament_cargo"`, `"ament_cmake"`).
    /// Phase 212.B.4: explicit dependency category so users opting in
    /// to colcon interop can author `buildtool_depend = ["ament_cmake"]`
    /// without polluting `<build_depend>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buildtool_depend: Vec<String>,
    /// `<exec_depend>` rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exec_depend: Vec<String>,
    /// `<test_depend>` rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_depend: Vec<String>,
    /// `<export><build_type>…</build_type></export>` body. Component
    /// pkgs default to `"ament_cargo"`; bringup pkgs default to
    /// `"ament_cmake"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_type: Option<String>,
}

/// `maintainer = { name = "…", email = "…" }` — Phase 212.B.4.
///
/// Modelled as a strict struct so a stray `affiliation = "…"` or
/// `github = "…"` typo surfaces as `unknown field` at parse time.
/// Both `name` and `email` are mandatory when the table is present;
/// `package.xml` requires the email attribute and a non-empty body,
/// so we mirror that policy verbatim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmentMaintainer {
    pub name: String,
    pub email: String,
}

// ---------------------------------------------------------------------------
// Per-bringup file: `<bringup>/system.toml`
// ---------------------------------------------------------------------------

/// `<bringup>/system.toml` — the authoritative system spec.
///
/// Sections (see `docs/design/0025-workspace-layout-by-case.md` Case 3/4 and
/// `0024-multi-node-workspace-layout.md` §4):
///
/// * `[system]` — name, RMW, domain, optional locator.
/// * `[[component]]` — one entry per node/component.
/// * `[deploy.<target>]` — per-target deploy block (`kind = "self" | "qemu"
///   | "flash" | …`).
/// * `[[domain]]` — optional per-system domain routing.
/// * `[[bridge]]` — optional cross-RMW bridges.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemToml {
    pub system: SystemHeader,
    #[serde(default, rename = "component", skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<SystemComponentEntry>,
    /// `[deploy.<target>]` — keyed by target name (e.g. `native`,
    /// `qemu-mps2-an385`, `flash-stm32f4-disco`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deploy: BTreeMap<String, DeployTarget>,
    #[serde(default, rename = "domain", skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<SystemDomainEntry>,
    #[serde(default, rename = "bridge", skip_serializing_if = "Vec::is_empty")]
    pub bridges: Vec<SystemBridgeEntry>,
    /// Phase 228.A (RFC-0015 §4.2) — `[tiers.<name>]` priority/scheduling tiers.
    /// The system owner maps the symbolic tier names a node's callback groups
    /// reference to per-RTOS task knobs. Empty → the single-tier degenerate case.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tiers: BTreeMap<String, TierDef>,
    /// Phase 228.A — `[[node_overrides]]` deployment-time tier reassignment of a
    /// node's callback groups (RFC-0015 §4.2), without touching the node package.
    #[serde(
        default,
        rename = "node_overrides",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub node_overrides: Vec<NodeOverride>,
    /// Phase 254 — declared capability axes (RFC-0031 §Generalization), the
    /// single typed home read by BOTH codegen paths (the Rust planner + the
    /// C/C++ bake). Supersedes the transitional per-package `nros.toml`
    /// capability overlays. `[safety]` — E2E message-integrity (CRC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety: Option<SystemSafety>,
    /// Phase 254 — `[param_services]`: the external ROS 2 parameter server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param_services: Option<SystemParamServices>,
    /// Phase 256 Wave 1 — `[lifecycle]`: the managed-node boot autostart state,
    /// the typed home superseding the per-package `nros.toml` `[lifecycle]`
    /// overlay. Read by the planner (`PlanLifecycle`). Absent ⇒ no lifecycle
    /// block (node stays a plain node).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<SystemLifecycle>,
    // Phase 256 — `[param_persistence]` is DISABLED at the config surface: the
    // feature is incomplete (only the hosted `file` `ParamStore` backend exists;
    // the embedded flash/NVS backends are unbuilt — issue 0080). The typed field
    // is intentionally absent, so `deny_unknown_fields` REJECTS a
    // `[param_persistence]` block until the backends land. The runtime `ParamStore`
    // seam (`nros-params`) + the codegen path are kept dormant for re-enable.
}

/// Phase 256 Wave 1 — `[lifecycle]` in `system.toml`. `autostart` ∈
/// `none` | `configure` | `active` (the `LifecycleAutostart` plan enum);
/// defaults to `none` (services registered, externally driven).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemLifecycle {
    #[serde(default = "default_autostart")]
    pub autostart: String,
}

fn default_autostart() -> String {
    "none".to_string()
}

/// Phase 254 — `[safety]` in `system.toml`: E2E message-integrity (CRC + sequence
/// gap/dup). Mirrors the `PlanSafety` shape; `enabled = false` opts out even when
/// the block is present.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemSafety {
    #[serde(default = "default_true_cap")]
    pub enabled: bool,
    #[serde(default = "default_true_cap")]
    pub crc: bool,
}

/// Phase 254 — `[param_services]` in `system.toml`: the external parameter server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemParamServices {
    #[serde(default = "default_true_cap")]
    pub enabled: bool,
}

fn default_true_cap() -> bool {
    true
}

/// Phase 261 W4 — shared capability validation for both codegen paths (planner +
/// bake): reject an unknown `[system].features` entry (typo guard, hard error) and
/// warn once per deprecated typed capability block (`[safety]` / `[param_services]`),
/// steering authors to `features = [...]`.
pub fn validate_and_warn_capabilities(sys: &SystemToml) -> eyre::Result<()> {
    let unknown = sys.unknown_features();
    if !unknown.is_empty() {
        let known: Vec<&str> = cargo_nano_ros::capability_resolver::CAPABILITIES
            .iter()
            .map(|c| c.declared)
            .collect();
        eyre::bail!(
            "unknown capability in [system].features: {} (known axes: {})",
            unknown.join(", "),
            known.join(", ")
        );
    }
    for blk in sys.deprecated_typed_capability_blocks() {
        eprintln!(
            "warning: the typed `[{blk}]` block is deprecated (phase-261); declare \
             `features = [\"{blk}\"]` under `[system]` instead"
        );
    }
    Ok(())
}

impl SystemToml {
    /// Phase 261 — is the declared capability axis (`capability_resolver::Capability
    /// .declared`, e.g. `"safety"` / `"param_services"`) enabled in this system?
    /// Maps the registry's language-neutral axis name onto the typed `[block]
    /// enabled` field, so the bake/generate can drive the C/C++ `#define` + Rust
    /// feature lowering from a registry loop instead of hardcoded per-axis branches.
    /// Unknown axis ⇒ `false`.
    pub fn capability_enabled(&self, declared: &str) -> bool {
        // Phase 261 W4 — the generic `[system].features = [...]` surface enables an
        // axis by declared name, equivalently to the typed `[<name>] enabled = true`
        // block. Either source flips the axis on.
        if self.system.features.iter().any(|f| f == declared) {
            return true;
        }
        match declared {
            "safety" => self.safety.as_ref().is_some_and(|s| s.enabled),
            "param_services" => self.param_services.as_ref().is_some_and(|p| p.enabled),
            _ => false,
        }
    }

    /// Phase 261 W4 — validate `[system].features`: every entry must name a known
    /// capability axis (`capability_resolver::capability`), else a hard error (typo
    /// guard). Returns the unknown names; empty ⇒ all valid.
    pub fn unknown_features(&self) -> Vec<String> {
        self.system
            .features
            .iter()
            .filter(|f| cargo_nano_ros::capability_resolver::capability(f).is_none())
            .cloned()
            .collect()
    }

    /// Phase 261 W4 — the typed capability blocks (`[safety]`, `[param_services]`)
    /// present in this system. Deprecated in favour of `[system].features = [...]`;
    /// callers warn so authors migrate. Returns the declared axis names.
    pub fn deprecated_typed_capability_blocks(&self) -> Vec<&'static str> {
        let mut blocks = Vec::new();
        if self.safety.is_some() {
            blocks.push("safety");
        }
        if self.param_services.is_some() {
            blocks.push("param_services");
        }
        blocks
    }

    /// Phase 256 — the deploy target a target-agnostic caller (the planner) should
    /// resolve per-target values against, when no explicit target was selected:
    /// `cli` (a `--target` flag) → `[system].default_target` → the sole
    /// `[deploy.<t>]` key when exactly one is declared → `None`. This is the
    /// shared "which deploy am I?" key the per-target classes (RMW override,
    /// build tuning, domain/locator override) resolve through.
    pub fn resolve_target(&self, cli: Option<&str>) -> Option<String> {
        if let Some(c) = cli {
            return Some(c.to_string());
        }
        if let Some(t) = &self.system.default_target {
            return Some(t.clone());
        }
        if self.deploy.len() == 1 {
            return self.deploy.keys().next().cloned();
        }
        None
    }

    /// Phase 256 Wave 8 — the ROS domain id for `target`: `[deploy.<target>].domain_id`
    /// overrides `[system].domain_id`. The RFC-0004 §3.1 ladder for `domain_id`
    /// (CLI flag is a future rung). Both codegen paths resolve through this.
    pub fn resolved_domain_id(&self, target: Option<&str>) -> u32 {
        target
            .and_then(|t| self.deploy.get(t))
            .and_then(|dt| dt.domain_id)
            .unwrap_or(self.system.domain_id)
    }

    /// Phase 256 Wave 8 — the locator for `target`: `[deploy.<target>].locator`
    /// overrides `[system].locator` (`None` when neither sets it).
    pub fn resolved_locator(&self, target: Option<&str>) -> Option<String> {
        target
            .and_then(|t| self.deploy.get(t))
            .and_then(|dt| dt.locator.clone())
            .or_else(|| self.system.locator.clone())
    }

    /// Phase 255 — the RMW backend name for `target`, applying the RFC-0031
    /// precedence (highest wins): the CLI `--rmw` flag, then `[deploy.<target>].rmw`,
    /// then `[system].rmw`, then the `"zenoh"` default. Both codegen paths (the
    /// planner's board-feature lowering + the bake's C `#define`) resolve through
    /// this single helper so a given target gets exactly one RMW — no duality.
    pub fn resolved_rmw(&self, target: Option<&str>, cli: Option<&str>) -> String {
        if let Some(c) = cli {
            return c.to_string();
        }
        if let Some(t) = target
            && let Some(dt) = self.deploy.get(t)
            && let Some(r) = &dt.rmw
        {
            return r.clone();
        }
        if self.system.rmw.is_empty() {
            "zenoh".to_string()
        } else {
            self.system.rmw.clone()
        }
    }

    /// Phase 255 Wave 5 — the multi-RMW link set a single binary needs when it
    /// hosts cross-RMW `[[bridge]]`s. A single binary must link the **union** of
    /// every bridged session's RMW so it can speak both sides. Returned in
    /// declaration order, deduped, and always seeded with the system default
    /// (`resolved_rmw(None, None)`) so the local side links too.
    ///
    /// Each `[[bridge]]` names two endpoints (`from`/`to`). An endpoint is an
    /// `<rmw>:<domain>` session selector (e.g. `cyclone:default`) — the RMW is
    /// the prefix before `:`. A bare endpoint with no `:` is read as a
    /// `[[domain]]` name and resolved to that domain's `rmw`. Unresolvable bare
    /// endpoints are skipped (the `nros check` provenance pass flags them; issue
    /// 0076 §A).
    ///
    /// Empty `[[bridge]]` set ⇒ a single-element vec (just the default) ⇒ the
    /// build is byte-identical to a non-bridged single-RMW system.
    pub fn bridged_rmws(&self) -> Vec<String> {
        let mut set: Vec<String> = vec![self.resolved_rmw(None, None)];
        let domain_rmw = |name: &str| -> Option<String> {
            self.domains
                .iter()
                .find(|d| d.name == name)
                .map(|d| d.rmw.clone())
        };
        for bridge in &self.bridges {
            for endpoint in [&bridge.from, &bridge.to] {
                // `<rmw>:<domain>` → the RMW is the prefix; a bare name is a
                // `[[domain]]` reference resolved to its `rmw`.
                let rmw = match endpoint.split_once(':') {
                    Some((rmw, _domain)) => Some(rmw.to_string()),
                    None => domain_rmw(endpoint),
                };
                if let Some(rmw) = rmw
                    && !rmw.is_empty()
                    && !set.contains(&rmw)
                {
                    set.push(rmw);
                }
            }
        }
        set
    }
}

/// `[system]` table inside `<bringup>/system.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemHeader {
    pub name: String,
    pub rmw: String,
    pub domain_id: u32,
    /// Optional default locator. Per-deploy blocks can override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    /// Optional default launch filename — RELATIVE to `<bringup>/launch/`
    /// (per docs/system-toml-schema-v0.1.md §3.1, design-doc §11.3
    /// 2026-06-03). Names the launch file picked when neither CLI flag nor
    /// macro arg nor per-deploy `launch` override selects one. When absent,
    /// the resolver falls back to the literal `"system.launch.xml"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_launch: Option<String>,
    /// Optional default `[deploy.<target>]` block key — picked by
    /// Entry codegen when the user does not pass `--target`. When absent,
    /// the launcher falls back to `"native"` if that block exists, else
    /// the first deploy entry in declaration / sorted order. Phase 212.J.2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_target: Option<String>,
    /// Phase 261 W4 — generic capability axes by declared name, e.g.
    /// `features = ["safety", "param_services"]`. Each entry must resolve via
    /// `capability_resolver::capability(name)` (unknown ⇒ hard error, typo guard);
    /// it lowers identically to the typed `[<name>] enabled = true` block on every
    /// language. The thin user surface that replaces the per-axis typed blocks (now
    /// deprecated). Absent ⇒ empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
}

/// `[[component]]` row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemComponentEntry {
    pub pkg: String,
    pub class: String,
    pub name: String,
}

/// `[deploy.<target>]` block.
///
/// Per the F.4 §12 known-gap #2 resolution (path a — relax parser): both
/// `kind` and `target` are OPTIONAL. The deploy block is configuration-
/// by-target — the `<target>` map key (e.g. `native`, `qemu-mps2-an385`,
/// `threadx-linux`, `platformio`) already names the runner, and the
/// runner stage derives sensible defaults for `kind` / `target` from
/// the target name when these fields are absent. Strict
/// `deny_unknown_fields` is preserved — widening the schema, not
/// loosening the policy.
///
/// `nros check` is the place to surface a heads-up when `kind`/`target`
/// is absent AND the runner can't synthesise defaults from the target
/// name; that's a lint, not a parser error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeployTarget {
    /// `"self"`, `"qemu"`, `"flash"`, … — interpreted by the runner stage.
    /// Optional (F.4 §12 gap #2): absent ⇒ runner derives from the
    /// `<target>` map key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Target triple / board id / runner key. The exact semantics depend
    /// on `kind`. Optional (F.4 §12 gap #2): absent ⇒ runner derives
    /// from the `<target>` map key + the chosen `kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Optional path (relative to the bringup pkg root) to a
    /// `launch/*.launch.xml` used for this deploy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<String>,
    /// Optional board identifier (e.g. `mps2_an385`, `qemu_riscv64`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    /// Optional framework identifier for runners that surface one — today
    /// PlatformIO (`"espidf"`, `"arduino"`, …) and indirectly ESP-IDF. Held
    /// verbatim and forwarded to the runner stage; no schema-level
    /// validation. Resolves F.4 §12 known gap #3 (the platformio fixture
    /// authored `framework = "espidf"` against a `DeployTarget` that
    /// didn't know the field).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    /// Phase 255 — per-deploy RMW override of `[system].rmw` (RFC-0031
    /// precedence: `--rmw` > `[deploy.<t>].rmw` > `[system].rmw` > `zenoh`).
    /// Resolved via [`SystemToml::resolved_rmw`]; both codegen paths read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rmw: Option<String>,
    /// Phase 256 Wave 3 — per-target build tuning, the typed home superseding the
    /// deprecated `nros.toml` `[build]` overlay. Build tuning is per-deploy (size
    /// on embedded, debug on native), so it lives in the deploy block. The
    /// planner resolves these for the selected target (`resolve_target`) into the
    /// plan's `PlanBuildOptions`. (The `[build.cargo]` / `[build.cc]` per-layer
    /// tables + compile `cfg` are a follow-up — Eq-clean scalars first.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Coherent size/speed intent (`size`|`speed`|`balanced`|`debug`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimize: Option<String>,
    /// Extra cargo features for this target's generated build.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// Phase 256 Wave 8 — per-deploy override of `[system].domain_id` (RFC-0004
    /// §3.1 ladder, like `rmw`). Resolved via [`SystemToml::resolved_domain_id`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<u32>,
    /// Phase 256 Wave 8 — per-deploy override of `[system].locator`. Resolved via
    /// [`SystemToml::resolved_locator`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
}

/// `[[domain]]` row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemDomainEntry {
    pub name: String,
    pub rmw: String,
    pub id: u32,
}

/// `[[bridge]]` row.
///
/// phase-267 W1c — a bridge forwards declared `topics` (by name; `nros sync`
/// resolves each to its `type_name` + `type_hash` from the publishing
/// component's metadata, so the user never writes the opaque RIHS hash). Empty
/// `topics` ⇒ forward every declared topic (resolve-from-interfaces). Direction
/// is `from`→`to`; `bidirectional` adds the reverse relay (echo-suppressed).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemBridgeEntry {
    pub name: String,
    pub from: String,
    pub to: String,
    /// Forwarded topic NAMES (e.g. `["/chatter"]`). Empty ⇒ forward all declared
    /// topics. Types/hashes are resolved by `nros sync`, not written here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<String>,
    /// `true` ⇒ also relay `to`→`from` (echo-suppressed). Default one-way.
    #[serde(default, skip_serializing_if = "is_false")]
    pub bidirectional: bool,
}

/// serde `skip_serializing_if` helper — omit a `false` flag so a one-way bridge
/// round-trips byte-identically to a pre-W1c `[[bridge]]`.
fn is_false(b: &bool) -> bool {
    !*b
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// phase-267 W1c/C3a — a node declares its published topics+types in
    /// `[package.metadata.nros.node]`; the planner reads them pre-build to
    /// resolve a bridge's topic names.
    #[test]
    fn node_metadata_parses_publishes_and_subscribes() {
        let raw = r#"
[node]
class = "talker_pkg::Talker"
name = "talker"
[[node.publishes]]
topic = "/chatter"
type = "std_msgs/Int32"
[[node.subscribes]]
topic = "/cmd"
type = "std_msgs/Bool"
"#;
        let m: PackageMetadataNros = toml::from_str(raw).expect("parse node metadata");
        let node = m.node.expect("node table");
        assert_eq!(node.publishes.len(), 1);
        assert_eq!(node.publishes[0].topic, "/chatter");
        assert_eq!(node.publishes[0].type_name, "std_msgs/Int32");
        assert_eq!(node.subscribes[0].topic, "/cmd");
        assert_eq!(node.subscribes[0].type_name, "std_msgs/Bool");

        // Back-compat: a node WITHOUT publishes/subscribes still parses.
        let plain: PackageMetadataNros =
            toml::from_str("[node]\nclass=\"p::T\"\nname=\"t\"\n").expect("plain node");
        assert!(plain.node.unwrap().publishes.is_empty());
    }

    /// Round-trip a `[workspace.metadata.nros]` golden through parse +
    /// serialize + reparse and compare structs.
    #[test]
    fn workspace_metadata_round_trip() {
        let raw = r#"
default_system = "demo_bringup"
rmw_override = "cyclonedds"
"#;
        let v1: WorkspaceMetadataNros = toml::from_str(raw).expect("parse golden");
        assert_eq!(v1.default_system.as_deref(), Some("demo_bringup"));
        assert_eq!(v1.rmw_override.as_deref(), Some("cyclonedds"));

        let reserialized = toml::to_string(&v1).expect("serialize");
        let v2: WorkspaceMetadataNros = toml::from_str(&reserialized).expect("reparse");
        assert_eq!(v1, v2);
    }

    /// Minimal workspace-metadata table (only `default_system`) parses; the
    /// optional `rmw_override` defaults to `None`.
    #[test]
    fn workspace_metadata_minimal_parses() {
        let raw = r#"default_system = "demo_bringup""#;
        let v: WorkspaceMetadataNros = toml::from_str(raw).expect("parse");
        assert_eq!(v.default_system.as_deref(), Some("demo_bringup"));
        assert!(v.rmw_override.is_none());
    }

    /// Empty workspace-metadata is also valid (workspace may declare the
    /// table without populating it yet).
    #[test]
    fn workspace_metadata_empty_parses() {
        let v: WorkspaceMetadataNros = toml::from_str("").expect("parse empty");
        assert_eq!(v, WorkspaceMetadataNros::default());
    }

    /// `[package.metadata.nros.component]` single-shape round-trip.
    #[test]
    fn package_metadata_single_component_round_trip() {
        let raw = r#"
[component]
default_namespace = "/demo"

[component.parameters]
rate_hz = 10
greeting = "hello"

[[component.remaps]]
from = "chatter"
to = "topic/chatter"
"#;
        let v1: PackageMetadataNros = toml::from_str(raw).expect("parse");
        v1.validate().expect("single-shape is valid");
        let component = v1.component.as_ref().expect("component present");
        assert_eq!(component.default_namespace.as_deref(), Some("/demo"));
        assert_eq!(component.parameters.len(), 2);
        assert_eq!(component.remaps.len(), 1);
        assert_eq!(component.remaps[0].from, "chatter");
        assert_eq!(component.remaps[0].to, "topic/chatter");
        assert!(v1.components.is_empty());

        let reserialized = toml::to_string(&v1).expect("serialize");
        let v2: PackageMetadataNros = toml::from_str(&reserialized).expect("reparse");
        assert_eq!(v1, v2);
    }

    /// `[package.metadata.nros.components.<Name>]` multi-shape round-trip.
    #[test]
    fn package_metadata_multi_component_round_trip() {
        let raw = r#"
[components.Talker]
default_namespace = "/demo"

[components.Talker.parameters]
rate_hz = 10

[components.Listener]
default_namespace = "/demo"
"#;
        let v1: PackageMetadataNros = toml::from_str(raw).expect("parse");
        v1.validate().expect("multi-shape is valid");
        assert!(v1.component.is_none());
        assert_eq!(v1.components.len(), 2);
        // BTreeMap ⇒ keys are sorted.
        let names: Vec<&str> = v1.components.keys().map(String::as_str).collect();
        assert_eq!(names, ["Listener", "Talker"]);

        let reserialized = toml::to_string(&v1).expect("serialize");
        let v2: PackageMetadataNros = toml::from_str(&reserialized).expect("reparse");
        assert_eq!(v1, v2);
    }

    /// Declaring both `component` and `components` is a hard error (loader
    /// must call `validate`).
    #[test]
    fn package_metadata_rejects_both_shapes() {
        let raw = r#"
[component]
default_namespace = "/a"

[components.Other]
default_namespace = "/b"
"#;
        let v: PackageMetadataNros = toml::from_str(raw).expect("parse");
        let err = v.validate().expect_err("conflicting shapes must error");
        assert!(
            err.contains("component"),
            "diagnostic mentions field: {err}"
        );
    }

    /// `deny_unknown_fields` rejects typos on the component table.
    #[test]
    fn rejects_unknown_field_in_strict_mode() {
        let raw = r#"
[component]
default_namespace = "/demo"
unknown_typo = true
"#;
        let err =
            toml::from_str::<PackageMetadataNros>(raw).expect_err("unknown field must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown_typo") || msg.contains("unknown field"),
            "diagnostic should name the typo: {msg}"
        );
    }

    /// Same strictness for the workspace table.
    #[test]
    fn rejects_unknown_field_on_workspace_metadata() {
        let raw = r#"
default_system = "demo_bringup"
not_a_field = 42
"#;
        let err = toml::from_str::<WorkspaceMetadataNros>(raw)
            .expect_err("unknown field must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("not_a_field") || msg.contains("unknown field"),
            "diagnostic: {msg}"
        );
    }

    /// Phase 212.B.4 — extended `[package.metadata.ament]` carries
    /// `description` / `maintainer = { name, email }` / `license` /
    /// `buildtool_depend` alongside the dependency lists.
    #[test]
    fn parses_ament_metadata_basic() {
        let raw = r#"
description = "A talker that publishes std_msgs/String at 10 Hz."
license = "Apache-2.0"
maintainer = { name = "Ada Lovelace", email = "ada@example.com" }
buildtool_depend = ["ament_cargo"]
exec_depend = ["std_msgs", "rcl_interfaces"]
build_depend = ["std_msgs"]
test_depend = []
"#;
        let v: PackageMetadataAment = toml::from_str(raw).expect("parse");
        assert_eq!(
            v.description.as_deref(),
            Some("A talker that publishes std_msgs/String at 10 Hz.")
        );
        assert_eq!(v.license.as_deref(), Some("Apache-2.0"));
        let m = v.maintainer.as_ref().expect("maintainer present");
        assert_eq!(m.name, "Ada Lovelace");
        assert_eq!(m.email, "ada@example.com");
        assert_eq!(v.buildtool_depend, vec!["ament_cargo"]);
        assert_eq!(v.exec_depend, vec!["std_msgs", "rcl_interfaces"]);
        assert_eq!(v.build_depend, vec!["std_msgs"]);
        assert!(v.test_depend.is_empty());

        // Round-trip cleanly.
        let reser = toml::to_string(&v).expect("ser");
        let v2: PackageMetadataAment = toml::from_str(&reser).expect("reparse");
        assert_eq!(v, v2);
    }

    /// `deny_unknown_fields` rejects typos on the extended ament table.
    #[test]
    fn rejects_unknown_field_in_ament_metadata() {
        let raw = r#"
description = "x"
license = "Apache-2.0"
not_a_field = true
"#;
        let err = toml::from_str::<PackageMetadataAment>(raw)
            .expect_err("unknown field must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("not_a_field") || msg.contains("unknown field"),
            "diagnostic: {msg}"
        );
    }

    /// `maintainer = { name, email }` is strict: a stray
    /// `affiliation = …` field fails at parse time.
    #[test]
    fn rejects_unknown_field_in_ament_maintainer() {
        let raw = r#"
maintainer = { name = "Ada", email = "a@b.c", affiliation = "ACME" }
"#;
        let err = toml::from_str::<PackageMetadataAment>(raw)
            .expect_err("unknown field on maintainer must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("affiliation") || msg.contains("unknown field"),
            "diagnostic: {msg}"
        );
    }

    /// Phase 212.L.7 — `[workspace.metadata.nros] default_system = "..."`
    /// + `rmw_override` + `domain_id_override` round-trips.
    #[test]
    fn loads_workspace_metadata_default_system() {
        let raw = r#"
default_system = "demo_bringup"
rmw_override = "cyclonedds"
domain_id_override = 7
"#;
        let v: WorkspaceMetadataNros = toml::from_str(raw).expect("parse");
        assert_eq!(v.default_system.as_deref(), Some("demo_bringup"));
        assert_eq!(v.rmw_override.as_deref(), Some("cyclonedds"));
        assert_eq!(v.domain_id_override, Some(7));

        let reser = toml::to_string(&v).expect("ser");
        let v2: WorkspaceMetadataNros = toml::from_str(&reser).expect("reparse");
        assert_eq!(v, v2);
    }

    /// `[package.metadata.ament]` round-trip (legacy fields only).
    #[test]
    fn ament_metadata_round_trip() {
        let raw = r#"
build_depend = ["rosidl_default_generators"]
exec_depend = ["rosidl_default_runtime", "std_msgs"]
test_depend = ["ament_lint_auto"]
build_type = "ament_cargo"
"#;
        let v1: PackageMetadataAment = toml::from_str(raw).expect("parse");
        assert_eq!(v1.build_depend, vec!["rosidl_default_generators"]);
        assert_eq!(v1.exec_depend, vec!["rosidl_default_runtime", "std_msgs"]);
        assert_eq!(v1.test_depend, vec!["ament_lint_auto"]);
        assert_eq!(v1.build_type.as_deref(), Some("ament_cargo"));

        let reserialized = toml::to_string(&v1).expect("serialize");
        let v2: PackageMetadataAment = toml::from_str(&reserialized).expect("reparse");
        assert_eq!(v1, v2);
    }

    /// Minimal `[package.metadata.ament]` (only `exec_depend`) parses.
    #[test]
    fn ament_metadata_minimal_parses() {
        let raw = r#"exec_depend = ["std_msgs"]"#;
        let v: PackageMetadataAment = toml::from_str(raw).expect("parse");
        assert_eq!(v.exec_depend, vec!["std_msgs"]);
        assert!(v.build_depend.is_empty());
        assert!(v.test_depend.is_empty());
        assert!(v.build_type.is_none());
    }

    /// Full `<bringup>/system.toml` golden round-trip.
    #[test]
    fn system_toml_round_trip() {
        let raw = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0
locator = "tcp/127.0.0.1:7447"

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::TalkerNode"
name = "talker"

[[component]]
pkg = "listener_pkg"
class = "listener_pkg::ListenerNode"
name = "listener"

[deploy.native]
kind = "self"
target = "x86_64-unknown-linux-gnu"
launch = "launch/system.launch.xml"

[deploy.qemu-mps2-an385]
kind = "qemu"
target = "thumbv7m-none-eabi"
board = "mps2_an385"

[[domain]]
name = "default"
rmw = "zenoh"
id = 0

[[bridge]]
name = "cyclone_to_zenoh"
from = "cyclone:default"
to = "zenoh:default"
"#;
        let v1: SystemToml = toml::from_str(raw).expect("parse system.toml");
        assert_eq!(v1.system.name, "demo");
        assert_eq!(v1.system.rmw, "zenoh");
        assert_eq!(v1.system.domain_id, 0);
        assert_eq!(v1.system.locator.as_deref(), Some("tcp/127.0.0.1:7447"));
        assert_eq!(v1.components.len(), 2);
        assert_eq!(v1.components[0].name, "talker");
        assert_eq!(v1.components[1].name, "listener");
        assert_eq!(v1.deploy.len(), 2);
        let native = v1.deploy.get("native").expect("native deploy present");
        assert_eq!(native.kind.as_deref(), Some("self"));
        assert_eq!(native.launch.as_deref(), Some("launch/system.launch.xml"));
        let qemu = v1
            .deploy
            .get("qemu-mps2-an385")
            .expect("qemu deploy present");
        assert_eq!(qemu.board.as_deref(), Some("mps2_an385"));
        assert_eq!(v1.domains.len(), 1);
        assert_eq!(v1.bridges.len(), 1);

        let reserialized = toml::to_string(&v1).expect("serialize");
        let v2: SystemToml = toml::from_str(&reserialized).expect("reparse");
        assert_eq!(v1, v2);
    }

    /// Phase 255 Wave 5 — `bridged_rmws` returns the union of the system default
    /// plus every cross-RMW `[[bridge]]` endpoint's RMW (the `<rmw>:<domain>`
    /// prefix, or a bare `[[domain]]` name resolved to its rmw). No bridges ⇒
    /// just the default (single-RMW, byte-identical build).
    #[test]
    fn bridged_rmws_unions_bridge_endpoints() {
        // `<rmw>:<domain>` selectors — the prefix is the RMW.
        let prefixed: SystemToml = toml::from_str(
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n\
             [[domain]]\nname=\"default\"\nrmw=\"zenoh\"\nid=0\n\
             [[bridge]]\nname=\"b\"\nfrom=\"cyclonedds:default\"\nto=\"zenoh:default\"\n",
        )
        .unwrap();
        assert_eq!(prefixed.bridged_rmws(), vec!["zenoh", "cyclonedds"]);

        // Bare endpoint names resolve through `[[domain]]`.
        let by_domain: SystemToml = toml::from_str(
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n\
             [[domain]]\nname=\"cloud\"\nrmw=\"cyclonedds\"\nid=1\n\
             [[bridge]]\nname=\"b\"\nfrom=\"cloud\"\nto=\"local\"\n",
        )
        .unwrap();
        // `cloud` → cyclonedds; `local` is undeclared → skipped.
        assert_eq!(by_domain.bridged_rmws(), vec!["zenoh", "cyclonedds"]);

        // No bridges ⇒ just the system default.
        let plain: SystemToml =
            toml::from_str("[system]\nname=\"d\"\nrmw=\"xrce\"\ndomain_id=0\n").unwrap();
        assert_eq!(plain.bridged_rmws(), vec!["xrce"]);
    }

    /// Phase 254 — `[safety]` / `[param_services]` capability axes parse as typed
    /// `system.toml` tables (the single home both codegen paths read), with
    /// defaults + round-trip. Absent ⇒ `None` (byte-identical to pre-254).
    #[test]
    fn parses_system_toml_capability_axes() {
        let raw = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[safety]
crc = false

[param_services]
"#;
        let v: SystemToml = toml::from_str(raw).expect("parse system.toml with capabilities");
        let safety = v.safety.as_ref().expect("[safety] present");
        assert!(safety.enabled, "enabled defaults true");
        assert!(!safety.crc, "crc = false round-trips");
        let ps = v.param_services.as_ref().expect("[param_services] present");
        assert!(ps.enabled, "enabled defaults true");

        // Round-trip.
        let v2: SystemToml =
            toml::from_str(&toml::to_string(&v).expect("serialize")).expect("reparse");
        assert_eq!(v, v2);

        // Absent → None.
        let bare: SystemToml =
            toml::from_str("[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n").expect("bare");
        assert!(bare.safety.is_none() && bare.param_services.is_none());

        // enabled = false opts out.
        let off: SystemToml = toml::from_str(
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n[safety]\nenabled=false\n",
        )
        .expect("parse");
        assert!(!off.safety.as_ref().unwrap().enabled);
    }

    /// Phase 255 — `resolved_rmw` applies the RFC-0031 precedence:
    /// `--rmw` > `[deploy.<t>].rmw` > `[system].rmw` > `zenoh`.
    #[test]
    fn resolved_rmw_precedence_ladder() {
        let sys: SystemToml = toml::from_str(
            r#"
[system]
name = "d"
rmw = "zenoh"
domain_id = 0

[deploy.native]
rmw = "cyclonedds"

[deploy.qemu]
kind = "qemu"
"#,
        )
        .unwrap();

        // CLI flag wins over everything.
        assert_eq!(sys.resolved_rmw(Some("native"), Some("xrce")), "xrce");
        // [deploy.<t>].rmw overrides [system].rmw.
        assert_eq!(sys.resolved_rmw(Some("native"), None), "cyclonedds");
        // deploy block without rmw → falls to [system].rmw.
        assert_eq!(sys.resolved_rmw(Some("qemu"), None), "zenoh");
        // unknown / no target → [system].rmw.
        assert_eq!(sys.resolved_rmw(Some("nope"), None), "zenoh");
        assert_eq!(sys.resolved_rmw(None, None), "zenoh");

        // Empty [system].rmw → the built-in "zenoh" default.
        let bare: SystemToml =
            toml::from_str("[system]\nname=\"d\"\nrmw=\"\"\ndomain_id=0\n").unwrap();
        assert_eq!(bare.resolved_rmw(None, None), "zenoh");
    }

    /// Phase 256 — `resolve_target`: `--target` → `[system].default_target` →
    /// the sole `[deploy.<t>]` → `None`.
    #[test]
    fn resolve_target_precedence() {
        // CLI flag wins.
        let two: SystemToml = toml::from_str(
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\ndefault_target=\"native\"\n\
             [deploy.native]\nkind=\"self\"\n[deploy.qemu]\nkind=\"qemu\"\n",
        )
        .unwrap();
        assert_eq!(two.resolve_target(Some("qemu")).as_deref(), Some("qemu"));
        // No flag → default_target.
        assert_eq!(two.resolve_target(None).as_deref(), Some("native"));

        // No default_target, two deploys → ambiguous → None.
        let ambiguous: SystemToml = toml::from_str(
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n\
             [deploy.a]\nkind=\"self\"\n[deploy.b]\nkind=\"self\"\n",
        )
        .unwrap();
        assert_eq!(ambiguous.resolve_target(None), None);

        // No default_target, sole deploy → that one.
        let sole: SystemToml = toml::from_str(
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n[deploy.only]\nkind=\"self\"\n",
        )
        .unwrap();
        assert_eq!(sole.resolve_target(None).as_deref(), Some("only"));
    }

    /// Phase 256 Wave 8 — `[deploy.<t>].domain_id`/`.locator` override the
    /// `[system]` defaults for that target; absent → the system value.
    #[test]
    fn resolved_domain_and_locator_honour_deploy_override() {
        let sys: SystemToml = toml::from_str(
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\nlocator=\"tcp/sys:7447\"\n\
             [deploy.robot]\nkind=\"flash\"\ndomain_id=7\nlocator=\"tcp/robot:7450\"\n\
             [deploy.native]\nkind=\"self\"\n",
        )
        .unwrap();

        // robot overrides both.
        assert_eq!(sys.resolved_domain_id(Some("robot")), 7);
        assert_eq!(
            sys.resolved_locator(Some("robot")).as_deref(),
            Some("tcp/robot:7450")
        );
        // native overrides neither → [system] defaults.
        assert_eq!(sys.resolved_domain_id(Some("native")), 0);
        assert_eq!(
            sys.resolved_locator(Some("native")).as_deref(),
            Some("tcp/sys:7447")
        );
        // no target → [system] defaults.
        assert_eq!(sys.resolved_domain_id(None), 0);
    }

    /// Minimal `<bringup>/system.toml` — only `[system]` + one
    /// `[[component]]`, optional sections absent.
    #[test]
    fn system_toml_minimal_parses() {
        let raw = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::TalkerNode"
name = "talker"
"#;
        let v: SystemToml = toml::from_str(raw).expect("parse minimal");
        assert_eq!(v.system.name, "demo");
        assert!(v.system.locator.is_none());
        assert_eq!(v.components.len(), 1);
        assert!(v.deploy.is_empty());
        assert!(v.domains.is_empty());
        assert!(v.bridges.is_empty());
        // Phase 228.A — tier surface defaults empty (backward compat).
        assert!(v.tiers.is_empty());
        assert!(v.node_overrides.is_empty());
    }

    #[test]
    fn system_toml_parses_tiers_and_overrides() {
        // Phase 228.A (RFC-0015 §4.2) — tier + override schema.
        let raw = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "ctrl_pkg"
class = "ctrl_pkg::Control"
name = "control_node"

[tiers.high]
spin_period_us = 1000
[tiers.high.freertos]
priority = 5
stack_bytes = 8192
[tiers.high.zephyr]
priority = -1
[tiers.high.posix]
priority = 80
sched_class = "SCHED_FIFO"

[tiers.low.freertos]
priority = 1

[[node_overrides]]
name = "control_node"
callback_groups = [{ id = "telemetry", tier = "low" }]
"#;
        let v: SystemToml = toml::from_str(raw).expect("parse tier schema");

        let high = v.tiers.get("high").expect("high tier");
        assert_eq!(high.spin_period_us, Some(1000));
        assert_eq!(high.freertos.as_ref().unwrap().priority, 5);
        assert_eq!(high.freertos.as_ref().unwrap().stack_bytes, Some(8192));
        assert_eq!(high.zephyr.as_ref().unwrap().priority, -1);
        assert_eq!(
            high.posix.as_ref().unwrap().sched_class.as_deref(),
            Some("SCHED_FIFO")
        );
        assert_eq!(
            v.tiers
                .get("low")
                .unwrap()
                .freertos
                .as_ref()
                .unwrap()
                .priority,
            1
        );

        let ov = &v.node_overrides[0];
        assert_eq!(ov.name, "control_node");
        assert_eq!(ov.callback_groups[0].tier, "low");

        // Round-trips through serde.
        let s = toml::to_string(&v).expect("serialize");
        let v2: SystemToml = toml::from_str(&s).expect("reparse");
        assert_eq!(v, v2);
    }

    // -----------------------------------------------------------------
    // Phase 212.L — class / name / application / deploy schema additions
    // -----------------------------------------------------------------

    /// `[package.metadata.nros.component]` round-trip carries the new
    /// `class` + `name` fields.
    #[test]
    fn loads_component_with_class_and_name() {
        let raw = r#"
[component]
class = "alpha_pkg::Node"
name = "alpha"
default_namespace = "/demo"

[component.parameters]
rate_hz = 5
"#;
        let v: PackageMetadataNros = toml::from_str(raw).expect("parse");
        v.validate().expect("single-shape ok");
        let c = v.component.as_ref().expect("component present");
        assert_eq!(c.class.as_deref(), Some("alpha_pkg::Node"));
        assert_eq!(c.name.as_deref(), Some("alpha"));
        assert_eq!(c.default_namespace.as_deref(), Some("/demo"));
        assert_eq!(c.parameters.len(), 1);

        // Round-trip.
        let reser = toml::to_string(&v).expect("ser");
        let v2: PackageMetadataNros = toml::from_str(&reser).expect("reparse");
        assert_eq!(v, v2);
    }

    /// `[package.metadata.nros.application]` accepts a `deploy = […]`
    /// allow-list and an optional `name`.
    #[test]
    fn loads_application_with_deploy_targets() {
        let raw = r#"
[application]
name = "demo_app"
deploy = ["native", "qemu-arm-baremetal"]
"#;
        let v: PackageMetadataNros = toml::from_str(raw).expect("parse");
        v.validate().expect("application-shape ok");
        let app = v.application.as_ref().expect("application present");
        assert_eq!(app.name.as_deref(), Some("demo_app"));
        assert_eq!(app.deploy, vec!["native", "qemu-arm-baremetal"]);
        assert!(v.component.is_none());
        assert!(v.components.is_empty());

        let reser = toml::to_string(&v).expect("ser");
        let v2: PackageMetadataNros = toml::from_str(&reser).expect("reparse");
        assert_eq!(v, v2);
    }

    /// `[package.metadata.nros.deploy.<target>]` populates the typed
    /// per-target table.
    #[test]
    fn loads_deploy_target_metadata() {
        let raw = r#"
[component]
class = "alpha_pkg::Node"
name = "alpha"

[deploy.native]
board = "native_sim/native/64"
rmw = "zenoh"
domain_id = 7
locator = "tcp/127.0.0.1:7447"

[deploy.qemu-mps2-an385]
board = "mps2-an385"
rmw = "cyclonedds"
"#;
        let v: PackageMetadataNros = toml::from_str(raw).expect("parse");
        v.validate().expect("valid");
        assert!(v.is_self_bringup_eligible());
        assert_eq!(v.deploy.len(), 2);
        let native = v.deploy.get("native").expect("native present");
        assert_eq!(native.board.as_deref(), Some("native_sim/native/64"));
        assert_eq!(native.rmw.as_deref(), Some("zenoh"));
        assert_eq!(native.domain_id, Some(7));
        assert_eq!(native.locator.as_deref(), Some("tcp/127.0.0.1:7447"));
        let qemu = v.deploy.get("qemu-mps2-an385").expect("qemu present");
        assert_eq!(qemu.board.as_deref(), Some("mps2-an385"));
        assert_eq!(qemu.rmw.as_deref(), Some("cyclonedds"));
        assert!(qemu.domain_id.is_none());
        assert!(qemu.locator.is_none());

        let reser = toml::to_string(&v).expect("ser");
        let v2: PackageMetadataNros = toml::from_str(&reser).expect("reparse");
        assert_eq!(v, v2);
    }

    /// `deny_unknown_fields` rejects typos on `[component]` w/ the new
    /// `class` + `name` siblings.
    #[test]
    fn rejects_unknown_field_in_component() {
        let raw = r#"
[component]
class = "alpha_pkg::Node"
name = "alpha"
bogus = true
"#;
        let err =
            toml::from_str::<PackageMetadataNros>(raw).expect_err("unknown field must reject");
        let s = err.to_string();
        assert!(s.contains("bogus") || s.contains("unknown field"), "{s}");
    }

    /// `deny_unknown_fields` on `[application]`.
    #[test]
    fn rejects_unknown_field_in_application() {
        let raw = r#"
[application]
deploy = ["native"]
oops = 1
"#;
        let err = toml::from_str::<PackageMetadataNros>(raw)
            .expect_err("unknown field on application must reject");
        let s = err.to_string();
        assert!(s.contains("oops") || s.contains("unknown field"), "{s}");
    }

    /// `deny_unknown_fields` on `[deploy.<target>]`.
    #[test]
    fn rejects_unknown_field_in_deploy_target() {
        let raw = r#"
[component]
class = "alpha_pkg::Node"
name = "alpha"

[deploy.native]
board = "native_sim"
mystery = "no"
"#;
        let err = toml::from_str::<PackageMetadataNros>(raw)
            .expect_err("unknown field on deploy.<target> must reject");
        let s = err.to_string();
        assert!(s.contains("mystery") || s.contains("unknown field"), "{s}");
    }

    /// Component + application in the same pkg is rejected (mutex).
    #[test]
    fn rejects_component_and_application_in_same_pkg() {
        let raw = r#"
[component]
class = "alpha_pkg::Node"
name = "alpha"

[application]
deploy = ["native"]
"#;
        let v: PackageMetadataNros = toml::from_str(raw).expect("parse");
        let err = v.validate().expect_err("mutex must trip");
        assert!(
            err.contains("application") || err.contains("component"),
            "{err}"
        );
    }

    /// `[system].default_launch` is accepted by `SystemHeader` per
    /// docs/system-toml-schema-v0.1.md §3.1 (resolves the 2026-06-03 §11.3
    /// design lock — F.4 §12 known gap #1).
    #[test]
    fn parses_system_toml_with_default_launch() {
        let raw = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0
default_launch = "talker_only.launch.xml"

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::TalkerNode"
name = "talker"
"#;
        let v: SystemToml = toml::from_str(raw).expect("parse with default_launch");
        assert_eq!(
            v.system.default_launch.as_deref(),
            Some("talker_only.launch.xml")
        );
        // Absence keeps the field None — resolver supplies the literal
        // "system.launch.xml" fallback.
        let raw_minimal = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0
"#;
        let v_min: SystemToml = toml::from_str(raw_minimal).expect("parse minimal");
        assert!(v_min.system.default_launch.is_none());
    }

    /// `[deploy.<target>]` accepts a block with neither `kind` nor `target`
    /// — both fields are optional per F.4 §12 known-gap #2 path (a). The
    /// in-tree `multi_pkg_workspace_threadx` / `multi_pkg_workspace_platformio`
    /// fixtures carry such blocks; the runner derives sensible defaults
    /// from the `<target>` map key.
    #[test]
    fn accepts_deploy_target_without_kind() {
        // Mirrors the `multi_pkg_workspace_threadx` fixture shape.
        let raw = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::Talker"
name = "talker"

[deploy.threadx-linux]
launch = "launch/system.launch.xml"
"#;
        let v: SystemToml = toml::from_str(raw)
            .expect("deploy block without kind/target must parse (F.4 §12 gap #2)");
        let dt = v
            .deploy
            .get("threadx-linux")
            .expect("threadx-linux deploy present");
        assert!(dt.kind.is_none(), "kind absent when omitted");
        assert!(dt.target.is_none(), "target absent when omitted");
        assert_eq!(dt.launch.as_deref(), Some("launch/system.launch.xml"));
        assert!(dt.board.is_none());
    }

    /// `[deploy.<target>].framework` is accepted (F.4 §12 known gap #3).
    /// PlatformIO carries `framework = "espidf"` / `"arduino"` / … on its
    /// deploy block; the field passes through verbatim for the runner.
    /// Mirrors the `multi_pkg_workspace_platformio` fixture.
    #[test]
    fn accepts_platformio_framework_field() {
        let raw = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::talker"
name = "talker"

[deploy.platformio]
launch = "launch/system.launch.xml"
framework = "espidf"
board = "esp32dev"
"#;
        let v: SystemToml =
            toml::from_str(raw).expect("framework field must parse (F.4 §12 gap #3)");
        let dt = v
            .deploy
            .get("platformio")
            .expect("platformio deploy present");
        assert_eq!(dt.framework.as_deref(), Some("espidf"));
        assert_eq!(dt.board.as_deref(), Some("esp32dev"));
        assert_eq!(dt.launch.as_deref(), Some("launch/system.launch.xml"));
        // Round-trip: serialized form keeps the field.
        let reser = toml::to_string(&v).expect("ser");
        let v2: SystemToml = toml::from_str(&reser).expect("reparse");
        assert_eq!(v, v2);
    }

    /// Phase 212.M-F.17 — synthesis subset round-trip. The α-bridge in
    /// `workspace.rs::synthetic_metadata_artifacts` reads `class` /
    /// `name` / `default_namespace` out of `[component]` and the
    /// `[components.<Name>]` table-of-tables, then mints fresh JSON for
    /// the planner. Lock in the shape of those reads here so a future
    /// schema tweak that drops one of them surfaces at the cargo
    /// metadata schema boundary (closest to the user-facing TOML)
    /// instead of as a planner-level mystery.
    #[test]
    fn synthesis_subset_round_trip_single_and_multi() {
        // Single-shape `[component]` carrying every M-F.17 field.
        let raw_single = r#"
[component]
class = "talker_pkg::Talker"
name = "talker"
default_namespace = "/demo"
"#;
        let v: PackageMetadataNros = toml::from_str(raw_single).expect("parse single");
        v.validate().expect("single-shape valid");
        let c = v.component.as_ref().expect("component present");
        assert_eq!(c.class.as_deref(), Some("talker_pkg::Talker"));
        assert_eq!(c.name.as_deref(), Some("talker"));
        assert_eq!(c.default_namespace.as_deref(), Some("/demo"));

        // Multi-shape `[components.<Name>]` — same subset on a per-entry
        // basis. Bridge uses `<Name>` as the component-name fallback
        // when `metadata.name` is absent on the entry.
        let raw_multi = r#"
[components.Talker]
class = "talker_pkg::Talker"
default_namespace = "/demo"

[components.Listener]
class = "listener_pkg::Listener"
"#;
        let v: PackageMetadataNros = toml::from_str(raw_multi).expect("parse multi");
        v.validate().expect("multi-shape valid");
        assert_eq!(v.components.len(), 2);
        let talker = v.components.get("Talker").expect("Talker entry");
        assert_eq!(talker.class.as_deref(), Some("talker_pkg::Talker"));
        assert_eq!(talker.default_namespace.as_deref(), Some("/demo"));
        // Multi-shape entries inherit their component name from the
        // table key when `metadata.name` is absent — the bridge layer
        // checks both, but the schema records only what's authored.
        assert!(talker.name.is_none());

        // Round-trip the multi-shape so a schema edit that breaks
        // serialisation of one of the synth subset fields fails here.
        let reser = toml::to_string(&v).expect("ser");
        let v2: PackageMetadataNros = toml::from_str(&reser).expect("reparse");
        assert_eq!(v, v2);
    }

    /// `deny_unknown_fields` on `[system]` catches typos at the bringup
    /// surface.
    #[test]
    fn system_toml_rejects_unknown_field() {
        let raw = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0
mystery_knob = "no"
"#;
        let err = toml::from_str::<SystemToml>(raw)
            .expect_err("unknown field on [system] must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("mystery_knob") || msg.contains("unknown field"),
            "diagnostic: {msg}"
        );
    }
}
