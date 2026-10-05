//! Issue 1676 — the contract-monitor rows a resolved `SystemModel` bakes, in
//! ONE place both producers read.
//!
//! RFC-0052 / phase-296 W3b — a contracted publisher (`min_rate_hz`, a node
//! path's `max_latency_ms`) and a contracted subscriber (`max_age_ms`) each
//! become one row of the executor's monitor tables. The CLI's C and C++ entry
//! emitters bake these rows; the `nros::main!` proc-macro bakes the same rows
//! into the Rust entry. They lived in `nros-cli-core`, which the proc-macro
//! cannot depend on, so the Rust road had no rows at all and its
//! `system_monitors.rs` was baked and never installed. Here, the two roads
//! cannot derive two different tables.

use ros_launch_manifest_model::SystemModel;

/// A model a contract row cannot be derived from — a contracted endpoint the
/// wiring does not place on any topic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ContractRowError(pub String);

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
pub fn monitor_rows(model: &SystemModel) -> Result<Vec<MonitorRow>, ContractRowError> {
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
    // phase-82 follow-up: a path may name a service CLIENT in its output
    // (the reaction walk follows that edge across the service). A call is
    // not a publisher: it has no rate promise and no owning topic, so it
    // gets no monitor row rather than tripping the owning-topic check below.
    let service_clients: std::collections::BTreeSet<&String> = model
        .structure
        .services
        .values()
        .flat_map(|s| s.client.iter())
        .collect();
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
            return Err(ContractRowError(format!(
                "SystemModel: node path '{path_ref}' declares max_latency_ms but \
                 lists no output endpoint — inconsistent model"
            )));
        }
        for out in &p.output {
            if service_clients.contains(out) {
                continue;
            }
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
            return Err(ContractRowError(format!(
                "SystemModel: contracted publisher '{ep_ref}' has no \
                 owning topic in structure.topics — inconsistent model"
            )));
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
pub fn age_rows(model: &SystemModel) -> Result<Vec<AgeRow>, ContractRowError> {
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
            return Err(ContractRowError(format!(
                "SystemModel: contracted subscriber '{ep_ref}' (max_age_ms) has no \
                 owning topic in structure.topics — inconsistent model"
            )));
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
    render_monitor_rs_at(rows, ages, RenderPaths::NROS_NODE)
}

/// [`render_monitor_rs`] with the type paths stated — issue 1676.
pub fn render_monitor_rs_at(rows: &[MonitorRow], ages: &[AgeRow], paths: RenderPaths) -> String {
    let mut out = String::new();
    out.push_str(
        "// GENERATED by `nros codegen-system --model` (RFC-0052 W3b.4/.5 / phase-296 N1).\n\
         // One PubMonitorCell per contracted publisher (+ one SubMonitorCell per age\n\
         // contract) + the executor monitor tables. Include from the entry; call\n\
         // `nros_install_monitors(&mut executor)` BEFORE entity creation (node-side\n\
         // attachment is auto-seeded from the executor at create_node).\n",
    );
    out.push_str(&format!(
        "use {m}::{{AgeMonitorSpec, MonitorSpec, PubMonitorCell, SubMonitorCell}};\n\n",
        m = paths.monitor
    ));
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
    // phase-467 W1 (issue 1471) -- the executor watches at most
    // `MAX_MONITORS` / `MAX_AGE_MONITORS` rows, and the spin loop inspects only
    // that many. A table past either is a COMPILE error here that names the
    // knob, never a table that boots with its tail unwatched. Both knobs derive
    // from these same row counts, so this fires only when a stated value is
    // below the contract. An empty table cannot overflow and gets no assert
    // (`0 <= N` is clippy's deny-level `absurd_extreme_comparisons`).
    for (n, bound, knob, what) in [
        (
            rows.len(),
            "MAX_MONITORS",
            "NROS_EXECUTOR_MAX_MONITORS",
            "rate/latency",
        ),
        (
            ages.len(),
            "MAX_AGE_MONITORS",
            "NROS_EXECUTOR_MAX_AGE_MONITORS",
            "age",
        ),
    ] {
        if n == 0 {
            continue;
        }
        out.push_str(&format!(
            "const _: () = assert!(\n    {n} <= {m}::{bound},\n    \
             \"this contract bakes {n} {what} monitor rows, more than the executor watches: \
             raise {knob} (it derives from the contract when nothing states it)\"\n);\n",
            m = paths.monitor
        ));
    }
    out.push('\n');
    out.push_str(
        &format!(
            "pub fn nros_install_monitors(executor: &mut {e}<'_>) {{\n    executor.set_monitor_table(NROS_MONITORS);\n    executor.set_age_table(NROS_AGE_MONITORS);\n}}\n",
            e = paths.executor
        ),
    );
    out
}

/// Where the rendered Rust names the monitor types and the executor.
///
/// The CLI bake writes a file the entry `include!`s with `nros-node` in scope
/// ([`RenderPaths::NROS_NODE`]); the `nros::main!` expansion reaches only the
/// `nros` umbrella ([`RenderPaths::NROS`]). The text is otherwise identical,
/// which is the point: one renderer, two roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPaths {
    /// Path of the module holding `MonitorSpec`, `AgeMonitorSpec`, the two
    /// cells and `MAX_MONITORS` / `MAX_AGE_MONITORS`.
    pub monitor: &'static str,
    /// Path of the `Executor` type.
    pub executor: &'static str,
}

impl RenderPaths {
    /// `nros-node`'s own paths — the baked `system_monitors.rs`.
    pub const NROS_NODE: Self = Self {
        monitor: "::nros_node::executor::monitor",
        executor: "::nros_node::executor::Executor",
    };
    /// The `nros` umbrella's re-exports — the `nros::main!` expansion.
    pub const NROS: Self = Self {
        monitor: "::nros::monitor",
        executor: "::nros::Executor",
    };
}

/// The endpoint ref's node half (`/ns/node/endpoint` -> `/ns/node`), the key a
/// per-executor slice keeps a row by. ONE spelling — the C/C++ lowering and the
/// `nros::main!` tier slice both ask it.
#[must_use]
pub fn row_node_fqn(endpoint_ref: &str) -> &str {
    endpoint_ref
        .rsplit_once('/')
        .map(|(node, _)| node)
        .unwrap_or("")
}
