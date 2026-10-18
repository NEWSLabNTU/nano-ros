//! phase-462 W4 -- a contract MODE lowered onto the image's tables.
//!
//! A mode (`contracts.modes.<m>`) is a set of overrides over the SAME
//! entities: a tighter latency, a lower rate, a shorter age. The executor's
//! tables are static, so a mode lowers to a VARIANT of the monitor table --
//! the same rows in the same order, with the mode's numbers -- baked beside
//! the base and selected by a mode switch, never rebuilt at run time.
//!
//! This module is the road-independent half, so the CLI's entry emitters and
//! the `nros::main!` expansion read one answer:
//!
//! - [`apply_mode`] applies one mode's overrides to a copy of the model,
//!   through the resolver's `resolved` keys (rlm v0.1.63), never by
//!   re-reading `target`, whose vocabulary is the declaring scope's;
//! - [`variant_rows`] derives the monitor and age rows of that copy and
//!   REFUSES a mode whose rows are not the base's rows: a mode cannot add or
//!   remove a monitored endpoint, because that would change the tables'
//!   length and the pools sized from them.
//!
//! An override that changes no on-target row -- a scope path's budget, a
//! topic's rate -- is LISTED as not lowered ([`ModeVariant::not_lowered`]),
//! never applied silently and never refused: it is a requirement the
//! resolve-time checks hold, not one the image observes.

use std::{collections::BTreeMap, fmt};

use ros_launch_manifest_model::{
    ModeOverrideContract, OverrideTable, ResolvedOverride, SystemModel,
};
use ros_launch_manifest_types::duration::Duration;

use crate::{
    CallbackGroupDecl,
    contract_monitors::{self, AgeRow, MonitorRow},
};

/// One mode applied to the model.
#[derive(Debug, Clone)]
pub struct ModeVariant {
    /// The mode's key in `contracts.modes`.
    pub name: String,
    /// The model with the mode's overrides applied.
    pub model: SystemModel,
    /// Each override this image does not observe, with why, in contract
    /// order: `"<target>: <reason>"`.
    pub not_lowered: Vec<String>,
}

/// Why a mode cannot be lowered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeError {
    /// The model carries no `resolved` key for an override: written before
    /// rlm v0.1.63, or a target naming no declared requirement (which the
    /// resolver reported as `override-target-missing`).
    Unresolved { mode: String, target: String },
    /// The resolved key is not in its table: the override names an entity
    /// the base does not have.
    MissingEntity {
        mode: String,
        target: String,
        table: &'static str,
        key: String,
    },
    /// The value does not parse as the field's kind.
    BadValue {
        mode: String,
        target: String,
        value: String,
        expected: &'static str,
    },
    /// The mode's monitored-row set differs from the base's.
    RowSetChanged {
        mode: String,
        table: &'static str,
        added: Vec<String>,
        removed: Vec<String>,
    },
    /// Deriving the rows of the variant failed (the same refusal the base
    /// would give).
    Rows { mode: String, reason: String },
    /// phase-462 W4c -- the mode's derived schedule has a different set of
    /// tiers from the base's. Tier threads are created once at boot, so a
    /// mode may change a tier's deadline but not which tiers exist.
    TierSetChanged {
        mode: String,
        added: Vec<String>,
        removed: Vec<String>,
    },
    /// phase-462 W4c -- the mode switch cannot meet the latency its reaction
    /// path states: an executor enters the mode at its next spin, and the
    /// slowest declared spin period is longer than the budget.
    SwitchTooSlow {
        mode: String,
        reaction: String,
        budget_ms: u32,
        tier: String,
        period_us: u64,
    },
}

impl fmt::Display for ModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModeError::Unresolved { mode, target } => write!(
                f,
                "mode `{mode}`: override `{target}` has no resolved model key -- the model \
                 predates ros-launch-manifest v0.1.63, or the target names no declared \
                 requirement (the resolver reports that as `override-target-missing`). \
                 Re-run `nros sync` with a current resolver"
            ),
            ModeError::MissingEntity {
                mode,
                target,
                table,
                key,
            } => write!(
                f,
                "mode `{mode}`: override `{target}` names `{key}`, which `contracts.{table}` \
                 does not have. A mode overrides requirements of the entities the base mode \
                 declares; it cannot add one"
            ),
            ModeError::BadValue {
                mode,
                target,
                value,
                expected,
            } => write!(
                f,
                "mode `{mode}`: override `{target}` = `{value}` is not {expected}"
            ),
            ModeError::RowSetChanged {
                mode,
                table,
                added,
                removed,
            } => write!(
                f,
                "mode `{mode}` changes which endpoints the {table} table monitors (added: \
                 [{}], removed: [{}]). A mode lowers to a VARIANT of the base table -- the \
                 same rows with the mode's numbers -- so it may change a requirement's value \
                 but not whether it exists: state the requirement in the base contract and \
                 override its value in the mode",
                added.join(", "),
                removed.join(", ")
            ),
            ModeError::Rows { mode, reason } => write!(f, "mode `{mode}`: {reason}"),
            ModeError::TierSetChanged {
                mode,
                added,
                removed,
            } => write!(
                f,
                "mode `{mode}` changes which tiers the contract derives (added: [{}], \
                 removed: [{}]). Tier threads are created once at boot, so a mode may \
                 change a tier's deadline but not whether the tier exists: keep the base's \
                 callback groups and paths, and override their numbers in the mode",
                added.join(", "),
                removed.join(", ")
            ),
            ModeError::SwitchTooSlow {
                mode,
                reaction,
                budget_ms,
                tier,
                period_us,
            } => write!(
                f,
                "mode `{mode}`: its reaction path `{reaction}` allows {budget_ms} ms for the \
                 switch, but tier `{tier}` spins every {period_us} us and an executor enters a \
                 mode at its next spin. Shorten that tier's period or widen the reaction's \
                 `max_latency`"
            ),
        }
    }
}

