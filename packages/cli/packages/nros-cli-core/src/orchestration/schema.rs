use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const SOURCE_METADATA_VERSION: u32 = 1;
pub const COMPONENT_CONFIG_VERSION: u32 = 1;
// v2 (Phase 172.B): additive `NrosPlan.callback_chains` (inferred callback
// execution chains from the topic dataflow graph). Old plans without the field
// still deserialize via `#[serde(default)]`.
pub const PLAN_VERSION: u32 = 2;

pub type ParameterTable = BTreeMap<String, ParameterValue>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterfaceRef {
    pub package: String,
    pub name: String,
    pub kind: InterfaceKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceKind {
    Message,
    Service,
    Action,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParameterValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    BoolArray(Vec<bool>),
    IntegerArray(Vec<i64>),
    FloatArray(Vec<f64>),
    StringArray(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QosProfile {
    pub reliability: QosReliability,
    pub durability: QosDurability,
    pub history: QosHistory,
    pub depth: u32,
    pub deadline_ms: Option<u64>,
    pub lifespan_ms: Option<u64>,
    pub liveliness: QosLiveliness,
    pub liveliness_lease_duration_ms: Option<u64>,
    /// Vendor/backend QoS keys that are not part of schema v1 yet.
    pub extensions: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QosReliability {
    SystemDefault,
    Reliable,
    BestEffort,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QosDurability {
    SystemDefault,
    Volatile,
    TransientLocal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QosHistory {
    SystemDefault,
    KeepLast,
    KeepAll,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QosLiveliness {
    SystemDefault,
    Automatic,
    ManualByTopic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLocation {
    pub artifact: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemapRule {
    pub from: String,
    pub to: String,
}

/// Phase 211.E — `<set_env>` / `<env>` declaration carried onto each instance.
/// Parallel to [`RemapRule`]: a `{name, value}` pair sourced from the parser's
/// `record.node[*].env` field and surfaced on `PlanInstance.env` so the deploy
/// stage can pass them to the spawned process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvDecl {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceName {
    /// Original source spelling, kept unresolved for launch namespace/remap logic.
    pub value: String,
    pub kind: SourceNameKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceNameKind {
    Absolute,
    Relative,
    Private,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedClass {
    /// Maps to runtime best-effort scheduling.
    BestEffort,
    /// Maps to runtime FIFO fixed-priority scheduling.
    RealTime,
    /// Maps to FIFO with period/window metadata consumed by generated bindings.
    TimeTriggered,
    /// Maps to FIFO for interrupt-adjacent callbacks; platform code owns IRQ binding.
    Interrupt,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadlinePolicy {
    Ignore,
    Warn,
    Skip,
    Fault,
}
