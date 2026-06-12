//! Phase 240.2b (RFC-0043) — `nros-metadata.json` → typed-Entry enrichment.
//!
//! The launch XML carries only `(pkg, exec, name, ns)` per `<node>` — never the
//! C++ component class or its header. The cmake fn `nano_ros_node_register(NAME
//! CLASS [HEADER] …)` records those into `${CMAKE_BINARY_DIR}/nros-metadata.json`
//! (`components[]`). This module reads that file, keys each component by
//! `(pkg, exec)`, and stamps `class_name` / `class_header` onto the matching
//! [`PlanNode`]s so [`super::emit_cpp::emit_typed`] can construct the components.
//!
//! Key derivation: the metadata component has `{name, class, class_header}` and
//! NO explicit `pkg`, but `nano_ros_node_register` enforces `class` starts with
//! `${PROJECT_NAME}::` (L.4), so `pkg = class.split("::").next()` and `exec =
//! name`. That `(pkg, exec)` is exactly the launch-XML `(pkg, exec)` (the cmake
//! `NAME` arg IS the launch `exec`; `PROJECT_NAME` IS the launch `pkg`).

use std::{collections::HashMap, path::Path};

use eyre::{Context, Result, bail};
use serde::Deserialize;

use super::Plan;

/// One `components[]` entry from `nros-metadata.json` (only the fields the typed
/// Entry needs; the rest — sources/deploy/pkg_dir/lang — are ignored here).
#[derive(Debug, Deserialize)]
struct ComponentMeta {
    name: String,
    class: String,
    #[serde(default)]
    class_header: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MetadataDoc {
    #[serde(default)]
    components: Vec<ComponentMeta>,
}

/// `(pkg, exec)` → `(class, class_header)` lookup built from the metadata.
#[derive(Debug, Default)]
pub struct ComponentIndex {
    by_key: HashMap<(String, String), (String, Option<String>)>,
}

impl ComponentIndex {
    /// Parse `nros-metadata.json` at `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read metadata `{}`", path.display()))?;
        Self::parse(&raw).with_context(|| format!("parse metadata `{}`", path.display()))
    }

    /// Parse metadata JSON from a string (test seam).
    pub fn parse(raw: &str) -> Result<Self> {
        let doc: MetadataDoc = serde_json::from_str(raw).context("parse metadata JSON")?;
        let mut by_key = HashMap::new();
        for c in doc.components {
            // pkg = class prefix before the first `::` (L.4 enforced by cmake).
            let Some((pkg, _)) = c.class.split_once("::") else {
                bail!(
                    "metadata component `{}` has class `{}` without a `::` namespace — \
                     cannot derive its pkg (nano_ros_node_register enforces `pkg::Class`)",
                    c.name,
                    c.class
                );
            };
            by_key.insert(
                (pkg.to_string(), c.name.clone()),
                (c.class.clone(), c.class_header.clone()),
            );
        }
        Ok(Self { by_key })
    }

    /// Look up a component by `(pkg, exec)`.
    fn get(&self, pkg: &str, exec: &str) -> Option<&(String, Option<String>)> {
        self.by_key.get(&(pkg.to_string(), exec.to_string()))
    }
}

/// Stamp `class_name` / `class_header` onto every [`PlanNode`] from the metadata.
///
/// Errors if any launch node has no matching `(pkg, exec)` component, or a match
/// lacks a `class_header` — the typed emitter needs both, and a silent miss would
/// surface later as a confusing emit error.
pub fn enrich_plan(plan: &mut Plan, index: &ComponentIndex) -> Result<()> {
    for n in &mut plan.nodes {
        let Some((class, header)) = index.get(&n.pkg, &n.exec) else {
            bail!(
                "typed entry: launch node pkg `{}` exec `{}` has no matching component in \
                 nros-metadata.json — is it declared with nano_ros_node_register(NAME {} CLASS {}::… )?",
                n.pkg,
                n.exec,
                n.exec,
                n.pkg
            );
        };
        let Some(header) = header else {
            bail!(
                "typed entry: component `{}::{}` (pkg `{}`) has no class_header in metadata",
                n.pkg,
                n.exec,
                n.pkg
            );
        };
        n.class_name = Some(class.clone());
        n.class_header = Some(header.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::entry::PlanNode;
    use std::path::PathBuf;

    const META: &str = r#"{
      "components": [
        {"name": "talker", "class": "talker_pkg::Talker",
         "class_header": "talker_pkg/Talker.hpp",
         "sources": ["src/Talker.cpp"], "deploy": ["native"],
         "pkg_dir": "/ws/src/talker_pkg", "lang": "cpp"},
        {"name": "listener", "class": "listener_pkg::Listener",
         "class_header": "listener_pkg/Listener.hpp",
         "sources": ["src/Listener.cpp"], "deploy": ["native"],
         "pkg_dir": "/ws/src/listener_pkg", "lang": "cpp"}
      ],
      "applications": [],
      "deploy_targets": {}
    }"#;

    fn plan(nodes: &[(&str, &str)]) -> Plan {
        Plan {
            board: "native".into(),
            nodes: nodes
                .iter()
                .map(|(pkg, exec)| PlanNode {
                    pkg: (*pkg).into(),
                    exec: (*exec).into(),
                    name: Some((*exec).into()),
                    namespace: None,
                    class_name: None,
                    class_header: None,
                })
                .collect(),
            depfile_paths: Vec::new(),
            bringup: "demo_bringup".into(),
            launch_file: PathBuf::from("/tmp/system.launch.xml"),
        }
    }

    #[test]
    fn enrich_stamps_class_and_header() {
        let index = ComponentIndex::parse(META).unwrap();
        let mut p = plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
        enrich_plan(&mut p, &index).unwrap();
        assert_eq!(p.nodes[0].class_name.as_deref(), Some("talker_pkg::Talker"));
        assert_eq!(
            p.nodes[0].class_header.as_deref(),
            Some("talker_pkg/Talker.hpp")
        );
        assert_eq!(
            p.nodes[1].class_name.as_deref(),
            Some("listener_pkg::Listener")
        );
    }

    #[test]
    fn missing_component_errors_with_pkg_exec() {
        let index = ComponentIndex::parse(META).unwrap();
        let mut p = plan(&[("ghost_pkg", "ghost")]);
        let err = enrich_plan(&mut p, &index).unwrap_err().to_string();
        assert!(err.contains("ghost_pkg"), "{err}");
        assert!(err.contains("ghost"), "{err}");
    }

    #[test]
    fn class_without_namespace_is_rejected() {
        let bad = r#"{"components":[{"name":"x","class":"NoNamespace","class_header":"x.hpp"}]}"#;
        let err = ComponentIndex::parse(bad).unwrap_err().to_string();
        assert!(err.contains("without a `::`"), "{err}");
    }

    #[test]
    fn header_absent_errors() {
        let no_hdr = r#"{"components":[{"name":"talker","class":"talker_pkg::Talker"}]}"#;
        let index = ComponentIndex::parse(no_hdr).unwrap();
        let mut p = plan(&[("talker_pkg", "talker")]);
        let err = enrich_plan(&mut p, &index).unwrap_err().to_string();
        assert!(err.contains("no class_header"), "{err}");
    }
}