impl std::error::Error for ModeError {}

/// The model's modes, in key order (the order a variant table is baked in).
#[must_use]
pub fn mode_names(model: &SystemModel) -> Vec<String> {
    model.contracts.modes.keys().cloned().collect()
}

fn table_name(t: &OverrideTable) -> &'static str {
    match t {
        OverrideTable::NodePaths => "node_paths",
        OverrideTable::ScopePaths => "scope_paths",
        OverrideTable::PubEndpoints => "pub_endpoints",
        OverrideTable::SubEndpoints => "sub_endpoints",
        OverrideTable::Topics => "topics",
    }
}

/// A rate value: a bare number, or one suffixed `hz` (`10`, `10hz`).
fn parse_rate(v: &str) -> Option<f64> {
    let t = v.trim();
    let t = t
        .strip_suffix("hz")
        .or_else(|| t.strip_suffix("Hz"))
        .unwrap_or(t);
    t.trim()
        .parse::<f64>()
        .ok()
        .filter(|r| r.is_finite() && *r >= 0.0)
}

fn parse_ms(v: &str) -> Option<f64> {
    v.trim()
        .parse::<Duration>()
        .ok()
        .map(Duration::as_millis_f64)
}

/// Apply `mode`'s overrides to a copy of `model`.
///
/// # Errors
///
/// [`ModeError::Unresolved`], [`ModeError::MissingEntity`] or
/// [`ModeError::BadValue`] for the first override that cannot be applied.
pub fn apply_mode(model: &SystemModel, mode: &str) -> Result<ModeVariant, ModeError> {
    let mut out = model.clone();
    let mut not_lowered = Vec::new();
    let overrides: &[ModeOverrideContract] = model
        .contracts
        .modes
        .get(mode)
        .map(|m| m.overrides.as_slice())
        .unwrap_or_default();
    for ov in overrides {
        let Some(ResolvedOverride { table, key, field }) = &ov.resolved else {
            return Err(ModeError::Unresolved {
                mode: mode.to_string(),
                target: ov.target.clone(),
            });
        };
        let missing = || ModeError::MissingEntity {
            mode: mode.to_string(),
            target: ov.target.clone(),
            table: table_name(table),
            key: key.clone(),
        };
        let bad = |expected: &'static str| ModeError::BadValue {
            mode: mode.to_string(),
            target: ov.target.clone(),
            value: ov.value.clone(),
            expected,
        };
        let rate = || parse_rate(&ov.value).ok_or_else(|| bad("a rate in Hz"));
        let ms = || parse_ms(&ov.value).ok_or_else(|| bad("a duration (`50ms`, `1s`)"));
        let mut skip = |why: &str| not_lowered.push(format!("{}: {why}", ov.target));
        let c = &mut out.contracts;
        match table {
            OverrideTable::PubEndpoints => {
                let e = c.pub_endpoints.get_mut(key).ok_or_else(missing)?;
                match field.as_str() {
                    "min_rate_hz" => e.min_rate_hz = Some(rate()?),
                    "max_rate_hz" => {
                        e.max_rate_hz = Some(rate()?);
                        skip("a publisher's rate CEILING has no on-target monitor");
                    }
                    other => skip(&format!("`{other}` has no on-target monitor")),
                }
            }
            OverrideTable::SubEndpoints => {
                let e = c.sub_endpoints.get_mut(key).ok_or_else(missing)?;
                match field.as_str() {
                    "max_age" => e.max_age_ms = Some(ms()?),
                    "min_rate_hz" => {
                        e.min_rate_hz = Some(rate()?);
                        skip("a subscriber's rate NEED is judged on its publisher's row");
                    }
                    "max_rate_hz" => {
                        e.max_rate_hz = Some(rate()?);
                        skip("a subscriber's rate ceiling has no on-target monitor");
                    }
                    other => skip(&format!("`{other}` has no on-target monitor")),
                }
            }
            OverrideTable::NodePaths => {
                let p = c.node_paths.get_mut(key).ok_or_else(missing)?;
                match field.as_str() {
                    "max_latency" => p.max_latency_ms = Some(ms()?),
                    other => skip(&format!("`{other}` has no on-target monitor")),
                }
            }
            OverrideTable::ScopePaths => {
                if !c.scope_paths.contains_key(key) {
                    return Err(missing());
                }
                skip("a scope path's budget is held at resolve time, not on target");
            }
            OverrideTable::Topics => {
                if !c.topics.contains_key(key) {
                    return Err(missing());
                }
                skip("a topic's requirement is held at resolve time, not on target");
            }
        }
    }
    Ok(ModeVariant {
        name: mode.to_string(),
        model: out,
        not_lowered,
    })
}

