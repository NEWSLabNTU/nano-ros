//! phase-463 W3 -- the census check: every delta between the contract and the
//! code the census saw, as a named verdict.
//!
//! # The three documents, and what each one is
//!
//! | document | what it states | who wrote it |
//! | --- | --- | --- |
//! | the CENSUS (`build/nros/census/<entry>.json`) | what the code CREATED, observed by running the entry's own native binary (phase-463 W2) | the machine |
//! | the CONTRACT (`<bringup>/launch/<stem>.contract.yaml`) | what the image is DECLARED to create | a person |
//! | the INVENTORY (`nros_entity_inventory.json`) | what the build DERIVED from the contract, and sized every pool from | the machine, from the person's document |
//!
//! Each of the three has had a reader for a while and no two of them were ever
//! compared. That is the whole defect phase-463 exists for: on the safety
//! island, deleting one `sub:` row from the contract produced zero diagnostics
//! on the host and `ExecutorFull` at boot on a board with no wired console.
//!
//! This module is the JOIN. It emits one [`Row`] per `(node, kind, name)` and
//! refuses when any row's severity is `error`.
//!
//! # The join key, and why it is the RESOLVED name
//!
//! The two sides spell an endpoint differently and neither spelling is the
//! other's:
//!
//! * the census records the topic AS THE CODE PASSED IT, plus the node's
//!   namespace -- so `/api/operation_mode/state`, resolved here from the
//!   written name and the node's namespace exactly as the runtime resolves it;
//! * the contract names a per-node ENDPOINT ALIAS (`operation_mode_state`) and
//!   wires it to a topic in the `topics:` table
//!   (`sub: [mrm_handler/operation_mode_state]`).
//!
//! So the key both sides can state is the resolved topic (or service) name,
//! and the alias travels beside it because the alias is what a person edits.
//! An endpoint the contract declares under a node but wires to no topic has no
//! resolved name at all -- that is the `unwired` verdict, and it is exactly
//! the island's E3c, where the resolver drops the endpoint in silence.
//!
//! # The two directions have different severities, and only one is waivable
//!
//! * the UNDER direction (the code creates more than the contract declares) is
//!   an error with NO waiver. Every pool derives one short and the first catch
//!   is `ExecutorFull` on a board with no console.
//! * the OVER direction (the contract declares more than the code creates) is
//!   an error by default -- a contract is a statement about the code and a
//!   false one is a defect -- but it is waivable per row with a reason, in
//!   `system.toml` under `[census.waive]`, never in the contract (whose parser
//!   rejects unknown keys and whose schema is rlm's).
//!
//! `unwired` is in the OVER direction and is NOT waivable, which is what the
//! phase doc's verdict table says and this module implements: a waiver is a
//! person taking responsibility for a true statement that this check cannot
//! confirm, and an unwired row is not a statement that could be true -- it
//! names a topic the contract itself never declares, so there is nothing for a
//! waiver to stand behind.

use std::collections::{BTreeMap, BTreeSet};

use ros_launch_manifest_types::{Manifest, ParamType, QosDecl};
use serde_json::Value;

/// What an entity row is. A closed set: an unrecognised census array is a
/// REFUSAL to read the census, never a kind this module skips, because a
/// skipped kind is exactly an under-report and under-reporting is the defect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Sub,
    Pub,
    Srv,
    Cli,
    ActionServer,
    ActionClient,
    Timer,
    Param,
}

impl Kind {
    /// The spelling a verdict row and a waiver key use. The `sub:` / `pub:` /
    /// `srv:` / `cli:` four are the CONTRACT's own keys, so a person reading a
    /// refusal reads the word they would type.
    pub fn tag(self) -> &'static str {
        match self {
            Kind::Sub => "sub",
            Kind::Pub => "pub",
            Kind::Srv => "srv",
            Kind::Cli => "cli",
            Kind::ActionServer => "action_server",
            Kind::ActionClient => "action_client",
            Kind::Timer => "timer",
            Kind::Param => "param",
        }
    }

    /// The word for what the code made, used in the prose of a refusal.
    fn noun(self) -> &'static str {
        match self {
            Kind::Sub => "subscription",
            Kind::Pub => "publisher",
            Kind::Srv => "service server",
            Kind::Cli => "service client",
            Kind::ActionServer => "action server",
            Kind::ActionClient => "action client",
            Kind::Timer => "timer",
            Kind::Param => "parameter",
        }
    }
}

/// How bad a row is. `None` is a `confirmed` row, which is reported and never
/// refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    None,
    Warning,
    Error,
}

impl Severity {
    pub fn tag(self) -> &'static str {
        match self {
            Severity::None => "ok",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// The verdict vocabulary, as phase-463 W3 fixes it.
///
/// Extended from play_launch phase 77's with the two directions the island
/// showed matter: an endpoint the CODE has and the contract does not
/// ([`Verdict::MissingInContract`]), and one the contract has and the code
/// does not ([`Verdict::Phantom`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    /// The code created it, the contract declares it, every fact agrees.
    Confirmed,
    /// The code created it; no contract row names it (island E3a).
    MissingInContract,
    /// The contract declares and wires it; the code never created it (E3b).
    Phantom,
    /// Declared under a node but absent from `topics:` / `services:` (E3c).
    Unwired,
    /// The code's history depth differs from the declared one (E2a).
    DepthMismatch,
    /// Reliability, durability or history differ.
    QosMismatch,
    /// The code's message type differs from the contract topic's (E4).
    TypeMismatch,
    /// A timer's launched period disagrees with the declared trigger rate.
    PeriodMismatch,
    /// The code declares a parameter the node's `params:` does not.
    ParamMissingInContract,
    /// `params:` names a parameter the code never declares.
    ParamPhantom,
    /// Name agrees, type differs.
    ParamTypeMismatch,
    /// The contract declares it and the census never ran this node.
    Unobserved,
}

impl Verdict {
    pub fn tag(self) -> &'static str {
        match self {
            Verdict::Confirmed => "confirmed",
            Verdict::MissingInContract => "missing-in-contract",
            Verdict::Phantom => "phantom",
            Verdict::Unwired => "unwired",
            Verdict::DepthMismatch => "depth-mismatch",
            Verdict::QosMismatch => "qos-mismatch",
            Verdict::TypeMismatch => "type-mismatch",
            Verdict::PeriodMismatch => "period-mismatch",
            Verdict::ParamMissingInContract => "param-missing-in-contract",
            Verdict::ParamPhantom => "param-phantom",
            Verdict::ParamTypeMismatch => "param-type-mismatch",
            Verdict::Unobserved => "unobserved",
        }
    }

    /// The severity before `--strict` and before any waiver.
    pub fn severity(self) -> Severity {
        match self {
            Verdict::Confirmed => Severity::None,
            Verdict::Unobserved => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// Can a person take responsibility for this row in `[census.waive]`?
    ///
    /// Only the two OVER-direction verdicts that are statements which could be
    /// true of a code path this census did not take: a `phantom` endpoint and
    /// a `param-phantom`. Everything else is either the UNDER direction (an
    /// image that dies at boot -- no waiver) or a disagreement about a FACT
    /// (a depth, a type, a period: a waiver would be a person asserting two
    /// different numbers are one number).
    pub fn waivable(self) -> bool {
        matches!(self, Verdict::Phantom | Verdict::ParamPhantom)
    }

    /// Why the severity is what it is. Printed with the refusal, because a
    /// person who has just been refused is the person who needs the argument.
    fn why(self) -> &'static str {
        match self {
            Verdict::Confirmed => "",
            Verdict::MissingInContract => {
                "every pool derives one short; the first catch is ExecutorFull on a board with \
                 no console"
            }
            Verdict::Phantom => {
                "safe for memory, but a false statement about the code, and it feeds the rate \
                 hierarchy and the causal checks"
            }
            Verdict::Unwired => {
                "the resolver drops it silently; the model and the inventory never see it"
            }
            Verdict::DepthMismatch => {
                "the arena is sized from the declaration (phase-403 step 2), in both directions"
            }
            Verdict::QosMismatch => {
                "RFC-0100 W3: a keep_all or transient-local endpoint prices differently"
            }
            Verdict::TypeMismatch => "the bound inventory priced the wrong type",
            Verdict::PeriodMismatch => {
                "the tier derivation and the rate hierarchy read the declared rate"
            }
            Verdict::ParamMissingInContract => {
                "the store has no slot counted for it; the phase-446 refusal, one build earlier"
            }
            Verdict::ParamPhantom => "a launch value that can reach nothing",
            Verdict::ParamTypeMismatch => {
                "play_launch rejects the launch value; the store's reader rejects the write"
            }
            Verdict::Unobserved => {
                "evidence absent is not evidence of absence (RFC-0078's rule, applied to \
                 structure)"
            }
        }
    }
}

/// One verdict, for one `(node, kind, name)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The node as the contract names it (the launch-authoritative identity,
    /// RFC-0046, spelled the way `nodes.<n>` spells it).
    pub node: String,
    pub kind: Kind,
    /// The resolved topic or service name; the parameter name for a `param`
    /// row; the endpoint alias for an `unwired` row, which HAS no resolved
    /// name -- that is what makes it unwired.
    pub name: String,
    pub verdict: Verdict,
    /// What disagrees, in one line.
    pub detail: String,
    /// The file that should change and the line to add or remove.
    pub remedy: String,
    /// The reason from `[census.waive]`, when this row was waived.
    pub waiver: Option<String>,
}

