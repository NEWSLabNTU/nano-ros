// nros service type - pure Rust, no_std compatible
// Package: example_interfaces
// Service: Trigger

use nros_core::{RosMessage, RosService, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Trigger request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriggerRequest {
}

impl Serialize for TriggerRequest {
    // Empty request - no fields to serialize
    fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {
        Ok(())
    }
}

impl Deserialize for TriggerRequest {
    // Empty request - no fields to deserialize
    fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {})
    }
}

impl RosMessage for TriggerRequest {
    const TYPE_NAME: &'static str = "example_interfaces::srv::dds_::Trigger_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Request) ───────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for TriggerRequest {
    const TYPE_NAME: &'static str = "example_interfaces/srv/Trigger_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[
];
}

/// Trigger response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriggerResponse {
    pub success: bool,
    pub message: heapless::String<256>,
}

impl Serialize for TriggerResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_bool(self.success)?;
        writer.write_string(self.message.as_str())?;
        Ok(())
    }
}

impl Deserialize for TriggerResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            success: reader.read_bool()?,
            message: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        })
    }
}

impl RosMessage for TriggerResponse {
    const TYPE_NAME: &'static str = "example_interfaces::srv::dds_::Trigger_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Response) ──────────────────

impl ::nros_serdes::Message for TriggerResponse {
    const TYPE_NAME: &'static str = "example_interfaces/srv/Trigger_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "success",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(TriggerResponse, success),
        },
        ::nros_serdes::Field {
            name: "message",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(TriggerResponse, message),
        },
];
}

/// Trigger service definition
pub struct Trigger;

impl RosService for Trigger {
    type Request = TriggerRequest;
    type Reply = TriggerResponse;

    const SERVICE_NAME: &'static str = "example_interfaces::srv::dds_::Trigger_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}