fn diff(base: &[String], variant: &[String]) -> (Vec<String>, Vec<String>) {
    let added = variant
        .iter()
        .filter(|k| !base.contains(k))
        .cloned()
        .collect();
    let removed = base
        .iter()
        .filter(|k| !variant.contains(k))
        .cloned()
        .collect();
    (added, removed)
}

/// The variant's monitor and age rows, refused unless they are the base's
/// rows in the base's order -- same endpoint, same topic, row for row -- so a
/// variant table can share the base table's cells.
///
/// # Errors
///
/// [`ModeError::RowSetChanged`] when the mode adds or removes a row;
/// [`ModeError::Rows`] when the variant's rows do not derive at all.
pub fn variant_rows(
    base: &SystemModel,
    variant: &ModeVariant,
) -> Result<(Vec<MonitorRow>, Vec<AgeRow>), ModeError> {
    let rows_err = |e: contract_monitors::ContractRowError| ModeError::Rows {
        mode: variant.name.clone(),
        reason: e.0,
    };
    let base_rows = contract_monitors::monitor_rows(base).map_err(rows_err)?;
    let base_ages = contract_monitors::age_rows(base).map_err(rows_err)?;
    let rows = contract_monitors::monitor_rows(&variant.model).map_err(rows_err)?;
    let ages = contract_monitors::age_rows(&variant.model).map_err(rows_err)?;
    let key_m = |r: &MonitorRow| format!("{} ({})", r.fqn, r.topic);
    let key_a = |r: &AgeRow| format!("{} ({})", r.fqn, r.topic);
    let (bm, vm): (Vec<_>, Vec<_>) = (
        base_rows.iter().map(key_m).collect(),
        rows.iter().map(key_m).collect(),
    );
    if bm != vm {
        let (added, removed) = diff(&bm, &vm);
        return Err(ModeError::RowSetChanged {
            mode: variant.name.clone(),
            table: "monitor",
            added,
            removed,
        });
    }
    let (ba, va): (Vec<_>, Vec<_>) = (
        base_ages.iter().map(key_a).collect(),
        ages.iter().map(key_a).collect(),
    );
    if ba != va {
        let (added, removed) = diff(&ba, &va);
        return Err(ModeError::RowSetChanged {
            mode: variant.name.clone(),
            table: "age",
            added,
            removed,
        });
    }
    // A row whose rate the mode removes would still be a row (latency-only),
    // but a rate of 0 means "no rate contract" to the executor -- the mode
    // would silently stop judging it. Refuse it like a removed row.
    for (b, v) in base_rows.iter().zip(&rows) {
        if b.min_rate_hz_milli != 0 && v.min_rate_hz_milli == 0 {
            return Err(ModeError::RowSetChanged {
                mode: variant.name.clone(),
                table: "monitor (rate)",
                added: Vec::new(),
                removed: vec![key_m(b)],
            });
        }
    }
    Ok((rows, ages))
}

/// One mode lowered: the applied model and its monitor and age rows.
pub type LoweredMode = (ModeVariant, Vec<MonitorRow>, Vec<AgeRow>);

/// Every mode of `model`, applied and derived: `(variant, rows, ages)` in
/// [`mode_names`] order. Empty for a model with no modes.
///
/// # Errors
///
/// The first mode that cannot be lowered.
pub fn all_variants(model: &SystemModel) -> Result<Vec<LoweredMode>, ModeError> {
    mode_names(model)
        .into_iter()
        .map(|m| {
            let v = apply_mode(model, &m)?;
            let (rows, ages) = variant_rows(model, &v)?;
            Ok((v, rows, ages))
        })
        .collect()
}