impl Row {
    /// The key a `[census.waive]` entry names. Printed with every waivable
    /// refusal so the entry can be copied rather than reconstructed.
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.node, self.kind.tag(), self.name)
    }

    /// The severity after the waiver and after `--strict`.
    pub fn severity(&self, strict: bool) -> Severity {
        if self.waiver.is_some() {
            return Severity::None;
        }
        match self.verdict.severity() {
            Severity::Warning if strict => Severity::Error,
            other => other,
        }
    }
}

/// Everything the check produced.
#[derive(Debug, Clone)]
pub struct Report {
    pub rows: Vec<Row>,
    pub strict: bool,
    /// The contract's path, for the remedy lines.
    pub contract_path: String,
    /// Notes that are not verdicts: facts about what this check did NOT
    /// compare, so an absence reads as a decision rather than as agreement.
    pub notes: Vec<String>,
}

impl Report {
    pub fn confirmed(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.verdict == Verdict::Confirmed)
            .count()
    }

    pub fn errors(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.severity(self.strict) == Severity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.severity(self.strict) == Severity::Warning)
            .count()
    }

    pub fn waived(&self) -> usize {
        self.rows.iter().filter(|r| r.waiver.is_some()).count()
    }

    /// Does this report refuse the build?
    pub fn refuses(&self) -> bool {
        self.errors() > 0
    }

    /// Every row of a given verdict, for a caller that wants one.
    pub fn of(&self, verdict: Verdict) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(move |r| r.verdict == verdict)
    }

    /// The whole report as a person reads it: the confirmed count, then one
    /// stanza per row that is not confirmed.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for row in &self.rows {
            if row.verdict == Verdict::Confirmed {
                continue;
            }
            let severity = row.severity(self.strict);
            out.push_str(&format!(
                "{} {} {} {} {}\n",
                severity.tag(),
                row.verdict.tag(),
                row.node,
                row.kind.tag(),
                row.name
            ));
            out.push_str(&format!("    {}\n", row.detail));
            for line in row.remedy.lines() {
                out.push_str(&format!("    {line}\n"));
            }
            match &row.waiver {
                Some(reason) => out.push_str(&format!(
                    "    waived in system.toml [census.waive.\"{}\"]: {}\n",
                    row.key(),
                    reason
                )),
                None if row.verdict.waivable() => out.push_str(&format!(
                    "    waivable: {}\n    add to system.toml:\n        \
                     [census.waive.\"{}\"]\n        reason = \"...\"\n",
                    row.verdict.why(),
                    row.key()
                )),
                None => out.push_str(&format!("    not waivable: {}\n", row.verdict.why())),
            }
        }
        for note in &self.notes {
            out.push_str(&format!("note: {note}\n"));
        }
        out.push_str(&format!(
            "census check: {} confirmed, {} error(s), {} warning(s), {} waived\n",
            self.confirmed(),
            self.errors(),
            self.warnings(),
            self.waived()
        ));
        out
    }
}

/// The three documents plus the waivers, as one argument.
pub struct Inputs<'a> {
    /// Either the wrapper `nros ws entity-census run` writes or the recorder
    /// document inside it; [`check`] accepts both.
    pub census: &'a Value,
    pub contract: &'a Manifest,
    pub contract_path: String,
    /// `nros_entity_inventory.json`, when the build wrote one. Optional
    /// because the census and the contract are what a delta is BETWEEN; the
    /// inventory is the derived third view, and its absence costs
    /// corroboration rather than correctness.
    pub inventory: Option<&'a Value>,
    /// `[census.waive]` from `system.toml`, keyed by [`Row::key`].
    pub waivers: &'a BTreeMap<String, String>,
    /// Turn warnings into errors (the merge queue's setting).
    pub strict: bool,
}

// ---------------------------------------------------------------------------
// The census side
// ---------------------------------------------------------------------------

/// One endpoint the code created, as the census recorded it.
#[derive(Debug, Clone)]
struct Observed {
    kind: Kind,
    /// The resolved name -- the join key.
    name: String,
    /// `package/msg/Name`, the contract's own spelling.
    type_name: String,
    depth: Option<u32>,
    reliability: Option<String>,
    durability: Option<String>,
    history: Option<String>,
}

#[derive(Debug, Clone)]
struct ObservedNode {
    /// The name the contract would use: the node's own name, without the
    /// namespace.
    key: String,
    endpoints: Vec<Observed>,
    /// Periods in ms, one per timer row that is not a guard condition.
    timer_periods_ms: Vec<u64>,
    /// How many guard conditions this node opened. Not compared: the contract
    /// schema has no way to declare one (see [`Report::notes`]).
    guard_conditions: usize,
}

/// The recorder document, whether it arrived wrapped or bare.
fn recorder_doc(census: &Value) -> &Value {
    census.get("census").unwrap_or(census)
}

/// Resolve a written name the way the runtime does: an absolute name is
/// itself, a relative one hangs under the node's namespace, a private one
/// under the node's own fully qualified name.
///
/// Remappings are NOT applied, and cannot be: the census records what the code
/// passed and the model does not say which endpoint a remap hit. That is the
/// same refusal [`crate::contract_join`] makes for the same reason, and it is
/// recorded in the phase doc's Limits rather than guessed at here.
fn resolve(written: &str, kind: &str, namespace: &str, node_fqn: &str) -> String {
    match kind {
        "absolute" => written.to_string(),
        "private" => format!("{node_fqn}/{written}"),
        _ if namespace == "/" => format!("/{written}"),
        _ => format!("{namespace}/{written}"),
    }
}

fn source_name(entity: &Value, field: &str) -> Option<(String, String)> {
    let v = entity.get(field)?;
    let value = v.get("value")?.as_str()?.to_string();
    let kind = v.get("kind").and_then(Value::as_str).unwrap_or("absolute");
    Some((value, kind.to_string()))
}

/// `{package, name: "msg/Control", kind}` back to the contract's
/// `package/msg/Control`.
fn interface_type(entity: &Value) -> String {
    let Some(iface) = entity.get("interface") else {
        return String::new();
    };
    let package = iface.get("package").and_then(Value::as_str).unwrap_or("");
    let name = iface.get("name").and_then(Value::as_str).unwrap_or("");
    if package.is_empty() {
        return name.to_string();
    }
    format!("{package}/{name}")
}

/// The parameter and lifecycle service families the ENTRY registers on every
/// node's behalf, which no component declares and no contract can.
///
/// Excluded by the INTERFACE kind and not by the endpoint's name, which is the
/// distinction the phase doc draws: a user service that happens to be called
/// `get_parameters` is still checked, and an infra service under an unexpected
/// name is still excluded.
fn is_infra_interface(type_name: &str) -> bool {
    type_name.starts_with("rcl_interfaces/") || type_name.starts_with("lifecycle_msgs/")
}

fn qos_word(entity: &Value, field: &str) -> Option<String> {
    let word = entity.get("qos")?.get(field)?.as_str()?;
    // `system_default` is the recorder's spelling for "the node did not ask",
    // which is ABSENT and not a policy. Comparing it with a declared policy
    // would report a mismatch where nobody made two statements.
    (word != "system_default" && word != "unknown").then(|| word.to_string())
}

fn read_census(census: &Value) -> Vec<ObservedNode> {
    let doc = recorder_doc(census);
    let Some(nodes) = doc.get("nodes").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for node in nodes {
        let name = node
            .get("unresolved_name")
            .and_then(|n| n.get("value"))
            .and_then(Value::as_str)
            .or_else(|| node.get("id").and_then(Value::as_str))
            .unwrap_or("")
            .to_string();
        let namespace = node
            .get("namespace")
            .and_then(Value::as_str)
            .unwrap_or("/")
            .to_string();
        let key = name.trim_start_matches('/').to_string();
        let node_fqn = if namespace == "/" {
            format!("/{key}")
        } else {
            format!("{namespace}/{key}")
        };

        let mut endpoints = Vec::new();
        for (field, kind, name_field) in [
            ("subscribers", Kind::Sub, "unresolved_topic"),
            ("publishers", Kind::Pub, "unresolved_topic"),
            ("services", Kind::Srv, "unresolved_name"),
            ("service_clients", Kind::Cli, "unresolved_name"),
            ("actions", Kind::ActionServer, "unresolved_name"),
            ("action_clients", Kind::ActionClient, "unresolved_name"),
        ] {
            let Some(rows) = node.get(field).and_then(Value::as_array) else {
                continue;
            };
            for row in rows {
                let Some((written, spelling)) = source_name(row, name_field) else {
                    continue;
                };
                let type_name = interface_type(row);
                if is_infra_interface(&type_name) {
                    continue;
                }
                endpoints.push(Observed {
                    kind,
                    name: resolve(&written, &spelling, &namespace, &node_fqn),
                    type_name,
                    depth: row
                        .get("qos")
                        .and_then(|q| q.get("depth"))
                        .and_then(Value::as_u64)
                        .map(|d| d as u32),
                    reliability: qos_word(row, "reliability"),
                    durability: qos_word(row, "durability"),
                    history: qos_word(row, "history"),
                });
            }
        }

        let mut timer_periods_ms = Vec::new();
        let mut guard_conditions = 0usize;
        if let Some(rows) = node.get("timers").and_then(Value::as_array) {
            for row in rows {
                match row.get("kind").and_then(Value::as_str) {
                    Some("guard_condition") => guard_conditions += 1,
                    _ => timer_periods_ms
                        .push(row.get("period_ms").and_then(Value::as_u64).unwrap_or(0)),
                }
            }
        }
        timer_periods_ms.sort_unstable();

        out.push(ObservedNode {
            key,
            endpoints,
            timer_periods_ms,
            guard_conditions,
        });
    }
    out
}

