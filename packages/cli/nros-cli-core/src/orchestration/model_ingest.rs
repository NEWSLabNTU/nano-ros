//! RFC-0052 / phase-296 W1 — SystemModel ingestion.
//!
//! `nros codegen-system --model system_model.yaml` consumes the checked
//! artifact play_launch `resolve` emits (RFC-0050): the model's execution
//! layer REPLACES the bringup's `[tiers.*]` + `[[node_overrides]]` before
//! the existing `resolve_system_tiers` pipeline runs — same resolver, same
//! `nros-plan.json`, same `run_tiers` output as the `system.toml`-authored
//! equivalent, by construction.
//!
//! Schema note: the tier tables exist twice by design — the shared
//! `ros-launch-manifest-sched::TierDef` (authoring + model schema, both
//! projects vendor it) and `nros-orchestration-ir::TierDef` (the no_std
//! resolver input the proc-macro also uses). This module is the ONE
//! conversion point; `tier_roundtrip_covers_every_field` is the
//! mirror-drift guard (issue-0160 lesson, applied cross-repo).

use std::{collections::BTreeMap, path::Path};

use eyre::{Context, Result, bail};
// RFC-0052 — `tier_from_model` moved to nros-orchestration-ir (the shared home
// for the CLI codegen + the nros::main! proc-macro); re-exported here so the
// existing `model_ingest::tier_from_model` call sites and tests keep working.
pub use nros_orchestration_ir::tier_from_model;
use nros_orchestration_ir::{CallbackGroupDecl, CallbackGroupOverride, NodeOverride};
use ros_launch_manifest_model::SystemModel;

use crate::orchestration::cargo_metadata_schema::SystemToml;

/// Load + schema-gate a SystemModel.
pub fn load_model(path: &Path) -> Result<SystemModel> {
    let yaml = std::fs::read_to_string(path)
        .with_context(|| format!("read SystemModel {}", path.display()))?;
    SystemModel::from_yaml_str(&yaml)
        .map_err(|e| eyre::eyre!("load SystemModel {}: {e}", path.display()))
}

/// Bare node name from a model FQN (`/ns/node` → `node`).
fn bare(fqn: &str) -> &str {
    fqn.rsplit('/').next().unwrap_or(fqn)
}

