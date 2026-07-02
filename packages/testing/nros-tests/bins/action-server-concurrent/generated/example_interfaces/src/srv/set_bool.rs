// nros service type - pure Rust, no_std compatible
// Package: example_interfaces
// Service: SetBool

use nros_core::{RosMessage, RosService, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// SetBool request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetBoolRequest {
    pub data: bool,
}

impl Serialize for SetBoolRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_bool(self.data)?;
        Ok(())
    }
}

impl Deserialize for SetBoolRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_bool()?,
        })
    }
}

impl RosMessage for SetBoolRequest {
    const TYPE_NAME: &'static str = "example_interfaces::srv::dds_::SetBool_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Request) ───────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for SetBoolRequest {
    const TYPE_NAME: &'static str = "example_interfaces/srv/SetBool_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "data",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(SetBoolRequest, data),
        },
];
}

/// SetBool response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetBoolResponse {
    pub success: bool,
    pub message: heapless::String<256>,
}

impl Serialize for SetBoolResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_bool(self.success)?;
        writer.write_string(self.message.as_str())?;
        Ok(())
    }
}

impl Deserialize for SetBoolResponse {
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

impl RosMessage for SetBoolResponse {
    const TYPE_NAME: &'static str = "example_interfaces::srv::dds_::SetBool_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Response) ──────────────────

impl ::nros_serdes::Message for SetBoolResponse {
    const TYPE_NAME: &'static str = "example_interfaces/srv/SetBool_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "success",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(SetBoolResponse, success),
        },
        ::nros_serdes::Field {
            name: "message",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(SetBoolResponse, message),
        },
];
}

/// SetBool service definition
pub struct SetBool;

impl RosService for SetBool {
    type Request = SetBoolRequest;
    type Reply = SetBoolResponse;

    const SERVICE_NAME: &'static str = "example_interfaces::srv::dds_::SetBool_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}