/// Every parameter the code declared, as `(node, name, type)`.
fn read_census_params(census: &Value) -> Vec<(String, String, String)> {
    let doc = recorder_doc(census);
    let Some(rows) = doc.get("parameters").and_then(Value::as_array) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            Some((
                row.get("node")
                    .and_then(Value::as_str)?
                    .trim_start_matches('/')
                    .to_string(),
                row.get("name").and_then(Value::as_str)?.to_string(),
                row.get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("not_set")
                    .to_string(),
            ))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The contract side
// ---------------------------------------------------------------------------

/// One endpoint the contract declares, with the wiring resolved.
#[derive(Debug, Clone)]
struct Declared {
    node: String,
    kind: Kind,
    /// The per-node alias a person edits.
    alias: String,
    /// The resolved topic or service name, or `None` when the endpoint is
    /// wired to nothing -- the `unwired` case.
    wired: Option<String>,
    type_name: Option<String>,
    qos: QosDecl,
}

fn param_type_word(ty: ParamType) -> &'static str {
    match ty {
        ParamType::Bool => "bool",
        ParamType::Integer => "integer",
        ParamType::Double => "double",
        ParamType::String => "string",
        ParamType::ByteArray => "byte_array",
        ParamType::BoolArray => "bool_array",
        ParamType::IntegerArray => "integer_array",
        ParamType::DoubleArray => "double_array",
        ParamType::StringArray => "string_array",
    }
}

fn read_contract(contract: &Manifest) -> Vec<Declared> {
    // The wiring, inverted once: `node/alias` to the topic (or service) that
    // names it. Inverting per endpoint would be the same walk once per row.
    let mut wiring: BTreeMap<(Kind, String), (String, String, Option<QosDecl>)> = BTreeMap::new();
    for (topic, decl) in &contract.topics {
        for who in &decl.publishers {
            wiring.insert(
                (Kind::Pub, who.clone()),
                (topic.clone(), decl.msg_type.clone(), decl.qos.clone()),
            );
        }
        for who in &decl.subscribers {
            wiring.insert(
                (Kind::Sub, who.clone()),
                (topic.clone(), decl.msg_type.clone(), decl.qos.clone()),
            );
        }
    }
    for (name, decl) in &contract.services {
        for who in &decl.server {
            wiring.insert(
                (Kind::Srv, who.clone()),
                (name.clone(), decl.srv_type.clone(), None),
            );
        }
        for who in &decl.client {
            wiring.insert(
                (Kind::Cli, who.clone()),
                (name.clone(), decl.srv_type.clone(), None),
            );
        }
    }
    for (name, decl) in &contract.actions {
        for who in &decl.server {
            wiring.insert(
                (Kind::ActionServer, who.clone()),
                (name.clone(), decl.action_type.clone(), None),
            );
        }
        for who in &decl.client {
            wiring.insert(
                (Kind::ActionClient, who.clone()),
                (name.clone(), decl.action_type.clone(), None),
            );
        }
    }

    let mut out = Vec::new();
    for (node, decl) in &contract.nodes {
        let mut push = |kind: Kind, alias: &String, endpoint_qos: Option<&QosDecl>| {
            let wired = wiring.get(&(kind, format!("{node}/{alias}")));
            out.push(Declared {
                node: node.clone(),
                kind,
                alias: alias.clone(),
                wired: wired.map(|(t, _, _)| t.clone()),
                type_name: wired.map(|(_, ty, _)| ty.clone()),
                qos: QosDecl::effective(wired.and_then(|(_, _, q)| q.as_ref()), endpoint_qos),
            });
        };
        for (alias, props) in &decl.subscribers {
            push(Kind::Sub, alias, props.qos.as_ref());
        }
        for (alias, props) in &decl.publishers {
            push(Kind::Pub, alias, props.qos.as_ref());
        }
        for alias in decl.srv.keys() {
            push(Kind::Srv, alias, None);
        }
        for (alias, props) in &decl.cli {
            push(Kind::Cli, alias, props.qos.as_ref());
        }
    }
    out
}

/// The periods, in ms, the node's timer-triggered paths declare.
fn declared_timer_periods(contract: &Manifest, node: &str) -> Vec<(String, f64, u64)> {
    let Some(decl) = contract.nodes.get(node) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (path, path_decl) in &decl.paths {
        if let Some(ros_launch_manifest_types::Trigger::Timer { rate_hz }) = &path_decl.trigger
            && *rate_hz > 0.0
        {
            out.push((path.clone(), *rate_hz, (1000.0 / rate_hz).round() as u64));
        }
    }
    out.sort_by_key(|(_, _, ms)| *ms);
    out
}

// ---------------------------------------------------------------------------
// The join
// ---------------------------------------------------------------------------

/// Compare the three documents and emit one verdict per `(node, kind, name)`.
pub fn check(inputs: Inputs<'_>) -> Report {
    let observed = read_census(inputs.census);
    let declared = read_contract(inputs.contract);
    let contract_path = inputs.contract_path.clone();
    let mut rows: Vec<Row> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    let censused: BTreeSet<&str> = observed.iter().map(|n| n.key.as_str()).collect();

    // Which declared endpoints a census row claimed. A declared row nothing
    // claimed is the OVER direction.
    let mut claimed: BTreeSet<(String, Kind, String)> = BTreeSet::new();

    for node in &observed {
        for entity in &node.endpoints {
            let hit = declared.iter().find(|d| {
                d.node == node.key
                    && d.kind == entity.kind
                    && d.wired.as_deref() == Some(entity.name.as_str())
            });
            let Some(decl) = hit else {
                rows.push(missing_in_contract(node, entity, &contract_path));
                continue;
            };
            claimed.insert((decl.node.clone(), decl.kind, decl.alias.clone()));
            rows.extend(compare_facts(node, entity, decl, &contract_path));
        }

        // The timers, matched by period. A node's timer has no name on either
        // side: the contract names a PATH and the census names a period, so
        // the pairing is by sorted period and the leftovers are the verdicts.
        let declared_periods = declared_timer_periods(inputs.contract, &node.key);
        let mut left = node.timer_periods_ms.iter().copied().peekable();
        let mut right = declared_periods.iter().peekable();
        while left.peek().is_some() || right.peek().is_some() {
            match (left.peek().copied(), right.peek().copied()) {
                (Some(observed_ms), Some((path, rate_hz, declared_ms))) => {
                    left.next();
                    right.next();
                    if observed_ms == *declared_ms {
                        rows.push(Row {
                            node: node.key.clone(),
                            kind: Kind::Timer,
                            name: path.clone(),
                            verdict: Verdict::Confirmed,
                            detail: format!("{observed_ms} ms, as declared"),
                            remedy: String::new(),
                            waiver: None,
                        });
                    } else {
                        rows.push(Row {
                            node: node.key.clone(),
                            kind: Kind::Timer,
                            name: path.clone(),
                            verdict: Verdict::PeriodMismatch,
                            detail: format!(
                                "the code launches this timer at {observed_ms} ms; \
                                 `paths.{path}.trigger.timer.rate_hz: {rate_hz}` declares \
                                 {declared_ms} ms"
                            ),
                            remedy: format!(
                                "in {contract_path}, under `nodes.{}.paths.{path}.trigger`:\n    \
                                 timer: {{ rate_hz: {} }}\nor change the period the code passes \
                                 to {declared_ms}",
                                node.key,
                                round_rate(observed_ms)
                            ),
                            waiver: None,
                        });
                    }
                }
                (Some(observed_ms), None) => {
                    left.next();
                    rows.push(Row {
                        node: node.key.clone(),
                        kind: Kind::Timer,
                        name: format!("{observed_ms}ms"),
                        verdict: Verdict::MissingInContract,
                        detail: format!(
                            "the code creates a timer at {observed_ms} ms; no path of \
                             `nodes.{}` declares a timer trigger for it",
                            node.key
                        ),
                        remedy: format!(
                            "in {contract_path}, under `nodes.{}.paths`:\n    <path_name>:\n      \
                             trigger: {{ timer: {{ rate_hz: {} }} }}\n      output: [...]",
                            node.key,
                            round_rate(observed_ms)
                        ),
                        waiver: None,
                    });
                }
                (None, Some((path, rate_hz, declared_ms))) => {
                    right.next();
                    rows.push(Row {
                        node: node.key.clone(),
                        kind: Kind::Timer,
                        name: path.clone(),
                        verdict: Verdict::Phantom,
                        detail: format!(
                            "`paths.{path}` declares a timer trigger at {rate_hz} Hz \
                             ({declared_ms} ms); the code creates no such timer"
                        ),
                        remedy: format!(
                            "in {contract_path}, remove `nodes.{}.paths.{path}.trigger.timer` \
                             or give the node the timer it declares",
                            node.key
                        ),
                        waiver: None,
                    });
                }
                (None, None) => break,
            }
        }

        if node.guard_conditions > 0 {
            notes.push(format!(
                "{}: {} guard condition(s) observed and not compared -- the contract schema has \
                 no way to declare one, so an absent row here is not a delta",
                node.key, node.guard_conditions
            ));
        }
    }

    // The OVER direction: declared and not claimed.
    for decl in &declared {
        if claimed.contains(&(decl.node.clone(), decl.kind, decl.alias.clone())) {
            continue;
        }
        if !censused.contains(decl.node.as_str()) {
            rows.push(Row {
                node: decl.node.clone(),
                kind: decl.kind,
                name: decl.wired.clone().unwrap_or_else(|| decl.alias.clone()),
                verdict: Verdict::Unobserved,
                detail: format!(
                    "the contract declares this {}; the census never ran `{}`",
                    decl.kind.noun(),
                    decl.node
                ),
                remedy: format!(
                    "run `nros ws entity-census run` on an entry that launches `{}`, or accept \
                     that this row is unchecked",
                    decl.node
                ),
                waiver: None,
            });
            continue;
        }
        let Some(wired) = decl.wired.clone() else {
            rows.push(Row {
                node: decl.node.clone(),
                kind: decl.kind,
                name: decl.alias.clone(),
                verdict: Verdict::Unwired,
                detail: format!(
                    "`nodes.{}.{}.{}` is declared and no `{}:` entry names \
                     `{}/{}`, so the resolver drops it",
                    decl.node,
                    decl.kind.tag(),
                    decl.alias,
                    wiring_table(decl.kind),
                    decl.node,
                    decl.alias
                ),
                remedy: format!(
                    "in {contract_path}, either wire it under `{}:`:\n    <name>:\n      \
                     type: <package>/<kind>/<Name>\n      {}: [{}/{}]\nor remove \
                     `nodes.{}.{}.{}`",
                    wiring_table(decl.kind),
                    wiring_side(decl.kind),
                    decl.node,
                    decl.alias,
                    decl.node,
                    decl.kind.tag(),
                    decl.alias
                ),
                waiver: None,
            });
            continue;
        };
        rows.push(Row {
            node: decl.node.clone(),
            kind: decl.kind,
            name: wired.clone(),
            verdict: Verdict::Phantom,
            detail: format!(
                "the contract declares and wires this {} as `{}`; the code never created it",
                decl.kind.noun(),
                wired
            ),
            remedy: format!(
                "in {contract_path}, remove `nodes.{}.{}.{}` and its `{}` entry, or give the \
                 code the {} it declares",
                decl.node,
                decl.kind.tag(),
                decl.alias,
                wired,
                decl.kind.noun()
            ),
            waiver: None,
        });
    }

    rows.extend(check_params(
        inputs.census,
        inputs.contract,
        &censused,
        &contract_path,
    ));

    if let Some(inventory) = inputs.inventory {
        notes.extend(inventory_notes(inventory, &rows));
    }

    // The waivers, applied last: a waiver names a ROW, and a row exists only
    // once the join has produced it.
    for row in &mut rows {
        if !row.verdict.waivable() {
            continue;
        }
        if let Some(reason) = inputs.waivers.get(&row.key()) {
            row.waiver = Some(reason.clone());
        }
    }

    rows.sort_by(|a, b| {
        (&a.node, a.kind, &a.name, a.verdict).cmp(&(&b.node, b.kind, &b.name, b.verdict))
    });

    Report {
        rows,
        strict: inputs.strict,
        contract_path,
        notes,
    }
}