/// Apply the model's execution layer onto the bringup system: tiers
/// replace `[tiers.*]`, bindings become `[[node_overrides]]` rows.
///
/// Binding keys are `<node FQN>` (whole node → every declared group) or
/// `<node FQN>/<callback group>`. Fail-loud (RFC-0052): a binding that
/// names no known component, or a group the node never declares, is an
/// error — never a silent no-op.
pub fn apply_model_execution(
    system: &mut SystemToml,
    model: &SystemModel,
    target_rtos: &str,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
) -> Result<()> {
    system.tiers = model
        .execution
        .tiers
        .iter()
        .map(|(name, t)| (name.clone(), tier_from_model(t, target_rtos)))
        .collect();

    let component_names: Vec<&str> = system.components.iter().map(|c| c.name.as_str()).collect();

    let mut overrides: BTreeMap<String, Vec<CallbackGroupOverride>> = BTreeMap::new();
    for (key, tier) in &model.execution.bindings {
        if !model.execution.tiers.contains_key(tier) {
            bail!("SystemModel binding '{key}' references undeclared tier '{tier}'");
        }
        // `<FQN>/<group>` vs `<FQN>`: the segment before the last is the
        // node when the last segment matches one of its declared groups.
        let (node, groups): (String, Vec<String>) = {
            let node_level = bare(key);
            if component_names.contains(&node_level) {
                // whole-node binding → every declared group (or the
                // implicit default group when none are declared).
                let declared = callback_groups
                    .get(node_level)
                    .map(|gs| gs.iter().map(|g| g.id.clone()).collect::<Vec<_>>())
                    .unwrap_or_default();
                if declared.is_empty() {
                    bail!(
                        "SystemModel binding '{key}': node '{node_level}' declares no \
                         callback groups to bind (whole-node tiering needs at least \
                         the node's group declarations in its package metadata)"
                    );
                }
                (node_level.to_string(), declared)
            } else if let Some((node_part, group)) = key.rsplit_once('/')
                && !bare(node_part).is_empty()
            {
                let node = bare(node_part).to_string();
                if !component_names.contains(&node.as_str()) {
                    bail!(
                        "SystemModel binding '{key}': no component named '{node}' \
                         in the bringup (components: {component_names:?})"
                    );
                }
                let declared = callback_groups.get(&node);
                if !declared.is_some_and(|gs| gs.iter().any(|g| g.id == group)) {
                    bail!(
                        "SystemModel binding '{key}': node '{node}' declares no \
                         callback group '{group}'"
                    );
                }
                (node, vec![group.to_string()])
            } else {
                bail!(
                    "SystemModel binding '{key}': no component named '{node_level}' \
                     in the bringup (components: {component_names:?})"
                );
            }
        };
        let entry = overrides.entry(node).or_default();
        for g in groups {
            entry.push(CallbackGroupOverride {
                id: g,
                tier: tier.clone(),
            });
        }
    }
    system.node_overrides = overrides
        .into_iter()
        .map(|(name, callback_groups)| NodeOverride {
            name,
            callback_groups,
        })
        .collect();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_to_unknown_tier_or_node_fails_loud() {
        use ros_launch_manifest_model::SystemModel;
        let mut system: SystemToml =
            toml::from_str("[system]\nname = \"t\"\nrmw = \"zenoh\"\ndomain_id = 0\n")
                .expect("minimal system.toml parses");
        let mut model = SystemModel::default();
        model
            .execution
            .bindings
            .insert("/ctrl/ctrl_node".to_string(), "high".to_string());
        let err = apply_model_execution(&mut system, &model, "posix", &BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert!(err.contains("undeclared tier 'high'"), "got: {err}");
    }
}

/// R1-N1 — one contracted-publisher monitor row extracted from the model
/// (RFC-0052 W3b.4 consumer side).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MonitorRow {
    /// Topic FQN (the wiring name the publisher creates).
    pub topic: String,
    /// Endpoint ref (`<node FQN>/<endpoint>`), the violation report key.
    pub fqn: String,
    /// Declared publisher guarantee, milli-Hz. 0 = no rate contract
    /// (latency-only row).
    pub min_rate_hz_milli: u32,
    /// W3b.5 — node-path budget (ms) for paths whose output is this
    /// endpoint (`contracts.node_paths`). 0 = no latency contract.
    #[serde(skip_serializing_if = "is_zero", default)]
    pub max_latency_ms: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// W3b.5 — one contracted-subscriber age row (`sub_endpoints.max_age_ms`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AgeRow {
    /// Topic FQN (the wiring name the subscriber creates).
    pub topic: String,
    /// Endpoint ref, the violation report key.
    pub fqn: String,
    /// Declared max take-age, ms.
    pub max_age_ms: u32,
}

