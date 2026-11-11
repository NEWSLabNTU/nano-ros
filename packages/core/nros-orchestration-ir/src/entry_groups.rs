//! issue 2432 — the derived schedule's INPUT, built in one place for every road.
//!
//! Three roads derive a rate-monotonic schedule from one model through one core
//! ([`crate::derive`]): `codegen entry` (C/C++), `codegen-system` (the Zephyr
//! bake) and the `nros::main!` proc-macro (Rust). The core is shared; what each
//! road fed it was not. Each built the per-node callback-group map — the
//! derivation's third argument — by hand, and the three spellings disagreed on
//! where a group comes from. Issue 2432 measured the cost: one workspace and one
//! contract derived two sched tasks in C and C++ and none in Rust, silently.
//!
//! This module holds the two halves every road needs:
//!
//! * [`node_group_bindings`] / [`node_callback_groups`] — the map's rows, one
//!   node at a time, from the groups the node's CODE declares plus the bindings
//!   the model resolved;
//! * [`declined`] / [`derived_lines`] — what the build says about it. A road
//!   that derives prints its plan; a road that declines prints one line naming
//!   why. Without that line the Rust image's empty table was indistinguishable
//!   from a table nobody asked for.
//!
//! It lives here, not in `nros-cli-core`, because the proc-macro cannot depend
//! on the CLI (the same reason [`crate::sidecar_slots`] does).
//!
//! `codegen entry` and `nros::main!` build their rows with
//! [`node_callback_groups`]; each road supplies the DECLARED groups from where
//! its language states them (the cmake keyword, or the Rust sidecar via
//! [`groups_of_component`]). `codegen-system`'s
//! `sched_task_resolver::collect_callback_groups` still has its own merge: an
//! authored `group_tasks` there HIDES every keyword group it does not name,
//! where the entry keeps such a group on the default sched task. The two agree
//! whenever a node binds every group it declares or none of them; the case
//! where they do not is pinned by that function's own test
//! (`an_authored_binding_still_outranks_the_keyword`) and recorded on issue
//! 2432, not changed here.

use std::collections::BTreeMap;

use ros_launch_manifest_model::SystemModel;

use crate::{CallbackGroupDecl, DEFAULT_SCHED_TASK, derive::DerivedSchedule};

/// The model's resolved bindings for ONE node, as `group id -> sched task`.
///
/// A `<fqn>/<group>` key binds that group. A bare `<fqn>` key binds the whole
/// node, i.e. its default group ([`DEFAULT_SCHED_TASK`] as the group id).
pub fn node_group_bindings(model: &SystemModel, fqn: &str) -> BTreeMap<String, String> {
    let prefix = format!("{fqn}/");
    let mut out = BTreeMap::new();
    for (key, sched_task) in &model.execution.bindings {
        if let Some(group) = key.strip_prefix(&prefix) {
            out.insert(group.to_string(), sched_task.clone());
        } else if key == fqn {
            out.insert(DEFAULT_SCHED_TASK.to_string(), sched_task.clone());
        }
    }
    out
}

/// One node's row of the derivation's callback-group map.
///
/// `declared` is the groups the node's code states (the cmake
/// `CALLBACK_GROUPS` keyword on the C/C++ roads); `bound` is
/// [`node_group_bindings`] or the equivalent `group_tasks` map.
///
/// * Declared groups come first, each on its bound sched task or, unbound, on
///   [`DEFAULT_SCHED_TASK`] — `derive::placement_is_unauthored` rule 4, the
///   shape the rate-monotonic derivation keys on.
/// * With nothing declared, the bound groups are the node's groups.
///
/// This is `codegen entry`'s rule, which predates the module; the macro now
/// asks the same function rather than a copy of it.
pub fn node_callback_groups(
    declared: &[String],
    bound: &BTreeMap<String, String>,
) -> Vec<CallbackGroupDecl> {
    if !declared.is_empty() {
        return crate::groups_at_default_sched_task(declared)
            .into_iter()
            .map(|mut d| {
                if let Some(sched_task) = bound.get(&d.id) {
                    d.sched_task = sched_task.clone();
                }
                d
            })
            .collect();
    }
    bound
        .iter()
        .map(|(id, sched_task)| CallbackGroupDecl {
            id: id.clone(),
            r#type: "MutuallyExclusive".to_string(),
            sched_task: sched_task.clone(),
        })
        .collect()
}