fn round_rate(period_ms: u64) -> f64 {
    if period_ms == 0 {
        return 0.0;
    }
    (1000.0 / period_ms as f64 * 100.0).round() / 100.0
}

fn wiring_table(kind: Kind) -> &'static str {
    match kind {
        Kind::Sub | Kind::Pub => "topics",
        Kind::Srv | Kind::Cli => "services",
        _ => "actions",
    }
}

fn wiring_side(kind: Kind) -> &'static str {
    match kind {
        Kind::Sub => "sub",
        Kind::Pub => "pub",
        Kind::Srv | Kind::ActionServer => "server",
        _ => "client",
    }
}

/// The UNDER direction, stated so a person can fix it in one edit: the node,
/// the entity, the file, and both lines the contract needs.
fn missing_in_contract(node: &ObservedNode, entity: &Observed, contract_path: &str) -> Row {
    let alias = entity
        .name
        .rsplit('/')
        .next()
        .unwrap_or(&entity.name)
        .to_string();
    let qos = match entity.depth {
        Some(depth) => format!(" {{ qos: {{ depth: {depth} }} }}"),
        None => " {}".to_string(),
    };
    Row {
        node: node.key.clone(),
        kind: entity.kind,
        name: entity.name.clone(),
        verdict: Verdict::MissingInContract,
        detail: format!(
            "the code creates this {} on `{}`; no `{}:` row of `nodes.{}` is wired to it, so \
             no `{}:` entry names it either",
            entity.kind.noun(),
            entity.name,
            entity.kind.tag(),
            node.key,
            wiring_table(entity.kind)
        ),
        remedy: format!(
            "in {contract_path}, add under `nodes.{}.{}` (the alias is the author's; `{alias}` \
             is this name's last segment):\n    {alias}:{qos}\nand wire it under \
             `{}`:\n    {}:\n      type: {}\n      {}: [{}/{alias}]",
            node.key,
            entity.kind.tag(),
            wiring_table(entity.kind),
            entity.name,
            if entity.type_name.is_empty() {
                "<package>/<kind>/<Name>"
            } else {
                entity.type_name.as_str()
            },
            wiring_side(entity.kind),
            node.key,
        ),
        waiver: None,
    }
}

/// Facts that must agree once both sides name the same endpoint.
fn compare_facts(
    node: &ObservedNode,
    entity: &Observed,
    decl: &Declared,
    contract_path: &str,
) -> Vec<Row> {
    let mut rows = Vec::new();
    let base = |verdict: Verdict, detail: String, remedy: String| Row {
        node: node.key.clone(),
        kind: entity.kind,
        name: entity.name.clone(),
        verdict,
        detail,
        remedy,
        waiver: None,
    };

    if let Some(declared_type) = decl.type_name.as_deref()
        && !entity.type_name.is_empty()
        && declared_type != entity.type_name
    {
        rows.push(base(
            Verdict::TypeMismatch,
            format!(
                "the code's message type is `{}`; `topics.{}.type` says `{declared_type}`",
                entity.type_name, entity.name
            ),
            format!(
                "in {contract_path}, under `{}.{}`:\n    type: {}",
                wiring_table(entity.kind),
                entity.name,
                entity.type_name
            ),
        ));
    }

    if let (Some(observed), Some(declared)) = (entity.depth, decl.qos.depth)
        && observed != declared
    {
        rows.push(base(
            Verdict::DepthMismatch,
            format!("the code passes history depth {observed}; the contract declares {declared}"),
            format!(
                "in {contract_path}, under `nodes.{}.{}.{}`:\n    qos: {{ depth: {observed} }}\n\
                 or change the depth the code passes to {declared}",
                decl.node,
                decl.kind.tag(),
                decl.alias
            ),
        ));
    }

    for (what, observed, declared) in [
        ("reliability", &entity.reliability, &decl.qos.reliability),
        ("durability", &entity.durability, &decl.qos.durability),
        ("history", &entity.history, &decl.qos.history),
    ] {
        if let (Some(observed), Some(declared)) = (observed.as_deref(), declared.as_deref())
            && observed != declared
        {
            rows.push(base(
                Verdict::QosMismatch,
                format!("the code passes {what} `{observed}`; the contract declares `{declared}`"),
                format!(
                    "in {contract_path}, under `nodes.{}.{}.{}.qos`:\n    {what}: {observed}",
                    decl.node,
                    decl.kind.tag(),
                    decl.alias
                ),
            ));
        }
    }

    if rows.is_empty() {
        rows.push(base(
            Verdict::Confirmed,
            format!(
                "declared as `nodes.{}.{}.{}`",
                decl.node,
                decl.kind.tag(),
                decl.alias
            ),
            String::new(),
        ));
    }
    rows
}

