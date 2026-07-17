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
use nros_orchestration_ir::{CallbackGroupDecl, CallbackGroupOverride, NodeOverride};
use ros_launch_manifest_model::SystemModel;
use ros_launch_manifest_sched::{TierDef as ModelTierDef, TierPlatformSpec};

use crate::orchestration::cargo_metadata_schema::{SystemToml, TierDef, TierRtosSpec};

/// Load + schema-gate a SystemModel.
pub fn load_model(path: &Path) -> Result<SystemModel> {
    let yaml = std::fs::read_to_string(path)
        .with_context(|| format!("read SystemModel {}", path.display()))?;
    SystemModel::from_yaml_str(&yaml)
        .map_err(|e| eyre::eyre!("load SystemModel {}: {e}", path.display()))
}

fn rtos_spec(spec: &TierPlatformSpec) -> TierRtosSpec {
    TierRtosSpec {
        priority: spec.priority,
        stack_bytes: spec.stack_bytes,
        preempt_threshold: spec.preempt_threshold,
        sched_class: spec.sched_class.clone(),
    }
}

/// Convert one shared-schema tier into the orchestration-ir shape.
///
/// Two deliberate seams (the schemas differ here, conversion is the
/// contract):
/// - `core` lives per-platform in the model but at the tier head in the
///   ir — hoisted from the SELECTED target's sub-table.
/// - a per-platform `deadline_us` override (model) tightens the portable
///   head value for the selected target.
pub fn tier_from_model(t: &ModelTierDef, target_rtos: &str) -> TierDef {
    let selected = t.platform(target_rtos);
    TierDef {
        spin_period_us: t.spin_period_us,
        class: t.class.clone(),
        period_us: t.period_us,
        budget_us: t.budget_us,
        deadline_us: selected.and_then(|s| s.deadline_us).or(t.deadline_us),
        deadline_policy: t.deadline_policy.clone(),
        core: selected.and_then(|s| s.core),
        freertos: t.freertos.as_ref().map(rtos_spec),
        zephyr: t.zephyr.as_ref().map(rtos_spec),
        threadx: t.threadx.as_ref().map(rtos_spec),
        nuttx: t.nuttx.as_ref().map(rtos_spec),
        posix: t.posix.as_ref().map(rtos_spec),
    }
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

    /// Mirror-drift guard: every field of the shared-schema tier must
    /// survive conversion. If either schema grows a field, this test is
    /// the tripwire — extend the conversion AND this fixture together.
    #[test]
    fn tier_roundtrip_covers_every_field() {
        let model_tier = ModelTierDef {
            class: Some("real_time".to_string()),
            deadline_us: Some(2000),
            period_us: Some(1000),
            budget_us: Some(500),
            deadline_policy: Some("skip".to_string()),
            spin_period_us: Some(250),
            posix: Some(TierPlatformSpec {
                priority: 80,
                stack_bytes: Some(65536),
                core: Some(2),
                sched_class: Some("SCHED_FIFO".to_string()),
                preempt_threshold: None,
                deadline_us: Some(1500), // per-platform tighten
            }),
            freertos: Some(TierPlatformSpec {
                priority: 5,
                stack_bytes: Some(32768),
                core: None,
                sched_class: None,
                preempt_threshold: None,
                deadline_us: None,
            }),
            zephyr: None,
            threadx: Some(TierPlatformSpec {
                priority: 4,
                stack_bytes: None,
                core: Some(1),
                sched_class: None,
                preempt_threshold: Some(4),
                deadline_us: None,
            }),
            nuttx: None,
        };

        // Selected target = posix: head core + deadline hoist from posix.
        let ir = tier_from_model(&model_tier, "posix");
        assert_eq!(ir.spin_period_us, Some(250));
        assert_eq!(ir.class.as_deref(), Some("real_time"));
        assert_eq!(ir.period_us, Some(1000));
        assert_eq!(ir.budget_us, Some(500));
        assert_eq!(ir.deadline_us, Some(1500), "per-platform override wins");
        assert_eq!(ir.deadline_policy.as_deref(), Some("skip"));
        assert_eq!(ir.core, Some(2), "core hoisted from selected platform");
        let posix = ir.posix.as_ref().unwrap();
        assert_eq!(posix.priority, 80);
        assert_eq!(posix.stack_bytes, Some(65536));
        assert_eq!(posix.sched_class.as_deref(), Some("SCHED_FIFO"));
        let tx = ir.threadx.as_ref().unwrap();
        assert_eq!(tx.priority, 4);
        assert_eq!(tx.preempt_threshold, Some(4));
        assert_eq!(ir.freertos.as_ref().unwrap().stack_bytes, Some(32768));
        assert!(ir.zephyr.is_none() && ir.nuttx.is_none());

        // Selected target = threadx: its core hoists; head deadline stands.
        let ir_tx = tier_from_model(&model_tier, "threadx");
        assert_eq!(ir_tx.core, Some(1));
        assert_eq!(ir_tx.deadline_us, Some(2000));
    }

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
    /// Declared publisher guarantee, milli-Hz.
    pub min_rate_hz_milli: u32,
}