/// Extract the publisher rate-monitor rows: every `pub_endpoints` entry
/// with `min_rate_hz`, joined to the topic whose wiring lists it as a
/// publisher. A contracted endpoint with NO owning topic in the wiring is
/// a model inconsistency — fail loud.
pub fn monitor_rows(model: &SystemModel) -> Result<Vec<MonitorRow>> {
    use std::collections::BTreeMap;
    // fqn -> (min_rate_milli, max_latency_ms); node_paths may add
    // latency-only rows for endpoints without a rate contract.
    let mut by_fqn: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for (ep_ref, c) in &model.contracts.pub_endpoints {
        let Some(min) = c.min_rate_hz else { continue };
        by_fqn.insert(
            ep_ref.clone(),
            (
                (min * 1000.0).round().max(0.0).min(u32::MAX as f64) as u32,
                0,
            ),
        );
    }
    // W3b.5 — node-path budgets attach to the path's OUTPUT endpoints.
    for (path_ref, p) in &model.contracts.node_paths {
        let Some(lat) = p.max_latency_ms else {
            continue;
        };
        let lat = lat.round().max(0.0).min(u32::MAX as f64) as u32;
        if lat == 0 {
            continue;
        }
        if p.output.is_empty() {
            bail!(
                "SystemModel: node path '{path_ref}' declares max_latency_ms but \
                 lists no output endpoint — inconsistent model"
            );
        }
        for out in &p.output {
            let e = by_fqn.entry(out.clone()).or_insert((0, 0));
            e.1 = e.1.max(lat);
        }
    }
    let mut rows = Vec::new();
    for (ep_ref, (min_milli, lat_ms)) in by_fqn {
        let topic = model
            .structure
            .topics
            .iter()
            .find(|(_, w)| w.publishers.iter().any(|p| p == &ep_ref))
            .map(|(t, _)| t.clone());
        let Some(topic) = topic else {
            bail!(
                "SystemModel: contracted publisher '{ep_ref}' has no \
                 owning topic in structure.topics — inconsistent model"
            );
        };
        rows.push(MonitorRow {
            topic,
            fqn: ep_ref,
            min_rate_hz_milli: min_milli,
            max_latency_ms: lat_ms,
        });
    }
    Ok(rows)
}

/// W3b.5 — extract the subscriber age rows: every `sub_endpoints` entry
/// with `max_age_ms`, joined to the topic whose wiring lists it as a
/// subscriber. Orphans fail loud (same rule as the publisher join).
pub fn age_rows(model: &SystemModel) -> Result<Vec<AgeRow>> {
    let mut rows = Vec::new();
    for (ep_ref, c) in &model.contracts.sub_endpoints {
        let Some(age) = c.max_age_ms else { continue };
        let topic = model
            .structure
            .topics
            .iter()
            .find(|(_, w)| w.subscribers.iter().any(|p| p == ep_ref))
            .map(|(t, _)| t.clone());
        let Some(topic) = topic else {
            bail!(
                "SystemModel: contracted subscriber '{ep_ref}' (max_age_ms) has no \
                 owning topic in structure.topics — inconsistent model"
            );
        };
        rows.push(AgeRow {
            topic,
            fqn: ep_ref.clone(),
            max_age_ms: age.round().max(1.0).min(u32::MAX as f64) as u32,
        });
    }
    rows.sort_by(|a, b| a.fqn.cmp(&b.fqn));
    Ok(rows)
}