/// The parameter half. Its two directions are the same two, under their own
/// verdict names because the remedy is a different block of the same file.
fn check_params(
    census: &Value,
    contract: &Manifest,
    censused: &BTreeSet<&str>,
    contract_path: &str,
) -> Vec<Row> {
    let observed = read_census_params(census);
    let mut rows = Vec::new();
    let mut claimed: BTreeSet<(String, String)> = BTreeSet::new();

    for (node, name, ty) in &observed {
        let Some(decl) = contract.nodes.get(node) else {
            continue;
        };
        // `params:` absent is "not stated", which is a different statement
        // from `params: {}` ("declares none"). Not stated means this check has
        // nothing to compare, and guessing here would turn a silence into an
        // accusation.
        let Some(params) = decl.params.as_ref() else {
            continue;
        };
        let Some(declared) = params.get(name) else {
            rows.push(Row {
                node: node.clone(),
                kind: Kind::Param,
                name: name.clone(),
                verdict: Verdict::ParamMissingInContract,
                detail: format!(
                    "the code declares parameter `{name}` ({ty}); `nodes.{node}.params` does not"
                ),
                remedy: format!(
                    "in {contract_path}, add under `nodes.{node}.params`:\n    {name}: \
                     {{ type: {ty} }}"
                ),
                waiver: None,
            });
            continue;
        };
        claimed.insert((node.clone(), name.clone()));
        let declared_word = param_type_word(declared.ty);
        if declared_word != ty {
            rows.push(Row {
                node: node.clone(),
                kind: Kind::Param,
                name: name.clone(),
                verdict: Verdict::ParamTypeMismatch,
                detail: format!(
                    "the code declares `{name}` as {ty}; the contract declares {declared_word}"
                ),
                remedy: format!(
                    "in {contract_path}, under `nodes.{node}.params.{name}`:\n    type: {ty}"
                ),
                waiver: None,
            });
        } else {
            rows.push(Row {
                node: node.clone(),
                kind: Kind::Param,
                name: name.clone(),
                verdict: Verdict::Confirmed,
                detail: format!("declared as `nodes.{node}.params.{name}: {{ type: {ty} }}`"),
                remedy: String::new(),
                waiver: None,
            });
        }
    }

    for (node, decl) in &contract.nodes {
        let Some(params) = decl.params.as_ref() else {
            continue;
        };
        for (name, param) in params {
            if claimed.contains(&(node.clone(), name.clone())) {
                continue;
            }
            if !censused.contains(node.as_str()) {
                rows.push(Row {
                    node: node.clone(),
                    kind: Kind::Param,
                    name: name.clone(),
                    verdict: Verdict::Unobserved,
                    detail: format!(
                        "the contract declares parameter `{name}`; the census never ran `{node}`"
                    ),
                    remedy: format!(
                        "run `nros ws entity-census run` on an entry that launches `{node}`"
                    ),
                    waiver: None,
                });
                continue;
            }
            rows.push(Row {
                node: node.clone(),
                kind: Kind::Param,
                name: name.clone(),
                verdict: Verdict::ParamPhantom,
                detail: format!(
                    "`nodes.{node}.params.{name}` ({}) names a parameter the code never \
                     declares, so a launch value for it can reach nothing",
                    param_type_word(param.ty)
                ),
                remedy: format!("in {contract_path}, remove `nodes.{node}.params.{name}`"),
                waiver: None,
            });
        }
    }

    rows
}