/// The mode variants of one executor's tables, rendered beside
/// [`contract_monitors::render_monitor_rs_full`]'s output (whose cells and
/// base tables they name): `NROS_MODE_MONITORS_<id>` /
/// `NROS_MODE_AGE_MONITORS_<id>` per mode, `NROS_MODE_TABLES` (entry 0 the
/// base, entry `id` mode `id`) and `nros_install_mode_tables`, which installs
/// them through `Executor::set_mode_tables`. `modes` are `(rows, ages)` in mode-id order,
/// already sliced to this executor exactly as the base was. Empty when there
/// are no modes.
///
/// phase-462 W4c -- `extras` carries what a table holds beyond its rows: each
/// mode's SchedContext deadlines (the base's first) and each mode's switch
/// budget. [`ModeExtras::default`] bakes neither.
#[must_use]
pub fn render_mode_tables_rs(
    modes: &[(&[MonitorRow], &[AgeRow])],
    extras: &ModeExtras,
    paths: contract_monitors::RenderPaths,
) -> String {
    if modes.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "// phase-462 W4 -- the contract's modes: the base rows, the base cells, the\n\
         // mode's numbers.\n",
    );
    for (k, (rows, ages)) in modes.iter().enumerate() {
        let id = k + 1;
        out.push_str(&contract_monitors::render_spec_tables(
            rows,
            ages,
            &format!("NROS_MODE_MONITORS_{id}"),
            &format!("NROS_MODE_AGE_MONITORS_{id}"),
        ));
    }
    for k in 0..=modes.len() {
        let Some(rows) = extras.deadlines.get(k).filter(|d| !d.is_empty()) else {
            continue;
        };
        out.push_str(&format!(
            "pub static NROS_MODE_DEADLINES_{k}: &[{m}::ModeDeadline] = &[\n",
            m = paths.monitor
        ));
        for (sc, us) in rows {
            out.push_str(&format!(
                "    {m}::ModeDeadline {{ sc: {sc}u8, deadline_us: {us}u32 }},\n",
                m = paths.monitor
            ));
        }
        out.push_str("];\n");
    }
    let tail = |k: usize| {
        let deadlines = if extras.deadlines.get(k).is_some_and(|d| !d.is_empty()) {
            format!("NROS_MODE_DEADLINES_{k}")
        } else {
            "&[]".to_string()
        };
        let (budget, reaction) = k
            .checked_sub(1)
            .and_then(|i| extras.switch.get(i))
            .map_or((0, ""), |(b, r)| (*b, r.as_str()));
        format!("deadlines: {deadlines}, switch_budget_ms: {budget}u32, reaction: {reaction:?}")
    };
    out.push_str(&format!(
        "pub static NROS_MODE_TABLES: &[{m}::ModeTables] = &[\n    \
         {m}::ModeTables {{ monitors: NROS_MONITORS, ages: NROS_AGE_MONITORS, {t} }},\n",
        m = paths.monitor,
        t = tail(0)
    ));
    for k in 1..=modes.len() {
        out.push_str(&format!(
            "    {m}::ModeTables {{ monitors: NROS_MODE_MONITORS_{k}, ages: \
             NROS_MODE_AGE_MONITORS_{k}, {t} }},\n",
            m = paths.monitor,
            t = tail(k)
        ));
    }
    out.push_str("];\n\n");
    out.push_str(&format!(
        "/// Install the mode variants AFTER `nros_install_monitors`; then\n\
         /// `request_mode(id)` switches this executor at its next spin.\n\
         pub fn nros_install_mode_tables(\n    executor: &mut {e}<'_>,\n) -> \
         ::core::result::Result<(), {m}::ModeTablesMismatch> {{\n    \
         executor.set_mode_tables(NROS_MODE_TABLES)\n}}\n",
        e = paths.executor,
        m = paths.monitor
    ));
    out
}

/// phase-462 W4c -- every DERIVED tier's deadline in one model, µs (0 = the
/// tier states none), keyed by tier name.
///
/// Only a derived tier's deadline is the contract's: an authored
/// `[tiers.*]` table states its own, which a mode override of a path's
/// latency does not move. So a model whose placement is authored derives
/// nothing here, and its modes bake no deadline rows.
#[must_use]
pub fn derived_tier_deadlines(
    model: &SystemModel,
    target_rtos: &str,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
) -> BTreeMap<String, TierTiming> {
    if !crate::derive::placement_is_unauthored(model, target_rtos, callback_groups) {
        return BTreeMap::new();
    }
    crate::derive::derive_tiers_from_contracts(model, target_rtos, callback_groups)
        .tiers
        .into_iter()
        .map(|(name, def)| {
            let spec = match target_rtos {
                "zephyr" => def.zephyr.as_ref(),
                "freertos" => def.freertos.as_ref(),
                "threadx" => def.threadx.as_ref(),
                "nuttx" => def.nuttx.as_ref(),
                "posix" | "native" => def.posix.as_ref(),
                _ => None,
            };
            let priority = spec.map(|p| p.priority);
            (
                name,
                TierTiming {
                    deadline_us: def.deadline_us.unwrap_or(0),
                    period_us: def.period_us.unwrap_or(0),
                    budget_us: def.budget_us.unwrap_or(0),
                    priority,
                },
            )
        })
        .collect()
}

/// phase-462 W4c -- one derived tier's timing, as a mode compares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierTiming {
    /// µs; 0 = none.
    pub deadline_us: u64,
    /// µs; 0 = none.
    pub period_us: u64,
    /// µs; 0 = none.
    pub budget_us: u64,
    /// The allocated priority on this target, when the tier has one.
    pub priority: Option<i64>,
}

/// phase-462 W4c -- each mode's derived tier deadlines, and the tiers whose
/// deadline differs in ANY mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModeTierDeadlines {
    /// The tiers a mode moves, in name order. A mode table carries one row per
    /// tier here, the base included, so switching back restores the base.
    pub tiers: Vec<String>,
    /// `deadlines[0]` is the base, `deadlines[id]` mode `id`: each tier of
    /// [`Self::tiers`] in the same order, µs (0 = none).
    pub deadlines: Vec<Vec<u64>>,
    /// What a mode changes about a tier that the executor cannot apply at a
    /// switch (priority, period, budget), as `"mode `m`: <why>"`.
    pub not_lowered: Vec<String>,
}

