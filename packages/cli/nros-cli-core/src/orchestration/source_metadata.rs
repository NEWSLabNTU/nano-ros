use serde::{Deserialize, Serialize};

use super::schema::{InterfaceRef, ParameterValue, QosProfile, SourceLocation, SourceName};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMetadata {
    pub version: u32,
    pub package: String,
    pub component: String,
    pub language: ComponentLanguage,
    pub executable: Option<String>,
    pub exported_symbol: Option<String>,
    pub nodes: Vec<SourceNode>,
    pub callbacks: Vec<SourceCallback>,
    pub parameters: Vec<SourceParameter>,
    pub trace: SourceMetadataTrace,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentLanguage {
    Rust,
    C,
    Cpp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceNode {
    pub id: String,
    pub unresolved_name: SourceName,
    pub namespace: Option<String>,
    pub publishers: Vec<SourcePublisher>,
    pub subscribers: Vec<SourceSubscriber>,
    pub timers: Vec<SourceTimer>,
    pub services: Vec<SourceService>,
    pub actions: Vec<SourceAction>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePublisher {
    pub id: String,
    pub unresolved_topic: SourceName,
    pub interface: InterfaceRef,
    pub qos: QosProfile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSubscriber {
    pub id: String,
    pub unresolved_topic: SourceName,
    pub interface: InterfaceRef,
    pub qos: QosProfile,
    pub callback: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTimer {
    pub id: String,
    pub period_ms: u64,
    pub callback: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceService {
    pub id: String,
    pub unresolved_name: SourceName,
    pub interface: InterfaceRef,
    pub callback: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAction {
    pub id: String,
    pub unresolved_name: SourceName,
    pub interface: InterfaceRef,
    pub goal_callback: String,
    pub cancel_callback: String,
    pub accepted_callback: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCallback {
    pub id: String,
    pub kind: CallbackKind,
    pub group: Option<String>,
    pub effects: Vec<CallbackEffect>,
    pub source: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackKind {
    Timer,
    Subscription,
    Service,
    ActionGoal,
    ActionCancel,
    ActionAccepted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallbackEffect {
    pub kind: CallbackEffectKind,
    /// Source entity ID affected by the callback.
    pub entity: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackEffectKind {
    Publishes,
    ReadsParameter,
    WritesParameter,
    SendsServiceRequest,
    SendsServiceReply,
    SendsActionGoal,
    SendsActionFeedback,
    SendsActionResult,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceParameter {
    pub node: String,
    pub name: String,
    pub default: ParameterValue,
    pub read_only: bool,
    pub source: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMetadataTrace {
    pub generator: String,
    pub package_manifest: String,
    pub source_artifacts: Vec<String>,
}