/// R1-N1 — render the baked Rust monitor-table include
/// (`nros-system/system_monitors.rs`): one `PubMonitorCell` static per
/// contracted publisher + the `MONITORS` spec table + an installer the
/// generated entry calls before entity creation. Empty rows → an empty
/// table (DCE'd — the zero-cost gate).
pub fn render_monitor_rs(rows: &[MonitorRow], ages: &[AgeRow]) -> String {
    let mut out = String::new();
    out.push_str(
        "// GENERATED by `nros codegen-system --model` (RFC-0052 W3b.4/.5 / phase-296 N1).\n\
         // One PubMonitorCell per contracted publisher (+ one SubMonitorCell per age\n\
         // contract) + the executor monitor tables. Include from the entry; call\n\
         // `nros_install_monitors(&mut executor)` BEFORE entity creation (node-side\n\
         // attachment is auto-seeded from the executor at create_node).\n\
         use ::nros_node::executor::monitor::{AgeMonitorSpec, MonitorSpec, PubMonitorCell, SubMonitorCell};\n\n",
    );
    for (i, _r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "static NROS_MONITOR_CELL_{i}: PubMonitorCell = PubMonitorCell::new();\n"
        ));
    }
    for (i, _r) in ages.iter().enumerate() {
        out.push_str(&format!(
            "static NROS_AGE_CELL_{i}: SubMonitorCell = SubMonitorCell::new();\n"
        ));
    }
    out.push_str("\npub static NROS_MONITORS: &[MonitorSpec] = &[\n");
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "    MonitorSpec {{ topic: {t:?}, fqn: {f:?}, min_rate_hz_milli: {m}u32, \
             max_latency_ms: {l}u32, cell: &NROS_MONITOR_CELL_{i} }},\n",
            t = r.topic,
            f = r.fqn,
            m = r.min_rate_hz_milli,
            l = r.max_latency_ms,
        ));
    }
    out.push_str("];\n\n");
    out.push_str("pub static NROS_AGE_MONITORS: &[AgeMonitorSpec] = &[\n");
    for (i, r) in ages.iter().enumerate() {
        out.push_str(&format!(
            "    AgeMonitorSpec {{ topic: {t:?}, fqn: {f:?}, max_age_ms: {a}u32, \
             cell: &NROS_AGE_CELL_{i} }},\n",
            t = r.topic,
            f = r.fqn,
            a = r.max_age_ms,
        ));
    }
    out.push_str("];\n\n");
    out.push_str(
        "pub fn nros_install_monitors(executor: &mut ::nros_node::executor::Executor<'_>) {\n    executor.set_monitor_table(NROS_MONITORS);\n    executor.set_age_table(NROS_AGE_MONITORS);\n}\n",
    );
    out
}

#[cfg(test)]
mod monitor_tests {
    use super::*;
    use ros_launch_manifest_model::{PubContract, TopicWiring};

    fn model_with_contract() -> SystemModel {
        let mut m = SystemModel::default();
        m.structure.topics.insert(
            "/perception/objects".to_string(),
            TopicWiring {
                msg_type: "std_msgs/msg/String".to_string(),
                publishers: vec!["/perception/detector/objects".to_string()],
                subscribers: vec![],
            },
        );
        m.contracts.pub_endpoints.insert(
            "/perception/detector/objects".to_string(),
            PubContract {
                min_rate_hz: Some(10.0),
                ..Default::default()
            },
        );
        m
    }