/// phase-462 W4c -- the per-mode deadlines of every derived tier, refusing a
/// mode whose derived tier SET differs from the base's.
///
/// The deadline is the one tier fact a switch can apply: the executor reads
/// a SchedContext's `deadline_us` when it judges a dispatch, so replacing it
/// at the next spin takes effect for the next callback. A priority, period or
/// budget a mode moves is LISTED, not applied: a priority is a thread's,
/// created once; a period or budget is a reservation armed at boot.
///
/// # Errors
///
/// [`ModeError::TierSetChanged`] for the first mode that adds or removes a
/// derived tier; any [`apply_mode`] refusal.
pub fn mode_tier_deadlines(
    model: &SystemModel,
    target_rtos: &str,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
) -> Result<ModeTierDeadlines, ModeError> {
    let names = mode_names(model);
    let base = derived_tier_deadlines(model, target_rtos, callback_groups);
    if names.is_empty() || base.is_empty() {
        return Ok(ModeTierDeadlines::default());
    }
    let mut per_mode = Vec::with_capacity(names.len());
    let mut not_lowered = Vec::new();
    for m in &names {
        let v = apply_mode(model, m)?;
        let t = derived_tier_deadlines(&v.model, target_rtos, callback_groups);
        let bk: Vec<String> = base.keys().cloned().collect();
        let vk: Vec<String> = t.keys().cloned().collect();
        if bk != vk {
            let (added, removed) = diff(&bk, &vk);
            return Err(ModeError::TierSetChanged {
                mode: m.clone(),
                added,
                removed,
            });
        }
        for (name, b) in &base {
            let x = &t[name];
            let mut note = |what: &str, from: String, to: String| {
                not_lowered.push(format!(
                    "mode `{m}`: tier `{name}` {what} {from} -> {to} is not applied at a \
                     switch (set once at boot); the mode keeps {from}"
                ));
            };
            if b.priority != x.priority {
                note(
                    "priority",
                    format!("{:?}", b.priority),
                    format!("{:?}", x.priority),
                );
            }
            if b.period_us != x.period_us {
                note(
                    "period_us",
                    b.period_us.to_string(),
                    x.period_us.to_string(),
                );
            }
            if b.budget_us != x.budget_us {
                note(
                    "budget_us",
                    b.budget_us.to_string(),
                    x.budget_us.to_string(),
                );
            }
        }
        per_mode.push(t);
    }
    let tiers: Vec<String> = base
        .iter()
        .filter(|(name, b)| {
            per_mode
                .iter()
                .any(|t| t[*name].deadline_us != b.deadline_us)
        })
        .map(|(name, _)| name.clone())
        .collect();
    let row = |t: &BTreeMap<String, TierTiming>| -> Vec<u64> {
        tiers.iter().map(|n| t[n].deadline_us).collect()
    };
    let mut deadlines = vec![row(&base)];
    deadlines.extend(per_mode.iter().map(row));
    Ok(ModeTierDeadlines {
        tiers,
        deadlines,
        not_lowered,
    })
}

/// phase-462 W4c -- the latency a mode's switch is allowed: its reaction
/// path's `max_latency`, in ms, rounded up. `None` when the mode names no
/// reaction or the path states no latency.
#[must_use]
pub fn mode_switch_budget_ms(model: &SystemModel, mode: &str) -> Option<(String, u32)> {
    let m = model.contracts.modes.get(mode)?;
    let reaction = m.reaction.as_ref()?;
    let ms = model
        .contracts
        .scope_paths
        .get(reaction)
        .or_else(|| model.contracts.node_paths.get(reaction))?
        .max_latency_ms?;
    Some((
        reaction.clone(),
        ms.ceil().clamp(1.0, f64::from(u32::MAX)) as u32,
    ))
}

/// phase-462 W4c -- refuse a mode whose switch cannot meet its budget.
///
/// An executor enters a requested mode at its NEXT spin, so the switch's
/// on-target latency is bounded by the longest spin period among the
/// executors that switch. `periods` is each tier's `(name, period_us)`, 0 for
/// a tier that states none. A tier with no period has no bound, so a mode
/// with a budget is then NOT CHECKED and says so in the returned notes rather
/// than passing; the run-time rule still judges it.
///
/// # Errors
///
/// [`ModeError::SwitchTooSlow`] for the first mode the slowest tier misses.
pub fn check_switch_latency(
    model: &SystemModel,
    periods: &[(String, u64)],
) -> Result<Vec<String>, ModeError> {
    let mut notes = Vec::new();
    for m in mode_names(model) {
        let Some((reaction, budget_ms)) = mode_switch_budget_ms(model, &m) else {
            continue;
        };
        if let Some((tier, _)) = periods.iter().find(|(_, p)| *p == 0) {
            notes.push(format!(
                "mode `{m}`: switch latency not checked at build time -- tier `{tier}` states \
                 no period, so its next spin is unbounded"
            ));
            continue;
        }
        if let Some((tier, period_us)) = periods.iter().max_by_key(|(_, p)| *p)
            && *period_us > u64::from(budget_ms) * 1_000
        {
            return Err(ModeError::SwitchTooSlow {
                mode: m,
                reaction,
                budget_ms,
                tier: tier.clone(),
                period_us: *period_us,
            });
        }
    }
    Ok(notes)
}

/// phase-462 W4c -- what one executor's mode tables carry beyond the rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModeExtras {
    /// `(sched context id, deadline_us)` rows: index 0 the base, `k` mode `k`,
    /// every entry naming the same contexts in the same order. Empty when no
    /// mode moves a deadline on this executor.
    pub deadlines: Vec<Vec<(u8, u32)>>,
    /// `(switch_budget_ms, reaction path)` per mode, index `k - 1` for mode
    /// `k` ([`mode_switch_rows`]).
    pub switch: Vec<(u32, String)>,
}