/// The `nros::main!` road's whole callback-group map: one
/// [`node_callback_groups`] row per node the image keeps, keyed by the node's
/// bare name (what `resolve_sched_tasks` matches members on).
///
/// `declared(pkg, exec)` is the group ids the node's CODE states. On the Rust
/// road that is the source-metadata sidecar `nros sync` writes
/// ([`groups_of_component`]) - the Rust spelling of what the cmake
/// `CALLBACK_GROUPS` keyword carries to `codegen entry`. Issue 2432: the macro
/// read the model's bindings alone, so a Rust node whose code names its groups
/// and whose `system.toml` binds none reached the derivation groupless, and the
/// image ran on one task where its C and C++ twins derived a schedule.
///
/// The macro calls this and nothing else to build the map it derives from, so
/// a test that calls it measures the macro's input, not a copy of it.
pub fn model_node_groups(
    model: &SystemModel,
    keep: impl Fn(&str) -> bool,
    declared: impl Fn(&str, &str) -> Vec<String>,
) -> BTreeMap<String, Vec<CallbackGroupDecl>> {
    let mut out = BTreeMap::new();
    for (fqn, inst) in &model.structure.nodes {
        if !keep(fqn) {
            continue;
        }
        let bare = fqn.rsplit('/').next().unwrap_or(fqn).to_string();
        let ids = match (inst.pkg.as_deref(), inst.exec.as_deref()) {
            (Some(pkg), Some(exec)) => declared(pkg, exec),
            _ => Vec::new(),
        };
        let rows = node_callback_groups(&ids, &node_group_bindings(model, fqn));
        if !rows.is_empty() {
            out.insert(bare, rows);
        }
    }
    out
}

/// `(package, executable)` + the distinct callback-group ids one source-metadata
/// sidecar records (`callbacks[].group`, nulls skipped), in first-seen order.
///
/// `None` when the document lacks the identity keys, the same rule as
/// [`crate::sidecar_slots::slots_of_component`]: an unusable sidecar is skipped,
/// never fatal. `serde_json::Value`-based for the reason that module gives - the
/// macro must not mirror the CLI's schema struct.
pub fn groups_of_component(sidecar: &serde_json::Value) -> Option<((String, String), Vec<String>)> {
    let package = sidecar.get("package")?.as_str()?.to_string();
    let executable = sidecar.get("executable")?.as_str()?.to_string();
    let mut ids: Vec<String> = Vec::new();
    for cb in sidecar
        .get("callbacks")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(g) = cb.get("group").and_then(serde_json::Value::as_str)
            && !ids.iter().any(|i| i == g)
        {
            ids.push(g.to_string());
        }
    }
    Some(((package, executable), ids))
}

/// Why a road did NOT run the derivation. Its `Display` is the line the road
/// prints, so the three roads say it in one spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Declined {
    /// No node reached the derivation with a callback group, so there is
    /// nothing a derived sched task could bind.
    NoCallbackGroups,
    /// Every group's sched task carries an authored placement for this target
    /// (`[tasks.<name>.<rtos>]`), and authored wins (RFC-0079, issue 1426).
    PlacementAuthored { target_rtos: String },
}

impl core::fmt::Display for Declined {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Declined::NoCallbackGroups => write!(
                f,
                "derived schedule: declined - no node declares a callback group, so \
                 every callback runs on the default sched task"
            ),
            Declined::PlacementAuthored { target_rtos } => write!(
                f,
                "derived schedule: declined - every callback group's sched task is \
                 authored for `{target_rtos}` (`[tasks.*.{target_rtos}]`), and an \
                 authored placement wins"
            ),
        }
    }
}

/// `Some(reason)` when a road must not derive; `None` when it must.
///
/// The second arm is exactly `derive::placement_is_unauthored`, so a road that
/// asks this asks the shared predicate.
pub fn declined(
    model: &SystemModel,
    target_rtos: &str,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
) -> Option<Declined> {
    if callback_groups.values().all(Vec::is_empty) {
        return Some(Declined::NoCallbackGroups);
    }
    if !crate::derive::placement_is_unauthored(model, target_rtos, callback_groups) {
        return Some(Declined::PlacementAuthored {
            target_rtos: target_rtos.to_string(),
        });
    }
    None
}