/// What the DERIVED view adds. It is not a fourth opinion about an endpoint:
/// it is the record of what the pools were actually sized from, so a refusal
/// can say what the image would have been built with.
fn inventory_notes(inventory: &Value, rows: &[Row]) -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(status) = inventory.get("status").and_then(Value::as_str)
        && status != "derived"
    {
        notes.push(format!(
            "the entity inventory is `{status}`, so no pool size was derived from this contract \
             at all"
        ));
        return notes;
    }
    let short = rows
        .iter()
        .filter(|r| r.verdict == Verdict::MissingInContract)
        .count();
    if short > 0
        && let Some(max_cbs) = inventory.get("max_cbs").and_then(Value::as_u64)
    {
        notes.push(format!(
            "the inventory derived max_cbs = {max_cbs} from this contract; {short} row(s) the \
             code creates are not in it, so the executor arena is sized short"
        ));
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The safety island's contract, reproduced in tree: four C++ nodes, 11
    /// subscriptions, 14 publishers, 2 service servers, 2 service clients, 4
    /// timers and 21 parameters. Comments trimmed; every declaration kept.
    ///
    /// This is the acceptance fixture and not a minimal one on purpose. The
    /// defect this phase exists for was found on THIS contract, and a
    /// three-row fixture would not have shown that the omission (E3a) is
    /// invisible to every other host layer.
    const ISLAND_CONTRACT: &str = r#"
version: 1

nodes:
  mrm_comfortable_stop_operator:
    params:
      update_rate: { type: integer }
      min_acceleration: { type: double }
      max_jerk: { type: double }
      min_jerk: { type: double }
    srv:
      operate: {}
    paths:
      on_timer:
        trigger: { timer: { rate_hz: 10 } }
        output: [status]
    pub:
      max_velocity_candidates: { min_rate_hz: 10 }
      clear_velocity_limit: { min_rate_hz: 10 }
      status: { min_rate_hz: 10 }

  mrm_emergency_stop_operator:
    params:
      update_rate: { type: integer }
      target_acceleration: { type: double }
      target_jerk: { type: double }
    srv:
      operate: {}
    paths:
      on_timer:
        trigger: { timer: { rate_hz: 30 } }
        output: [emergency_control_cmd, status]
    sub:
      control_cmd: { min_rate_hz: 30, qos: { depth: 1 }, state: true }
    pub:
      emergency_control_cmd: { min_rate_hz: 30 }
      status: { min_rate_hz: 30 }

  mrm_handler:
    params:
      update_rate: { type: integer }
      timeout_operation_mode_availability: { type: double }
      timeout_call_mrm_behavior: { type: double }
      timeout_cancel_mrm_behavior: { type: double }
      use_emergency_holding: { type: bool }
      timeout_emergency_recovery: { type: double }
      use_parking_after_stopped: { type: bool }
      use_pull_over: { type: bool }
      use_comfortable_stop: { type: bool }
      turning_hazard_on.emergency: { type: bool }
      turning_indicator_on.emergency: { type: bool }
    cli:
      comfortable_stop_operate: {}
      emergency_stop_operate: {}
    paths:
      on_timer:
        trigger: { timer: { rate_hz: 10 } }
        output:
          - mrm_state
          - turn_indicators_cmd
          - hazard_lights_cmd
          - gear_cmd_out
          - emergency_holding
    sub:
      operation_mode_availability: { min_rate_hz: 10, qos: { depth: 1 } }
      kinematic_state: { min_rate_hz: 10, qos: { depth: 1 } }
      control_mode: { min_rate_hz: 10, qos: { depth: 1 } }
      comfortable_stop_status: { min_rate_hz: 10, qos: { depth: 1 } }
      emergency_stop_status: { min_rate_hz: 10, qos: { depth: 1 } }
      gear_cmd_in: { min_rate_hz: 10, qos: { depth: 1 } }
      operation_mode_state: { min_rate_hz: 10, qos: { depth: 1 } }
    pub:
      mrm_state: { min_rate_hz: 10 }
      gear_cmd_out: { min_rate_hz: 10 }
      hazard_lights_cmd: { min_rate_hz: 10 }
      turn_indicators_cmd: { min_rate_hz: 10 }
      emergency_holding: { min_rate_hz: 10 }

  stop_mode_operator:
    params:
      rate: { type: double }
      stop_hold_acceleration: { type: double }
      enable_auto_parking: { type: bool }
    paths:
      on_timer:
        trigger: { timer: { rate_hz: 30 } }
        output: [control, gear, turn_indicators, hazard_lights]
    sub:
      steering_status: { min_rate_hz: 30, qos: { depth: 1 } }
      velocity_status: { min_rate_hz: 30, qos: { depth: 1 } }
      route_state: { min_rate_hz: 30, qos: { depth: 1 } }
    pub:
      control: { min_rate_hz: 30 }
      gear: { min_rate_hz: 30 }
      hazard_lights: { min_rate_hz: 30 }
      turn_indicators: { min_rate_hz: 30 }

topics:
  /system/mrm/comfortable_stop/status:
    type: tier4_system_msgs/msg/MrmBehaviorStatus
    pub: [mrm_comfortable_stop_operator/status]
    sub: [mrm_handler/comfortable_stop_status]
    rate_hz: 10
  /system/mrm/emergency_stop/status:
    type: tier4_system_msgs/msg/MrmBehaviorStatus
    pub: [mrm_emergency_stop_operator/status]
    sub: [mrm_handler/emergency_stop_status]
    rate_hz: 30
  /control/command/control_cmd:
    type: autoware_control_msgs/msg/Control
    external: pub
    sub: [mrm_emergency_stop_operator/control_cmd]
    rate_hz: 30
  /system/operation_mode/availability:
    type: tier4_system_msgs/msg/OperationModeAvailability
    external: pub
    sub: [mrm_handler/operation_mode_availability]
    rate_hz: 10
  /localization/kinematic_state:
    type: nav_msgs/msg/Odometry
    external: pub
    sub: [mrm_handler/kinematic_state]
    rate_hz: 10
  /vehicle/status/control_mode:
    type: autoware_vehicle_msgs/msg/ControlModeReport
    external: pub
    sub: [mrm_handler/control_mode]
    rate_hz: 10
  /control/command/gear_cmd:
    type: autoware_vehicle_msgs/msg/GearCommand
    external: pub
    sub: [mrm_handler/gear_cmd_in]
    rate_hz: 10
  /api/operation_mode/state:
    type: autoware_adapi_v1_msgs/msg/OperationModeState
    external: pub
    sub: [mrm_handler/operation_mode_state]
    rate_hz: 10
  /vehicle/status/steering_status:
    type: autoware_vehicle_msgs/msg/SteeringReport
    external: pub
    sub: [stop_mode_operator/steering_status]
    rate_hz: 30
  /vehicle/status/velocity_status:
    type: autoware_vehicle_msgs/msg/VelocityReport
    external: pub
    sub: [stop_mode_operator/velocity_status]
    rate_hz: 30
  /planning/route_state:
    type: autoware_planning_msgs/msg/RouteState
    external: pub
    sub: [stop_mode_operator/route_state]
    rate_hz: 30
  /planning/scenario_planning/max_velocity_candidates:
    type: autoware_internal_planning_msgs/msg/VelocityLimit
    external: sub
    pub: [mrm_comfortable_stop_operator/max_velocity_candidates]
    rate_hz: 10
  /planning/scenario_planning/clear_velocity_limit:
    type: autoware_internal_planning_msgs/msg/VelocityLimitClearCommand
    external: sub
    pub: [mrm_comfortable_stop_operator/clear_velocity_limit]
    rate_hz: 10
  /system/emergency/control_cmd:
    type: autoware_control_msgs/msg/Control
    external: sub
    pub: [mrm_emergency_stop_operator/emergency_control_cmd]
    rate_hz: 30
  /system/fail_safe/mrm_state:
    type: autoware_adapi_v1_msgs/msg/MrmState
    external: sub
    pub: [mrm_handler/mrm_state]
    rate_hz: 10
  /system/emergency/gear_cmd:
    type: autoware_vehicle_msgs/msg/GearCommand
    external: sub
    pub: [mrm_handler/gear_cmd_out]
    rate_hz: 10
  /system/emergency/hazard_lights_cmd:
    type: autoware_vehicle_msgs/msg/HazardLightsCommand
    external: sub
    pub: [mrm_handler/hazard_lights_cmd]
    rate_hz: 10
  /system/emergency/turn_indicators_cmd:
    type: autoware_vehicle_msgs/msg/TurnIndicatorsCommand
    external: sub
    pub: [mrm_handler/turn_indicators_cmd]
    rate_hz: 10
  /system/fail_safe/emergency_holding:
    type: tier4_system_msgs/msg/EmergencyHoldingState
    external: sub
    pub: [mrm_handler/emergency_holding]
    rate_hz: 10
  /system/stop_mode/control:
    type: autoware_control_msgs/msg/Control
    external: sub
    pub: [stop_mode_operator/control]
    rate_hz: 30
  /system/stop_mode/gear:
    type: autoware_vehicle_msgs/msg/GearCommand
    external: sub
    pub: [stop_mode_operator/gear]
    rate_hz: 30
  /system/stop_mode/hazard_lights:
    type: autoware_vehicle_msgs/msg/HazardLightsCommand
    external: sub
    pub: [stop_mode_operator/hazard_lights]
    rate_hz: 30
  /system/stop_mode/turn_indicators:
    type: autoware_vehicle_msgs/msg/TurnIndicatorsCommand
    external: sub
    pub: [stop_mode_operator/turn_indicators]
    rate_hz: 30

services:
  /system/mrm/comfortable_stop/operate:
    type: tier4_system_msgs/srv/OperateMrm
    server: [mrm_comfortable_stop_operator/operate]
    client: [mrm_handler/comfortable_stop_operate]
  /system/mrm/emergency_stop/operate:
    type: tier4_system_msgs/srv/OperateMrm
    server: [mrm_emergency_stop_operator/operate]
    client: [mrm_handler/emergency_stop_operate]
"#;

    const CONTRACT_PATH: &str = "src/safety_island_bringup/launch/safety_island.contract.yaml";

    fn manifest(yaml: &str) -> Manifest {
        ros_launch_manifest_types::parse_manifest_str(yaml).expect("the fixture contract parses")
    }

    /// One endpoint row in the recorder's shape: the topic AS THE CODE WROTE
    /// IT (absolute on the island) and the interface split the way
    /// `parse_interface` splits it.
    fn endpoint(name_field: &str, topic: &str, ty: &str, depth: u32) -> Value {
        let (package, leaf) = ty.split_once('/').unwrap_or(("", ty));
        json!({
            "id": topic,
            name_field: { "value": topic, "kind": "absolute" },
            "interface": { "package": package, "name": leaf, "kind": "message" },
            "qos": {
                "reliability": "reliable",
                "durability": "volatile",
                "history": "keep_last",
                "depth": depth,
                "liveliness": "system_default",
            },
        })
    }

    fn node(name: &str, period_ms: u64) -> Value {
        json!({
            "id": name,
            "unresolved_name": { "value": name, "kind": "relative" },
            "namespace": null,
            "publishers": [],
            "subscribers": [],
            "services": [],
            "service_clients": [],
            "actions": [],
            "action_clients": [],
            "timers": [ { "id": "on_timer", "kind": "wall", "period_ms": period_ms } ],
        })
    }

    fn push(node: &mut Value, field: &str, row: Value) {
        node.get_mut(field)
            .and_then(Value::as_array_mut)
            .expect("the field exists")
            .push(row);
    }

    fn param(node: &str, name: &str, ty: &str) -> Value {
        json!({ "node": node, "name": name, "type": ty })
    }

    /// The census the island's native entry writes: 11 / 14 / 2 / 2 / 4 / 21.
    fn island_census() -> Value {
        let mut comfortable = node("mrm_comfortable_stop_operator", 100);
        push(
            &mut comfortable,
            "services",
            endpoint(
                "unresolved_name",
                "/system/mrm/comfortable_stop/operate",
                "tier4_system_msgs/srv/OperateMrm",
                10,
            ),
        );
        for (topic, ty) in [
            (
                "/planning/scenario_planning/max_velocity_candidates",
                "autoware_internal_planning_msgs/msg/VelocityLimit",
            ),
            (
                "/planning/scenario_planning/clear_velocity_limit",
                "autoware_internal_planning_msgs/msg/VelocityLimitClearCommand",
            ),
            (
                "/system/mrm/comfortable_stop/status",
                "tier4_system_msgs/msg/MrmBehaviorStatus",
            ),
        ] {
            push(
                &mut comfortable,
                "publishers",
                endpoint("unresolved_topic", topic, ty, 10),
            );
        }

        let mut emergency = node("mrm_emergency_stop_operator", 33);
        push(
            &mut emergency,
            "subscribers",
            endpoint(
                "unresolved_topic",
                "/control/command/control_cmd",
                "autoware_control_msgs/msg/Control",
                1,
            ),
        );
        push(
            &mut emergency,
            "services",
            endpoint(
                "unresolved_name",
                "/system/mrm/emergency_stop/operate",
                "tier4_system_msgs/srv/OperateMrm",
                10,
            ),
        );
        for (topic, ty) in [
            (
                "/system/emergency/control_cmd",
                "autoware_control_msgs/msg/Control",
            ),
            (
                "/system/mrm/emergency_stop/status",
                "tier4_system_msgs/msg/MrmBehaviorStatus",
            ),
        ] {
            push(
                &mut emergency,
                "publishers",
                endpoint("unresolved_topic", topic, ty, 10),
            );
        }

        let mut handler = node("mrm_handler", 100);
        for (topic, ty) in [
            (
                "/system/operation_mode/availability",
                "tier4_system_msgs/msg/OperationModeAvailability",
            ),
            ("/localization/kinematic_state", "nav_msgs/msg/Odometry"),
            (
                "/vehicle/status/control_mode",
                "autoware_vehicle_msgs/msg/ControlModeReport",
            ),
            (
                "/system/mrm/comfortable_stop/status",
                "tier4_system_msgs/msg/MrmBehaviorStatus",
            ),
            (
                "/system/mrm/emergency_stop/status",
                "tier4_system_msgs/msg/MrmBehaviorStatus",
            ),
            (
                "/control/command/gear_cmd",
                "autoware_vehicle_msgs/msg/GearCommand",
            ),
            // The seventh. The CMakeLists ENTITIES list omitted it and so,
            // later, did the contract row that replaced that list.
            (
                "/api/operation_mode/state",
                "autoware_adapi_v1_msgs/msg/OperationModeState",
            ),
        ] {
            push(
                &mut handler,
                "subscribers",
                endpoint("unresolved_topic", topic, ty, 1),
            );
        }
        for name in [
            "/system/mrm/comfortable_stop/operate",
            "/system/mrm/emergency_stop/operate",
        ] {
            push(
                &mut handler,
                "service_clients",
                endpoint(
                    "unresolved_name",
                    name,
                    "tier4_system_msgs/srv/OperateMrm",
                    10,
                ),
            );
        }
        for (topic, ty) in [
            (
                "/system/fail_safe/mrm_state",
                "autoware_adapi_v1_msgs/msg/MrmState",
            ),
            (
                "/system/emergency/gear_cmd",
                "autoware_vehicle_msgs/msg/GearCommand",
            ),
            (
                "/system/emergency/hazard_lights_cmd",
                "autoware_vehicle_msgs/msg/HazardLightsCommand",
            ),
            (
                "/system/emergency/turn_indicators_cmd",
                "autoware_vehicle_msgs/msg/TurnIndicatorsCommand",
            ),
            (
                "/system/fail_safe/emergency_holding",
                "tier4_system_msgs/msg/EmergencyHoldingState",
            ),
        ] {
            push(
                &mut handler,
                "publishers",
                endpoint("unresolved_topic", topic, ty, 10),
            );
        }

        let mut stop_mode = node("stop_mode_operator", 33);
        for (topic, ty) in [
            (
                "/vehicle/status/steering_status",
                "autoware_vehicle_msgs/msg/SteeringReport",
            ),
            (
                "/vehicle/status/velocity_status",
                "autoware_vehicle_msgs/msg/VelocityReport",
            ),
            (
                "/planning/route_state",
                "autoware_planning_msgs/msg/RouteState",
            ),
        ] {
            push(
                &mut stop_mode,
                "subscribers",
                endpoint("unresolved_topic", topic, ty, 1),
            );
        }
        for (topic, ty) in [
            (
                "/system/stop_mode/control",
                "autoware_control_msgs/msg/Control",
            ),
            (
                "/system/stop_mode/gear",
                "autoware_vehicle_msgs/msg/GearCommand",
            ),
            (
                "/system/stop_mode/hazard_lights",
                "autoware_vehicle_msgs/msg/HazardLightsCommand",
            ),
            (
                "/system/stop_mode/turn_indicators",
                "autoware_vehicle_msgs/msg/TurnIndicatorsCommand",
            ),
        ] {
            push(
                &mut stop_mode,
                "publishers",
                endpoint("unresolved_topic", topic, ty, 10),
            );
        }

        let parameters: Vec<Value> = [
            ("mrm_comfortable_stop_operator", "update_rate", "integer"),
            (
                "mrm_comfortable_stop_operator",
                "min_acceleration",
                "double",
            ),
            ("mrm_comfortable_stop_operator", "max_jerk", "double"),
            ("mrm_comfortable_stop_operator", "min_jerk", "double"),
            ("mrm_emergency_stop_operator", "update_rate", "integer"),
            (
                "mrm_emergency_stop_operator",
                "target_acceleration",
                "double",
            ),
            ("mrm_emergency_stop_operator", "target_jerk", "double"),
            ("mrm_handler", "update_rate", "integer"),
            (
                "mrm_handler",
                "timeout_operation_mode_availability",
                "double",
            ),
            ("mrm_handler", "timeout_call_mrm_behavior", "double"),
            ("mrm_handler", "timeout_cancel_mrm_behavior", "double"),
            ("mrm_handler", "use_emergency_holding", "bool"),
            ("mrm_handler", "timeout_emergency_recovery", "double"),
            ("mrm_handler", "use_parking_after_stopped", "bool"),
            ("mrm_handler", "use_pull_over", "bool"),
            ("mrm_handler", "use_comfortable_stop", "bool"),
            ("mrm_handler", "turning_hazard_on.emergency", "bool"),
            ("mrm_handler", "turning_indicator_on.emergency", "bool"),
            ("stop_mode_operator", "rate", "double"),
            ("stop_mode_operator", "stop_hold_acceleration", "double"),
            ("stop_mode_operator", "enable_auto_parking", "bool"),
        ]
        .iter()
        .map(|(n, name, ty)| param(n, name, ty))
        .collect();

        json!({
            "schema": "nros.entity_census/1",
            "entry": "island_entry",
            "census": {
                "version": 2,
                "nodes": [comfortable, emergency, handler, stop_mode],
                "parameters": parameters,
            },
        })
    }

    fn no_waivers() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn run(census: &Value, contract: &Manifest, waivers: &BTreeMap<String, String>) -> Report {
        check(Inputs {
            census,
            contract,
            contract_path: CONTRACT_PATH.to_string(),
            inventory: None,
            waivers,
            strict: false,
        })
    }

    #[test]
    fn the_pristine_island_contract_confirms_every_row() {
        let census = island_census();
        let contract = manifest(ISLAND_CONTRACT);
        let report = run(&census, &contract, &no_waivers());

        assert_eq!(report.errors(), 0, "{}", report.render());
        assert_eq!(report.warnings(), 0, "{}", report.render());
        assert_eq!(
            report.confirmed(),
            54,
            "33 entity rows and 21 parameter rows:\n{}",
            report.render()
        );
        let entities = report
            .rows
            .iter()
            .filter(|r| r.verdict == Verdict::Confirmed && r.kind != Kind::Param)
            .count();
        assert_eq!(entities, 33, "{}", report.render());
    }

    /// E3a, the island's own defect: one `sub:` row and its topic deleted,
    /// nothing else changed. Every other host layer reported zero errors.
    #[test]
    fn e3a_the_omitted_subscription_is_missing_in_contract() {
        let census = island_census();
        let yaml = ISLAND_CONTRACT
            .replace(
                "      operation_mode_state: { min_rate_hz: 10, qos: { depth: 1 } }\n",
                "",
            )
            .replace(
                "  /api/operation_mode/state:\n    type: \
                 autoware_adapi_v1_msgs/msg/OperationModeState\n    external: pub\n    sub: \
                 [mrm_handler/operation_mode_state]\n    rate_hz: 10\n",
                "",
            );
        let contract = manifest(&yaml);
        let report = run(&census, &contract, &no_waivers());

        let missing: Vec<&Row> = report.of(Verdict::MissingInContract).collect();
        assert_eq!(missing.len(), 1, "{}", report.render());
        let row = missing[0];
        assert_eq!(row.node, "mrm_handler");
        assert_eq!(row.kind, Kind::Sub);
        assert_eq!(row.name, "/api/operation_mode/state");
        assert!(
            row.detail.contains("nodes.mrm_handler")
                && row.detail.contains("/api/operation_mode/state"),
            "the refusal names the node and the endpoint: {}",
            row.detail
        );
        assert!(
            row.remedy.contains(CONTRACT_PATH)
                && row.remedy.contains("state: { qos: { depth: 1 } }")
                && row
                    .remedy
                    .contains("autoware_adapi_v1_msgs/msg/OperationModeState"),
            "the refusal names the file and both lines: {}",
            row.remedy
        );
        assert!(!row.verdict.waivable(), "the UNDER direction has no waiver");
        assert_eq!(report.errors(), 1, "{}", report.render());
        assert!(report.refuses());
    }

    /// E3b: a sub the code never creates, declared and wired.
    #[test]
    fn e3b_a_declared_endpoint_the_code_never_creates_is_a_waivable_phantom() {
        let census = island_census();
        let yaml = ISLAND_CONTRACT
            .replace(
                "      gear_cmd_in: { min_rate_hz: 10, qos: { depth: 1 } }\n",
                "      gear_cmd_in: { min_rate_hz: 10, qos: { depth: 1 } }\n      phantom_in: \
                 { min_rate_hz: 10, qos: { depth: 1 } }\n",
            )
            .replace(
                "topics:\n",
                concat!(
                    "topics:\n",
                    "  /system/phantom:\n",
                    "    type: std_msgs/msg/Int32\n",
                    "    external: pub\n",
                    "    sub: [mrm_handler/phantom_in]\n",
                    "    rate_hz: 10\n",
                ),
            );
        let contract = manifest(&yaml);
        let report = run(&census, &contract, &no_waivers());

        let phantom: Vec<&Row> = report.of(Verdict::Phantom).collect();
        assert_eq!(phantom.len(), 1, "{}", report.render());
        assert_eq!(phantom[0].name, "/system/phantom");
        assert_eq!(phantom[0].key(), "mrm_handler:sub:/system/phantom");
        assert_eq!(report.errors(), 1, "{}", report.render());

        // The OVER direction is waivable per row, in system.toml and never in
        // the contract.
        let mut waivers = BTreeMap::new();
        waivers.insert(
            "mrm_handler:sub:/system/phantom".to_string(),
            "created only in the diagnostics build".to_string(),
        );
        let waived = run(&census, &contract, &waivers);
        assert_eq!(waived.errors(), 0, "{}", waived.render());
        assert_eq!(waived.waived(), 1);
        assert!(
            waived
                .render()
                .contains("created only in the diagnostics build"),
            "a waived row still prints, with its reason: {}",
            waived.render()
        );
    }

    /// E3c: declared under the node, wired to no topic. The resolver drops it
    /// and nothing downstream ever sees it.
    #[test]
    fn e3c_a_declared_endpoint_wired_to_no_topic_is_unwired() {
        let census = island_census();
        let yaml = ISLAND_CONTRACT.replace(
            "      gear_cmd_in: { min_rate_hz: 10, qos: { depth: 1 } }\n",
            "      gear_cmd_in: { min_rate_hz: 10, qos: { depth: 1 } }\n      dangling: \
             { min_rate_hz: 10, qos: { depth: 1 } }\n",
        );
        let contract = manifest(&yaml);
        let report = run(&census, &contract, &no_waivers());

        let unwired: Vec<&Row> = report.of(Verdict::Unwired).collect();
        assert_eq!(unwired.len(), 1, "{}", report.render());
        assert_eq!(unwired[0].name, "dangling");
        assert!(
            unwired[0].detail.contains("the resolver drops it"),
            "{}",
            unwired[0].detail
        );
        assert!(
            !unwired[0].verdict.waivable(),
            "a waiver would stand behind a statement that names a topic the contract does not \
             declare"
        );
        assert_eq!(report.errors(), 1, "{}", report.render());
    }

    /// E4: the contract prices the wrong message type.
    #[test]
    fn e4_a_type_the_code_does_not_use_is_a_type_mismatch() {
        let census = island_census();
        let yaml = ISLAND_CONTRACT.replace(
            "  /localization/kinematic_state:\n    type: nav_msgs/msg/Odometry",
            "  /localization/kinematic_state:\n    type: nav_msgs/msg/Path",
        );
        let contract = manifest(&yaml);
        let report = run(&census, &contract, &no_waivers());

        let rows: Vec<&Row> = report.of(Verdict::TypeMismatch).collect();
        assert_eq!(rows.len(), 1, "{}", report.render());
        assert!(
            rows[0].detail.contains("nav_msgs/msg/Odometry")
                && rows[0].detail.contains("nav_msgs/msg/Path"),
            "both types are named: {}",
            rows[0].detail
        );
        assert_eq!(report.errors(), 1, "{}", report.render());
    }

    /// E2a: the declared depth is what the arena is sized from, so both
    /// numbers belong in the refusal.
    #[test]
    fn e2a_a_depth_the_code_does_not_pass_is_a_depth_mismatch_naming_both() {
        let census = island_census();
        let yaml = ISLAND_CONTRACT.replace(
            "      kinematic_state: { min_rate_hz: 10, qos: { depth: 1 } }",
            "      kinematic_state: { min_rate_hz: 10, qos: { depth: 10 } }",
        );
        let contract = manifest(&yaml);
        let report = run(&census, &contract, &no_waivers());

        let rows: Vec<&Row> = report.of(Verdict::DepthMismatch).collect();
        assert_eq!(rows.len(), 1, "{}", report.render());
        assert!(
            rows[0].detail.contains("depth 1") && rows[0].detail.contains("declares 10"),
            "both numbers: {}",
            rows[0].detail
        );
        assert_eq!(report.errors(), 1, "{}", report.render());
    }

    #[test]
    fn a_timer_at_a_period_the_contract_does_not_declare_is_a_period_mismatch() {
        let mut census = island_census();
        census["census"]["nodes"][3]["timers"][0]["period_ms"] = json!(50);
        let contract = manifest(ISLAND_CONTRACT);
        let report = run(&census, &contract, &no_waivers());

        let rows: Vec<&Row> = report.of(Verdict::PeriodMismatch).collect();
        assert_eq!(rows.len(), 1, "{}", report.render());
        assert_eq!(rows[0].node, "stop_mode_operator");
        assert!(
            rows[0].detail.contains("50 ms") && rows[0].detail.contains("33 ms"),
            "{}",
            rows[0].detail
        );
    }

    #[test]
    fn a_parameter_the_contract_does_not_declare_refuses_one_build_before_the_board() {
        let mut census = island_census();
        census["census"]["parameters"]
            .as_array_mut()
            .expect("the parameter array")
            .push(param("mrm_handler", "undeclared_knob", "integer"));
        let contract = manifest(ISLAND_CONTRACT);
        let report = run(&census, &contract, &no_waivers());

        let rows: Vec<&Row> = report.of(Verdict::ParamMissingInContract).collect();
        assert_eq!(rows.len(), 1, "{}", report.render());
        assert!(
            rows[0]
                .remedy
                .contains("undeclared_knob: { type: integer }"),
            "{}",
            rows[0].remedy
        );
        assert_eq!(report.errors(), 1, "{}", report.render());
    }

    #[test]
    fn a_contract_parameter_the_code_never_declares_is_a_waivable_param_phantom() {
        let census = island_census();
        let yaml = ISLAND_CONTRACT.replace(
            "      use_pull_over: { type: bool }\n",
            "      use_pull_over: { type: bool }\n      never_declared: { type: bool }\n",
        );
        let contract = manifest(&yaml);
        let report = run(&census, &contract, &no_waivers());

        let rows: Vec<&Row> = report.of(Verdict::ParamPhantom).collect();
        assert_eq!(rows.len(), 1, "{}", report.render());
        assert_eq!(rows[0].key(), "mrm_handler:param:never_declared");
        assert!(rows[0].verdict.waivable());
    }

    #[test]
    fn a_parameter_whose_type_disagrees_is_a_param_type_mismatch() {
        let census = island_census();
        let yaml = ISLAND_CONTRACT.replace(
            "      use_pull_over: { type: bool }",
            "      use_pull_over: { type: integer }",
        );
        let contract = manifest(&yaml);
        let report = run(&census, &contract, &no_waivers());

        let rows: Vec<&Row> = report.of(Verdict::ParamTypeMismatch).collect();
        assert_eq!(rows.len(), 1, "{}", report.render());
        assert!(
            rows[0].detail.contains("bool") && rows[0].detail.contains("integer"),
            "{}",
            rows[0].detail
        );
    }

    /// Evidence absent is not evidence of absence -- until the merge queue
    /// says it is.
    #[test]
    fn a_node_the_census_never_ran_is_a_warning_and_strict_makes_it_an_error() {
        let mut census = island_census();
        census["census"]["nodes"]
            .as_array_mut()
            .expect("the node array")
            .remove(3);
        census["census"]["parameters"]
            .as_array_mut()
            .expect("the parameter array")
            .retain(|p| p["node"] != json!("stop_mode_operator"));
        let contract = manifest(ISLAND_CONTRACT);
        let report = run(&census, &contract, &no_waivers());

        assert_eq!(report.errors(), 0, "{}", report.render());
        // Three subs, four publishers and three parameters of the node that
        // was not run.
        assert_eq!(report.warnings(), 10, "{}", report.render());
        assert!(
            report
                .of(Verdict::Unobserved)
                .all(|r| r.node == "stop_mode_operator")
        );

        let strict = check(Inputs {
            census: &census,
            contract: &contract,
            contract_path: CONTRACT_PATH.to_string(),
            inventory: None,
            waivers: &no_waivers(),
            strict: true,
        });
        assert_eq!(strict.errors(), 10, "{}", strict.render());
        assert!(strict.refuses());
    }

    /// The runtime's own service families are registered by the ENTRY, not by
    /// node code, and no contract can declare them. Excluded by the interface
    /// kind, so a user service under one of those names is still checked.
    #[test]
    fn the_parameter_services_are_excluded_by_interface_kind() {
        let mut census = island_census();
        for name in [
            "get_parameters",
            "set_parameters",
            "list_parameters",
            "describe_parameters",
            "get_parameter_types",
            "set_parameters_atomically",
        ] {
            let row = endpoint(
                "unresolved_name",
                &format!("/mrm_handler/{name}"),
                "rcl_interfaces/srv/GetParameters",
                10,
            );
            census["census"]["nodes"][2]["services"]
                .as_array_mut()
                .expect("the service array")
                .push(row);
        }
        let contract = manifest(ISLAND_CONTRACT);
        let report = run(&census, &contract, &no_waivers());
        assert_eq!(report.errors(), 0, "{}", report.render());
        assert_eq!(report.confirmed(), 54, "{}", report.render());
    }

    #[test]
    fn a_guard_condition_is_a_note_and_not_a_delta() {
        let mut census = island_census();
        census["census"]["nodes"][2]["timers"]
            .as_array_mut()
            .expect("the timer array")
            .push(json!({ "id": "gc", "kind": "guard_condition", "period_ms": 0 }));
        let contract = manifest(ISLAND_CONTRACT);
        let report = run(&census, &contract, &no_waivers());
        assert_eq!(report.errors(), 0, "{}", report.render());
        assert!(
            report.notes.iter().any(|n| n.contains("guard condition")),
            "{:?}",
            report.notes
        );
    }

    /// The waivers live in `system.toml` because the contract's parser rejects
    /// a key it does not know -- which is the property that makes
    /// `[census.waive]` unrepresentable there rather than merely discouraged.
    #[test]
    fn the_contract_parser_rejects_a_census_block() {
        let yaml = format!("{ISLAND_CONTRACT}\ncensus:\n  waive:\n    a: {{ reason: no }}\n");
        let err = ros_launch_manifest_types::parse_manifest_str(&yaml)
            .expect_err("an unknown key is a parse error, not a skip")
            .to_string();
        assert!(err.contains("census"), "{err}");
    }

    /// The derived view is not a fourth opinion: it is the record of what the
    /// pools were sized from, so a refusal can say what the image would have
    /// been built with.
    #[test]
    fn the_inventory_says_what_the_image_would_have_been_sized_with() {
        let census = island_census();
        let yaml = ISLAND_CONTRACT.replace(
            "      operation_mode_state: { min_rate_hz: 10, qos: { depth: 1 } }\n",
            "",
        );
        let contract = manifest(&yaml);
        let inventory = json!({ "status": "derived", "max_cbs": 18, "components": [] });
        let report = check(Inputs {
            census: &census,
            contract: &contract,
            contract_path: CONTRACT_PATH.to_string(),
            inventory: Some(&inventory),
            waivers: &no_waivers(),
            strict: false,
        });
        assert!(
            report.notes.iter().any(|n| n.contains("max_cbs = 18")),
            "{:?}",
            report.notes
        );
    }

    /// A bare recorder document -- what the binary itself writes, without the
    /// verb's provenance wrapper -- reads the same.
    #[test]
    fn a_bare_recorder_document_is_accepted_too() {
        let census = island_census();
        let bare = census.get("census").expect("the recorder document").clone();
        let contract = manifest(ISLAND_CONTRACT);
        let report = run(&bare, &contract, &no_waivers());
        assert_eq!(report.confirmed(), 54, "{}", report.render());
        assert_eq!(report.errors(), 0, "{}", report.render());
    }
}