/// phase-462 W4c -- each mode's switch budget and reaction path, in
/// [`mode_names`] order; `(0, "")` for a mode that states none.
#[must_use]
pub fn mode_switch_rows(model: &SystemModel) -> Vec<(u32, String)> {
    mode_names(model)
        .iter()
        .map(|m| mode_switch_budget_ms(model, m).map_or((0, String::new()), |(r, b)| (b, r)))
        .collect()
}

/// phase-462 W4c -- [`ModeTierDeadlines`] as one executor's deadline rows:
/// `sc_of(tier)` names the context a tier runs under on this executor, or
/// `None` when the tier is not this executor's (its rows are skipped).
#[must_use]
pub fn deadline_rows(
    d: &ModeTierDeadlines,
    sc_of: impl Fn(&str) -> Option<u8>,
) -> Vec<Vec<(u8, u32)>> {
    let mine: Vec<(usize, u8)> = d
        .tiers
        .iter()
        .enumerate()
        .filter_map(|(i, t)| sc_of(t).map(|sc| (i, sc)))
        .collect();
    if mine.is_empty() {
        return Vec::new();
    }
    d.deadlines
        .iter()
        .map(|per| {
            mine.iter()
                .map(|(i, sc)| (*sc, per[*i].min(u64::from(u32::MAX)) as u32))
                .collect()
        })
        .collect()
}

/// The baked id of each mode, in [`mode_names`] order: `(name, id)`, ids
/// from 1 (0 is the base contract). One numbering for every executor and
/// every road, so `request_mode(id)` means one mode wherever it is asked.
#[must_use]
pub fn mode_ids(model: &SystemModel) -> Vec<(String, u16)> {
    mode_names(model)
        .into_iter()
        .enumerate()
        .map(|(i, n)| (n, (i + 1) as u16))
        .collect()
}

