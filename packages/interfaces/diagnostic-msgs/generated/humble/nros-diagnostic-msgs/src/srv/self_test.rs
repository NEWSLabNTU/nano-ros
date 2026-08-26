// nros service type - pure Rust, no_std compatible
// Package: diagnostic_msgs
// Service: SelfTest

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// SelfTest request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelfTestRequest {}

impl Serialize for SelfTestRequest {
    // Empty request - no fields to serialize
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        let __dh = writer.begin_dheader()?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for SelfTestRequest {
    // Empty request - no fields to deserialize
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        reader.end_dheader(__dh)?;
        Ok(Self {})
    }
}

impl RosMessage for SelfTestRequest {
    const TYPE_NAME: &'static str = "diagnostic_msgs::srv::dds_::SelfTest_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Request) âââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for SelfTestRequest {
    const TYPE_NAME: &'static str = "diagnostic_msgs/srv/SelfTest_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[];
}

/// SelfTest response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelfTestResponse {
    pub id: heapless::String<256>,
    pub passed: u8,
    pub status: heapless::Vec<crate::msg::DiagnosticStatus, 64>,
}

impl Serialize for SelfTestResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.id.as_str())?;
        writer.write_u8(self.passed)?;
        writer.write_u32(self.status.len() as u32)?;
        for item in &self.status {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for SelfTestResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            id: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            passed: reader.read_u8()?,
            status: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for SelfTestResponse {
    const TYPE_NAME: &'static str = "diagnostic_msgs::srv::dds_::SelfTest_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Response) ââââââââââââââââââ

#[allow(non_upper_case_globals)]
pub const RESP_NESTED_STATUS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::DiagnosticStatus as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::DiagnosticStatus as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const RESP_FT_STATUS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&RESP_NESTED_STATUS);
impl ::nros_serdes::Message for SelfTestResponse {
    const TYPE_NAME: &'static str = "diagnostic_msgs/srv/SelfTest_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "id",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(SelfTestResponse, id),
        },
        ::nros_serdes::Field {
            name: "passed",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(SelfTestResponse, passed),
        },
        ::nros_serdes::Field {
            name: "status",
            ty: ::nros_serdes::FieldType::Sequence(&RESP_FT_STATUS_ELEM),
            offset: ::core::mem::offset_of!(SelfTestResponse, status),
        },
    ];
}

/// SelfTest service definition
pub struct SelfTest;

impl RosService for SelfTest {
    type Request = SelfTestRequest;
    type Reply = SelfTestResponse;

    const SERVICE_NAME: &'static str = "diagnostic_msgs::srv::dds_::SelfTest_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
