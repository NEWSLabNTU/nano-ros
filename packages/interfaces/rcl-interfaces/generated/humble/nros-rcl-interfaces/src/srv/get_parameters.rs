// nros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: GetParameters

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// GetParameters request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetParametersRequest {
    pub names: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for GetParametersRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.names.len() as u32)?;
        for item in &self.names {
            writer.write_string(item.as_str())?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for GetParametersRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            names: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    let s = reader.read_string()?;
                    let elem =
                        heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?;
                    vec.push(elem).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for GetParametersRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::GetParameters_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Request) âââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const REQ_FT_NAMES_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::String;
impl ::nros_serdes::Message for GetParametersRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/GetParameters_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "names",
        ty: ::nros_serdes::FieldType::Sequence(&REQ_FT_NAMES_ELEM),
        offset: ::core::mem::offset_of!(GetParametersRequest, names),
    }];
}

/// GetParameters response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetParametersResponse {
    pub values: heapless::Vec<crate::msg::ParameterValue, 64>,
}

impl Serialize for GetParametersResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.values.len() as u32)?;
        for item in &self.values {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for GetParametersResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            values: {
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

impl RosMessage for GetParametersResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::GetParameters_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Response) ââââââââââââââââââ

#[allow(non_upper_case_globals)]
pub const RESP_NESTED_VALUES: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::ParameterValue as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::ParameterValue as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const RESP_FT_VALUES_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&RESP_NESTED_VALUES);
impl ::nros_serdes::Message for GetParametersResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/GetParameters_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "values",
        ty: ::nros_serdes::FieldType::Sequence(&RESP_FT_VALUES_ELEM),
        offset: ::core::mem::offset_of!(GetParametersResponse, values),
    }];
}

/// GetParameters service definition
pub struct GetParameters;

impl RosService for GetParameters {
    type Request = GetParametersRequest;
    type Reply = GetParametersResponse;

    const SERVICE_NAME: &'static str = "rcl_interfaces::srv::dds_::GetParameters_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