/// A mode name as a constant's suffix: `degraded-safe` -> `DEGRADED_SAFE`.
#[must_use]
pub fn mode_const_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// phase-462 W4c -- two timer nodes whose callback groups name no tier,
    /// so the contract derives one tier each; `modes` lands under
    /// `contracts.modes`, `extra` under `contracts:`.
    fn tiered(modes: &str, extra: &str) -> SystemModel {
        let yaml = format!(
            r#"
meta:
  version: 1
structure:
  nodes:
    /control_node:
      scope: /
    /telem_node:
      scope: /
  topics:
    /cmd:
      type: std_msgs/Int32
      pub: [/control_node/cmd]
    /status:
      type: std_msgs/Int32
      pub: [/telem_node/status]
contracts:
  pub_endpoints:
    /control_node/cmd: {{}}
    /telem_node/status: {{}}
  node_paths:
    /control_node/loop:
      output: [/control_node/cmd]
      trigger: {{ kind: timer, value: {{ rate_hz: 100.0 }} }}
      max_latency_ms: 5.0
    /telem_node/loop:
      output: [/telem_node/status]
      trigger: {{ kind: timer, value: {{ rate_hz: 10.0 }} }}
      max_latency_ms: 100.0
{extra}
  modes:
{modes}
"#
        );
        SystemModel::from_yaml_str(&yaml).expect("tiered fixture")
    }

    fn groups() -> BTreeMap<String, Vec<CallbackGroupDecl>> {
        let g = |id: &str| {
            vec![CallbackGroupDecl {
                id: id.to_string(),
                r#type: "MutuallyExclusive".to_string(),
                tier: id.to_string(),
            }]
        };
        BTreeMap::from([
            ("control_node".to_string(), g("ctrl")),
            ("telem_node".to_string(), g("telem")),
        ])
    }

    fn latency(key: &str, value: &str) -> String {
        ov(
            "nodes.x.paths.loop.max_latency",
            value,
            "node_paths",
            key,
            "max_latency",
        )
    }

    /// A mode that tightens one path's latency moves that tier's derived
    /// deadline, and only that tier carries rows -- the base's value first,
    /// so switching back restores it.
    #[test]
    fn a_mode_moves_the_derived_deadline_of_the_tier_it_tightens() {
        let m = tiered(
            &format!(
                "    tight:\n      overrides:\n{}",
                latency("/control_node/loop", "2ms")
            ),
            "",
        );
        let d = mode_tier_deadlines(&m, "posix", &groups()).expect("lowers");
        assert_eq!(d.tiers, ["derived-control_node"]);
        assert_eq!(d.deadlines, [vec![5_000], vec![2_000]]);
    }

    /// The negative controls: a model with no modes, and a mode that moves no
    /// deadline, bake no deadline rows; an authored placement derives none.
    #[test]
    fn no_mode_or_no_moved_deadline_means_no_rows() {
        let none = tiered("    {}", "");
        assert_eq!(
            mode_tier_deadlines(&none, "posix", &groups()).unwrap(),
            ModeTierDeadlines::default()
        );
        let rate_only = tiered(
            &format!(
                "    quiet:\n      overrides:\n{}",
                ov(
                    "nodes.x.pub.cmd.min_rate_hz",
                    "5",
                    "pub_endpoints",
                    "/control_node/cmd",
                    "min_rate_hz"
                )
            ),
            "",
        );
        let d = mode_tier_deadlines(&rate_only, "posix", &groups()).unwrap();
        assert!(d.tiers.is_empty(), "{d:?}");
    }

    /// The rendered tables carry each mode's deadline rows -- the base's in
    /// table 0, so a switch back restores it -- and each mode's budget.
    #[test]
    fn the_variant_tables_render_deadlines_and_the_switch_budget() {
        let empty: (&[MonitorRow], &[AgeRow]) = (&[], &[]);
        let extras = ModeExtras {
            deadlines: vec![vec![(0, 5_000)], vec![(0, 2_000)]],
            switch: vec![(50, "//enter_safe".to_string())],
        };
        let text =
            render_mode_tables_rs(&[empty], &extras, contract_monitors::RenderPaths::NROS_NODE);
        for want in [
            "pub static NROS_MODE_DEADLINES_0: &[::nros_node::executor::monitor::ModeDeadline] = &[",
            "ModeDeadline { sc: 0u8, deadline_us: 5000u32 }",
            "ModeDeadline { sc: 0u8, deadline_us: 2000u32 }",
            "deadlines: NROS_MODE_DEADLINES_0, switch_budget_ms: 0u32, reaction: \"\" }",
            "deadlines: NROS_MODE_DEADLINES_1, switch_budget_ms: 50u32, reaction: \"//enter_safe\" }",
        ] {
            assert!(text.contains(want), "missing `{want}` in:\n{text}");
        }
        let d = ModeTierDeadlines {
            tiers: vec!["a".into(), "b".into()],
            deadlines: vec![vec![1, 2], vec![3, 4]],
            not_lowered: Vec::new(),
        };
        assert_eq!(
            deadline_rows(&d, |t| (t == "b").then_some(7)),
            [vec![(7, 2)], vec![(7, 4)]],
            "only this executor's tiers"
        );
        assert!(deadline_rows(&d, |_| None).is_empty());
    }

    /// A switch budget below the slowest tier's period refuses; at or above
    /// it passes; a tier with no period is stated as not checked.
    #[test]
    fn a_switch_slower_than_its_reaction_budget_refuses() {
        let reaction = "  scope_paths:\n    //enter_safe:\n      input: [/cmd]\n      \
                        output: [/cmd]\n      max_latency_ms: 50.0\n";
        let m = tiered(
            &format!(
                "    safe:\n      reaction: //enter_safe\n      overrides:\n{}",
                latency("/control_node/loop", "2ms")
            ),
            reaction,
        );
        assert_eq!(
            mode_switch_budget_ms(&m, "safe"),
            Some(("//enter_safe".to_string(), 50))
        );
        let slow = [("ctrl".to_string(), 10_000), ("telem".to_string(), 100_000)];
        let e = check_switch_latency(&m, &slow).unwrap_err();
        assert!(
            matches!(&e, ModeError::SwitchTooSlow { tier, period_us: 100_000, .. } if tier == "telem"),
            "{e}"
        );
        let fast = [("ctrl".to_string(), 10_000), ("telem".to_string(), 50_000)];
        assert!(check_switch_latency(&m, &fast).unwrap().is_empty());
        let unbounded = [("ctrl".to_string(), 10_000), ("idle".to_string(), 0)];
        let notes = check_switch_latency(&m, &unbounded).unwrap();
        assert!(notes[0].contains("not checked"), "{notes:?}");
    }

    /// One talker publishing `/cmd` at 10 Hz from a 20 ms node path, one
    /// listener taking it with a 200 ms age, and the modes under test.
    fn model(modes: &str) -> SystemModel {
        let yaml = format!(
            r#"
meta:
  version: 1
structure:
  nodes:
    /talker:
      scope: bringup.launch.xml
      package: p
      executable: talker
    /listener:
      scope: bringup.launch.xml
      package: p
      executable: listener
  topics:
    /cmd:
      type: std_msgs/msg/Int32
      pub: [/talker/cmd]
      sub: [/listener/cmd]
contracts:
  pub_endpoints:
    /talker/cmd:
      min_rate_hz: 10.0
  sub_endpoints:
    /listener/cmd:
      max_age_ms: 200.0
  node_paths:
    /talker/tick:
      output: [/talker/cmd]
      max_latency_ms: 20.0
  scope_paths:
    /bringup.launch.xml/e2e:
      input: [/cmd]
      output: [/cmd]
      max_latency_ms: 80.0
  modes:
{modes}
"#
        );
        SystemModel::from_yaml_str(&yaml).expect("fixture model")
    }

    fn ov(target: &str, value: &str, table: &str, key: &str, field: &str) -> String {
        format!(
            "      - target: {target}\n        value: '{value}'\n        resolved: \
             {{ table: {table}, key: '{key}', field: {field} }}\n"
        )
    }

    #[test]
    fn a_mode_lowers_to_the_base_rows_with_its_numbers() {
        let modes = format!(
            "    degraded:\n      overrides:\n{}{}{}{}",
            ov(
                "nodes.talker.pub.cmd.min_rate_hz",
                "5",
                "pub_endpoints",
                "/talker/cmd",
                "min_rate_hz"
            ),
            ov(
                "nodes.talker.paths.tick.max_latency",
                "50ms",
                "node_paths",
                "/talker/tick",
                "max_latency"
            ),
            ov(
                "nodes.listener.sub.cmd.max_age",
                "1s",
                "sub_endpoints",
                "/listener/cmd",
                "max_age"
            ),
            ov(
                "paths.e2e.max_latency",
                "200ms",
                "scope_paths",
                "/bringup.launch.xml/e2e",
                "max_latency"
            ),
        );
        let m = model(&modes);
        let all = all_variants(&m).expect("lowers");
        assert_eq!(all.len(), 1);
        let (v, rows, ages) = &all[0];
        assert_eq!(v.name, "degraded");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fqn, "/talker/cmd");
        assert_eq!(rows[0].min_rate_hz_milli, 5_000);
        assert_eq!(rows[0].max_latency_ms, 50);
        assert_eq!(ages.len(), 1);
        assert_eq!(ages[0].max_age_ms, 1_000);
        assert_eq!(
            v.not_lowered,
            ["paths.e2e.max_latency: a scope path's budget is held at resolve time, not on target"]
        );
        // The base is untouched.
        let base = contract_monitors::monitor_rows(&m).unwrap();
        assert_eq!(base[0].min_rate_hz_milli, 10_000);
        assert_eq!(base[0].max_latency_ms, 20);
    }

    #[test]
    fn a_mode_cannot_change_which_endpoints_are_monitored() {
        // A mode naming an entity the base does not declare:
        let modes = format!(
            "    m:\n      overrides:\n{}",
            ov(
                "nodes.ghost.pub.x.min_rate_hz",
                "5",
                "pub_endpoints",
                "/ghost/x",
                "min_rate_hz"
            ),
        );
        let err = all_variants(&model(&modes)).unwrap_err();
        assert!(
            matches!(&err, ModeError::MissingEntity { key, .. } if key == "/ghost/x"),
            "{err}"
        );
        // A rate of 0 would silently stop judging the row.
        let modes = format!(
            "    m:\n      overrides:\n{}",
            ov(
                "nodes.talker.pub.cmd.min_rate_hz",
                "0",
                "pub_endpoints",
                "/talker/cmd",
                "min_rate_hz"
            ),
        );
        let err = all_variants(&model(&modes)).unwrap_err();
        assert!(matches!(err, ModeError::RowSetChanged { .. }), "{err}");
    }

    #[test]
    fn an_unresolved_or_unreadable_override_refuses() {
        let modes = "    m:\n      overrides:\n      - target: nodes.talker.pub.cmd.min_rate_hz\n        value: '5'\n";
        let err = all_variants(&model(modes)).unwrap_err();
        assert!(matches!(err, ModeError::Unresolved { .. }), "{err}");
        let modes = format!(
            "    m:\n      overrides:\n{}",
            ov(
                "nodes.talker.paths.tick.max_latency",
                "soon",
                "node_paths",
                "/talker/tick",
                "max_latency"
            ),
        );
        let err = all_variants(&model(&modes)).unwrap_err();
        assert!(matches!(err, ModeError::BadValue { .. }), "{err}");
        assert!(err.to_string().contains("a duration"), "{err}");
    }

    /// The rendered variants name the base's cells and tables, and entry 0 of
    /// `NROS_MODE_TABLES` is the base. `nros-node`'s `monitor::tests::
    /// rendered_modes` compiles this text against the real types.
    #[test]
    fn the_variant_tables_render_over_the_base_cells() {
        let modes = format!(
            "    degraded:\n      overrides:\n{}",
            ov(
                "nodes.talker.pub.cmd.min_rate_hz",
                "5",
                "pub_endpoints",
                "/talker/cmd",
                "min_rate_hz"
            ),
        );
        let all = all_variants(&model(&modes)).unwrap();
        let slices: Vec<(&[MonitorRow], &[AgeRow])> = all
            .iter()
            .map(|(_, r, a)| (r.as_slice(), a.as_slice()))
            .collect();
        let text = render_mode_tables_rs(
            &slices,
            &ModeExtras::default(),
            contract_monitors::RenderPaths::NROS_NODE,
        );
        assert!(
            text.contains("pub static NROS_MODE_MONITORS_1: &[MonitorSpec] = &["),
            "{text}"
        );
        assert!(text.contains("min_rate_hz_milli: 5000u32"), "{text}");
        assert!(text.contains("cell: &NROS_MONITOR_CELL_0"), "{text}");
        assert!(
            text.contains(
                "ModeTables { monitors: NROS_MONITORS, ages: NROS_AGE_MONITORS, deadlines: &[], \
                 switch_budget_ms: 0u32, reaction: \"\" }"
            ),
            "{text}"
        );
        assert!(
            text.contains("monitors: NROS_MODE_MONITORS_1, ages: NROS_MODE_AGE_MONITORS_1, "),
            "{text}"
        );
        assert!(
            render_mode_tables_rs(
                &[],
                &ModeExtras::default(),
                contract_monitors::RenderPaths::NROS_NODE
            )
            .is_empty()
        );
        assert_eq!(mode_ids(&model(&modes)), [("degraded".to_string(), 1)]);
        assert_eq!(mode_const_name("degraded-safe"), "DEGRADED_SAFE");
        assert_eq!(mode_const_name("2x"), "_2X");
    }

    #[test]
    fn a_model_with_no_modes_has_no_variants() {
        let m = model("    {}");
        assert!(all_variants(&m).unwrap().is_empty());
    }
}
