// nros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: SetParametersAtomically

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// SetParametersAtomically request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetParametersAtomicallyRequest {
    pub parameters: heapless::Vec<crate::msg::Parameter, 64>,
}

impl Serialize for SetParametersAtomicallyRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.parameters.len() as u32)?;
        for item in &self.parameters {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for SetParametersAtomicallyRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            parameters: {
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

impl RosMessage for SetParametersAtomicallyRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::SetParametersAtomically_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Request) âââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const REQ_NESTED_PARAMETERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Parameter as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Parameter as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const REQ_FT_PARAMETERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&REQ_NESTED_PARAMETERS);
impl ::nros_serdes::Message for SetParametersAtomicallyRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/SetParametersAtomically_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "parameters",
        ty: ::nros_serdes::FieldType::Sequence(&REQ_FT_PARAMETERS_ELEM),
        offset: ::core::mem::offset_of!(SetParametersAtomicallyRequest, parameters),
    }];
}

/// SetParametersAtomically response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetParametersAtomicallyResponse {
    pub result: crate::msg::SetParametersResult,
}

impl Serialize for SetParametersAtomicallyResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        self.result.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for SetParametersAtomicallyResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            result: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for SetParametersAtomicallyResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::SetParametersAtomically_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Response) ââââââââââââââââââ

#[allow(non_upper_case_globals)]
pub const RESP_NESTED_RESULT: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::SetParametersResult as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::SetParametersResult as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for SetParametersAtomicallyResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/SetParametersAtomically_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "result",
        ty: ::nros_serdes::FieldType::Nested(&RESP_NESTED_RESULT),
        offset: ::core::mem::offset_of!(SetParametersAtomicallyResponse, result),
    }];
}

/// SetParametersAtomically service definition
pub struct SetParametersAtomically;

impl RosService for SetParametersAtomically {
    type Request = SetParametersAtomicallyRequest;
    type Reply = SetParametersAtomicallyResponse;

    const SERVICE_NAME: &'static str = "rcl_interfaces::srv::dds_::SetParametersAtomically_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