/// The lines a road prints after it derived: one per derived sched task and per
/// allocated placement, or ONE line saying the derivation ran and ranked
/// nothing (no node carries a rate the contract states).
pub fn derived_lines(derived: &DerivedSchedule, target_rtos: &str) -> Vec<String> {
    if derived.sched_tasks.is_empty() && derived.placements.is_empty() {
        return vec![
            "derived schedule: ran, derived nothing - no contracted node states a rate \
             that ranks it, so every callback stays on the default sched task"
                .to_string(),
        ];
    }
    let mut out = Vec::new();
    for (name, def) in &derived.sched_tasks {
        match crate::rtos_spec(def, target_rtos) {
            Some(spec) => out.push(format!(
                "derived schedule: sched task `{name}` priority {} ({target_rtos})",
                spec.priority
            )),
            None => out.push(format!("derived schedule: sched task `{name}`")),
        }
    }
    for (name, spec) in &derived.placements {
        out.push(format!(
            "derived schedule: authored sched task `{name}` allocated priority {} \
             ({target_rtos})",
            spec.priority
        ));
    }
    out
}

/// The derived table as `sched task -> (priority, member nodes)`, the shape the
/// cross-road parity test compares. Members come from the derivation's node
/// overrides.
pub fn derived_table(
    derived: &DerivedSchedule,
    target_rtos: &str,
) -> BTreeMap<String, (Option<i64>, Vec<String>)> {
    let mut out: BTreeMap<String, (Option<i64>, Vec<String>)> = derived
        .sched_tasks
        .iter()
        .map(|(name, def)| {
            (
                name.clone(),
                (
                    crate::rtos_spec(def, target_rtos).map(|s| s.priority),
                    Vec::new(),
                ),
            )
        })
        .collect();
    for ov in &derived.overrides {
        for cg in &ov.callback_groups {
            if let Some(row) = out.get_mut(&cg.sched_task) {
                if !row.1.contains(&ov.name) {
                    row.1.push(ov.name.clone());
                }
            }
        }
    }
    for row in out.values_mut() {
        row.1.sort();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound(rows: &[(&str, &str)]) -> BTreeMap<String, String> {
        rows.iter()
            .map(|(g, t)| ((*g).to_string(), (*t).to_string()))
            .collect()
    }

    #[test]
    fn declared_groups_land_on_the_default_sched_task_unless_bound() {
        let rows = node_callback_groups(
            &["ctrl".to_string(), "telem".to_string()],
            &bound(&[("ctrl", "high")]),
        );
        let got: Vec<(&str, &str)> = rows
            .iter()
            .map(|d| (d.id.as_str(), d.sched_task.as_str()))
            .collect();
        assert_eq!(got, vec![("ctrl", "high"), ("telem", DEFAULT_SCHED_TASK)]);
    }

    #[test]
    fn a_sidecar_names_its_distinct_groups_and_skips_nulls() {
        let doc = serde_json::json!({
            "package": "control_node",
            "executable": "control_node",
            "callbacks": [
                {"id": "on_tick", "group": "ctrl"},
                {"id": "on_msg", "group": null},
                {"id": "on_other", "group": "ctrl"},
                {"id": "on_slow", "group": "telem"},
            ],
        });
        let ((pkg, exec), ids) = groups_of_component(&doc).expect("identity keys present");
        assert_eq!(
            (pkg.as_str(), exec.as_str()),
            ("control_node", "control_node")
        );
        assert_eq!(ids, vec!["ctrl".to_string(), "telem".to_string()]);
        assert!(groups_of_component(&serde_json::json!({"callbacks": []})).is_none());
    }

    #[test]
    fn with_nothing_declared_the_bindings_are_the_groups() {
        let rows = node_callback_groups(&[], &bound(&[("ctrl", "high")]));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].sched_task, "high");
        assert!(node_callback_groups(&[], &BTreeMap::new()).is_empty());
    }
}