/// Extract the publisher rate-monitor rows: every `pub_endpoints` entry
/// with `min_rate_hz`, joined to the topic whose wiring lists it as a
/// publisher. A contracted endpoint with NO owning topic in the wiring is
/// a model inconsistency — fail loud.
pub fn monitor_rows(model: &SystemModel) -> Result<Vec<MonitorRow>> {
    let mut rows = Vec::new();
    for (ep_ref, c) in &model.contracts.pub_endpoints {
        let Some(min) = c.min_rate_hz else { continue };
        let topic = model
            .structure
            .topics
            .iter()
            .find(|(_, w)| w.publishers.iter().any(|p| p == ep_ref))
            .map(|(t, _)| t.clone());
        let Some(topic) = topic else {
            bail!(
                "SystemModel: contracted publisher '{ep_ref}' (min_rate_hz) has no \
                 owning topic in structure.topics — inconsistent model"
            );
        };
        rows.push(MonitorRow {
            topic,
            fqn: ep_ref.clone(),
            min_rate_hz_milli: (min * 1000.0).round().max(0.0).min(u32::MAX as f64) as u32,
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
pub fn render_monitor_rs(rows: &[MonitorRow]) -> String {
    let mut out = String::new();
    out.push_str(
        "// GENERATED by `nros codegen-system --model` (RFC-0052 W3b.4 / phase-296 N1).\n\
         // One PubMonitorCell per contracted publisher + the executor monitor table.\n\
         // Include from the entry; call `nros_install_monitors(&mut executor)` and\n\
         // `node.set_monitors(NROS_MONITORS)` BEFORE entity creation.\n\
         use ::nros_node::executor::monitor::{MonitorSpec, PubMonitorCell};\n\n",
    );
    for (i, _r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "static NROS_MONITOR_CELL_{i}: PubMonitorCell = PubMonitorCell::new();\n"
        ));
    }
    out.push_str("\npub static NROS_MONITORS: &[MonitorSpec] = &[\n");
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "    MonitorSpec {{ topic: {t:?}, fqn: {f:?}, min_rate_hz_milli: {m}u32, \
             cell: &NROS_MONITOR_CELL_{i} }},\n",
            t = r.topic,
            f = r.fqn,
            m = r.min_rate_hz_milli,
        ));
    }
    out.push_str("];\n\n");
    out.push_str(
        "pub fn nros_install_monitors(executor: &mut ::nros_node::executor::Executor<'_>) {\n             executor.set_monitor_table(NROS_MONITORS);\n}\n",
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
        let rs = render_monitor_rs(&rows);
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
        let rs = render_monitor_rs(&rows);
        assert!(rs.contains("NROS_MONITORS: &[MonitorSpec] = &[\n];"));
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
