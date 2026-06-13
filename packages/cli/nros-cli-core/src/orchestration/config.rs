use serde::{Deserialize, Serialize};

use super::{
    schema::{DeadlinePolicy, ParameterTable, RemapRule, SchedClass},
    source_metadata::ComponentLanguage,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentConfig {
    pub version: u32,
    pub package: String,
    pub component: String,
    pub language: ComponentLanguage,
    // W.3 (Phase 172): an absent `[linkage]` is legal — the fields are derived
    // from the package/component name + crate convention (see `ComponentLinkage`
    // `resolved_*`). A minimal component manifest declares only package +
    // component + language + metadata.
    #[serde(default)]
    pub linkage: ComponentLinkage,
    pub metadata: ComponentMetadataConfig,
    // W.3 (Phase 172): an absent `[overrides]` is legal — a minimal component
    // manifest declares only linkage + metadata. Defaults to empty overrides.
    #[serde(default)]
    pub overrides: ComponentOverrides,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentLinkage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crate_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_library: Option<String>,
}

impl ComponentLinkage {
    /// Crate name — explicit, else the ROS package name with `-`→`_`
    /// (`package.xml` ⇒ Cargo crate convention). W.3 derivation.
    pub fn resolved_crate_name(&self, package: &str) -> String {
        self.crate_name
            .clone()
            .unwrap_or_else(|| package.replace('-', "_"))
    }

    /// Executable / bin name — explicit, else the component's short name.
    pub fn resolved_executable(&self, component_name: &str) -> String {
        self.executable
            .clone()
            .unwrap_or_else(|| component_name.to_string())
    }

    /// Exported registration symbol — explicit, else the `nros_component_<name>`
    /// convention the codegen + `nros::component!` macro use.
    pub fn resolved_exported_symbol(&self, component_name: &str) -> String {
        self.exported_symbol
            .clone()
            .unwrap_or_else(|| format!("nros_component_{component_name}"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentMetadataConfig {
    pub source_metadata: String,
    pub generated_by: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_namespace: Option<String>,
    // W.3 (Phase 172): `parameters`/`remaps` default to empty so a minimal
    // `[overrides]` (or none at all) is legal — previously a manifest without
    // them failed with *"missing field `parameters`"*.
    #[serde(default, skip_serializing_if = "ParameterTable::is_empty")]
    pub parameters: ParameterTable,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remaps: Vec<RemapRule>,
}

// Phase 172 flip: the per-package `SystemConfig` tree (`TargetConfig`'s
// triple/board, `ManifestSource`, `SystemComponent`, `SystemOverlay`,
// `InstanceSelector`, `SchedulingSelector`, `SchedulingConfig`,
// `EndpointMapping`, `BuildConfig`) is retired. Deployment config now lives in
// the bringup `system.toml` (`[deploy.<name>]` carries kind/target/board;
// RFC-0004 §4). What remains here is the component manifest
// (`ComponentConfig`) + the scheduling tier (`SchedContextConfig`, consumed by
// the planner).

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchedContextConfig {
    pub id: String,
    pub executor: String,
    pub class: SchedClass,
    pub priority: Option<u8>,
    pub period_ms: Option<u64>,
    pub budget_ms: Option<u64>,
    pub deadline_ms: Option<u64>,
    pub deadline_policy: DeadlinePolicy,
    pub stack_size: Option<u32>,
    pub core: Option<u32>,
    pub task: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W.3 (Phase 172): a minimal component manifest declares only linkage +
    /// metadata — no `[overrides]` table at all — and parses with empty
    /// overrides instead of failing *"missing field `parameters`"*.
    #[test]
    fn minimal_component_manifest_without_overrides_parses() {
        let raw = r#"
            version = 1
            package = "demo_nodes_rs"
            component = "talker"
            language = "rust"

            [linkage]
            crate_name = "demo_nodes_rs"
            executable = "talker"

            [metadata]
            source_metadata = "target/nros/metadata/talker.json"
        "#;
        let cfg: ComponentConfig = toml::from_str(raw).expect("minimal manifest parses");
        assert_eq!(cfg.overrides, ComponentOverrides::default());
        assert!(cfg.overrides.parameters.is_empty());
        assert!(cfg.overrides.remaps.is_empty());
        assert!(cfg.overrides.default_namespace.is_none());
    }

    /// W.3 (Phase 172): an absent `[linkage]` is legal too — the metadata
    /// driver derives executable / symbol from the component name.
    #[test]
    fn minimal_component_manifest_without_linkage_parses() {
        let raw = r#"
            version = 1
            package = "demo_nodes_rs"
            component = "demo_nodes_rs::talker"
            language = "rust"
            [metadata]
            source_metadata = "target/nros/metadata/talker.json"
        "#;
        let cfg: ComponentConfig = toml::from_str(raw).expect("no-linkage manifest parses");
        assert_eq!(cfg.linkage, ComponentLinkage::default());
        assert!(cfg.linkage.executable.is_none());
    }

    #[test]
    fn linkage_resolves_explicit_then_derived() {
        // Explicit wins.
        let explicit = ComponentLinkage {
            crate_name: Some("my_crate".into()),
            executable: Some("my_exe".into()),
            exported_symbol: Some("my_sym".into()),
            static_library: None,
        };
        assert_eq!(explicit.resolved_crate_name("ros-pkg"), "my_crate");
        assert_eq!(explicit.resolved_executable("talker"), "my_exe");
        assert_eq!(explicit.resolved_exported_symbol("talker"), "my_sym");

        // Derived from name / crate convention.
        let bare = ComponentLinkage::default();
        assert_eq!(bare.resolved_crate_name("ros-pkg"), "ros_pkg");
        assert_eq!(bare.resolved_executable("talker"), "talker");
        assert_eq!(
            bare.resolved_exported_symbol("talker"),
            "nros_component_talker"
        );
    }

    /// An `[overrides]` table that sets only `default_namespace` (no
    /// `parameters`/`remaps`) is also legal now.
    #[test]
    fn partial_overrides_defaults_missing_fields() {
        let raw = r#"
            version = 1
            package = "p"
            component = "c"
            language = "rust"

            [linkage]
            crate_name = "p"

            [metadata]
            source_metadata = "m.json"

            [overrides]
            default_namespace = "/demo"
        "#;
        let cfg: ComponentConfig = toml::from_str(raw).expect("partial overrides parse");
        assert_eq!(cfg.overrides.default_namespace.as_deref(), Some("/demo"));
        assert!(cfg.overrides.parameters.is_empty());
        assert!(cfg.overrides.remaps.is_empty());
    }
}
