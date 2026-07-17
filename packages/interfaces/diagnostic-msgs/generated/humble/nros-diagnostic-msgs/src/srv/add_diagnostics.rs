// nros service type - pure Rust, no_std compatible
// Package: diagnostic_msgs
// Service: AddDiagnostics

use nros_core::{RosMessage, RosService, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// AddDiagnostics request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AddDiagnosticsRequest {
    pub load_namespace: heapless::String<256>,
}

impl Serialize for AddDiagnosticsRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_string(self.load_namespace.as_str())?;
        Ok(())
    }
}

impl Deserialize for AddDiagnosticsRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            load_namespace: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        })
    }
}

impl RosMessage for AddDiagnosticsRequest {
    const TYPE_NAME: &'static str = "diagnostic_msgs::srv::dds_::AddDiagnostics_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Request) âââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for AddDiagnosticsRequest {
    const TYPE_NAME: &'static str = "diagnostic_msgs/srv/AddDiagnostics_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "load_namespace",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(AddDiagnosticsRequest, load_namespace),
        },
];
}

/// AddDiagnostics response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AddDiagnosticsResponse {
    pub success: bool,
    pub message: heapless::String<256>,
}

impl Serialize for AddDiagnosticsResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_bool(self.success)?;
        writer.write_string(self.message.as_str())?;
        Ok(())
    }
}

impl Deserialize for AddDiagnosticsResponse {
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

impl RosMessage for AddDiagnosticsResponse {
    const TYPE_NAME: &'static str = "diagnostic_msgs::srv::dds_::AddDiagnostics_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Response) ââââââââââââââââââ

impl ::nros_serdes::Message for AddDiagnosticsResponse {
    const TYPE_NAME: &'static str = "diagnostic_msgs/srv/AddDiagnostics_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "success",
            ty: ::nros_serdes::FieldType::Bool,
            offset: ::core::mem::offset_of!(AddDiagnosticsResponse, success),
        },
        ::nros_serdes::Field {
            name: "message",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(AddDiagnosticsResponse, message),
        },
];
}

/// AddDiagnostics service definition
pub struct AddDiagnostics;

impl RosService for AddDiagnostics {
    type Request = AddDiagnosticsRequest;
    type Reply = AddDiagnosticsResponse;

    const SERVICE_NAME: &'static str = "diagnostic_msgs::srv::dds_::AddDiagnostics_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}