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
//! [`node_callback_groups`]. `codegen-system`'s
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
/// The macro calls this and nothing else to build the map it derives from, so
/// a test that calls it measures the macro's input, not a copy of it.
pub fn model_node_groups(
    model: &SystemModel,
    keep: impl Fn(&str) -> bool,
) -> BTreeMap<String, Vec<CallbackGroupDecl>> {
    let mut out = BTreeMap::new();
    for fqn in model.structure.nodes.keys() {
        if !keep(fqn) {
            continue;
        }
        let bare = fqn.rsplit('/').next().unwrap_or(fqn).to_string();
        let rows = node_callback_groups(&[], &node_group_bindings(model, fqn));
        if !rows.is_empty() {
            out.insert(bare, rows);
        }
    }
    out
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
    fn with_nothing_declared_the_bindings_are_the_groups() {
        let rows = node_callback_groups(&[], &bound(&[("ctrl", "high")]));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].sched_task, "high");
        assert!(node_callback_groups(&[], &BTreeMap::new()).is_empty());
    }
}