    #[test]
    fn rows_join_endpoint_to_topic_and_render() {
        let rows = monitor_rows(&model_with_contract()).expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].topic, "/perception/objects");
        assert_eq!(rows[0].min_rate_hz_milli, 10_000);
        let rs = render_monitor_rs(&rows, &[]);
        assert!(rs.contains("NROS_MONITOR_CELL_0"));
        assert!(rs.contains("min_rate_hz_milli: 10000u32"));
        assert!(rs.contains("topic: \"/perception/objects\""));
    }

    #[test]
    fn orphan_contract_fails_loud() {
        let mut m = model_with_contract();
        m.structure.topics.clear();
        let err = monitor_rows(&m).unwrap_err().to_string();
        assert!(
            err.contains("no \\\n                 owning topic") || err.contains("no owning topic"),
            "{err}"
        );
    }

    #[test]
    fn empty_contracts_render_empty_table() {
        let rows = monitor_rows(&SystemModel::default()).expect("rows");
        assert!(rows.is_empty());
        let rs = render_monitor_rs(&rows, &[]);
        assert!(rs.contains("NROS_MONITORS: &[MonitorSpec] = &[\n];"));
        assert!(rs.contains("NROS_AGE_MONITORS: &[AgeMonitorSpec] = &[\n];"));
    }

    #[test]
    fn age_rows_join_subscriber_and_render() {
        use ros_launch_manifest_model::SubContract;
        let mut m = SystemModel::default();
        m.structure.topics.insert(
            "/sensing/scan".to_string(),
            TopicWiring {
                msg_type: "sensor_msgs/msg/LaserScan".to_string(),
                publishers: vec![],
                subscribers: vec!["/perception/detector/scan".to_string()],
            },
        );
        m.contracts.sub_endpoints.insert(
            "/perception/detector/scan".to_string(),
            SubContract {
                max_age_ms: Some(100.0),
                ..Default::default()
            },
        );
        let ages = age_rows(&m).expect("ages");
        assert_eq!(ages.len(), 1);
        assert_eq!(ages[0].topic, "/sensing/scan");
        assert_eq!(ages[0].max_age_ms, 100);
        let rs = render_monitor_rs(&[], &ages);
        assert!(rs.contains("NROS_AGE_CELL_0"));
        assert!(rs.contains("max_age_ms: 100u32"));
        assert!(rs.contains("set_age_table(NROS_AGE_MONITORS)"));

        // Orphan sub contract: fail loud.
        m.structure.topics.clear();
        let err = age_rows(&m).unwrap_err().to_string();
        assert!(
            err.contains("no owning topic") || err.contains("owning topic"),
            "{err}"
        );
    }

    #[test]
    fn node_path_budget_attaches_to_output_endpoint() {
        use ros_launch_manifest_model::PathContract;
        let mut m = model_with_contract();
        m.contracts.node_paths.insert(
            "/perception/detector/proc".to_string(),
            PathContract {
                input: vec!["/perception/detector/scan".to_string()],
                output: vec!["/perception/detector/objects".to_string()],
                max_latency_ms: Some(20.0),
                ..Default::default()
            },
        );
        let rows = monitor_rows(&m).expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].max_latency_ms, 20);
        assert_eq!(rows[0].min_rate_hz_milli, 10_000, "rate contract kept");
        let rs = render_monitor_rs(&rows, &[]);
        assert!(rs.contains("max_latency_ms: 20u32"));
    }
}

/// R1-N3 — convert the model's transport declarations into the plan's
/// [`PlanTransport`] shape (the type the board network bake consumes).
/// Unknown `kind` strings are a bake-time error (fail-loud).
pub fn plan_transports(
    model: &SystemModel,
) -> Result<Vec<crate::orchestration::plan::PlanTransport>> {
    use crate::orchestration::plan::{PlanTransport, TransportKind};
    let mut out = Vec::new();
    for t in &model.execution.transports {
        let kind = match t.kind.as_str() {
            "ethernet" => TransportKind::Ethernet,
            "wifi" => TransportKind::Wifi,
            "serial" => TransportKind::Serial,
            "can" => TransportKind::Can,
            other => bail!(
                "SystemModel transport kind '{other}' is not supported \
                 (ethernet | wifi | serial | can)"
            ),
        };
        out.push(PlanTransport {
            kind,
            id: t.id.clone(),
            ip: t.ip.clone(),
            ssid: t.ssid.clone(),
            password: t.password.clone(),
            mac: t.mac.clone(),
            gateway: t.gateway.clone(),
            interfaces: t.interfaces.clone(),
            device: t.device.clone(),
            baudrate: t.baudrate,
            rmw: t.rmw.clone(),
            locator: t.locator.clone(),
            domain: t.domain.map(u32::from),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use ros_launch_manifest_model::Transport;

    #[test]
    fn transports_convert_and_unknown_kind_fails() {
        let mut m = SystemModel::default();
        m.execution.transports.push(Transport {
            kind: "ethernet".to_string(),
            id: Some("eth0".to_string()),
            ip: Some("10.0.2.50/24".to_string()),
            mac: Some("02:00:00:00:00:01".to_string()),
            domain: Some(7),
            ..Default::default()
        });
        let pts = plan_transports(&m).expect("converts");
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].mac.as_deref(), Some("02:00:00:00:00:01"));
        assert_eq!(pts[0].domain, Some(7));

        m.execution.transports[0].kind = "carrier-pigeon".to_string();
        assert!(plan_transports(&m).is_err());
    }
}
