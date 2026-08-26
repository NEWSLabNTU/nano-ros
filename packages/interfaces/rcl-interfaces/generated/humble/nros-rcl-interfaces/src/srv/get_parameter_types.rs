// nros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: GetParameterTypes

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// GetParameterTypes request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetParameterTypesRequest {
    pub names: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for GetParameterTypesRequest {
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

impl Deserialize for GetParameterTypesRequest {
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

impl RosMessage for GetParameterTypesRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::GetParameterTypes_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Request) âââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const REQ_FT_NAMES_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::String;
impl ::nros_serdes::Message for GetParameterTypesRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/GetParameterTypes_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "names",
        ty: ::nros_serdes::FieldType::Sequence(&REQ_FT_NAMES_ELEM),
        offset: ::core::mem::offset_of!(GetParameterTypesRequest, names),
    }];
}

/// GetParameterTypes response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetParameterTypesResponse {
    pub types: heapless::Vec<u8, 64>,
}

impl Serialize for GetParameterTypesResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.types.len() as u32)?;
        for item in &self.types {
            writer.write_u8(*item)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for GetParameterTypesResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            types: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_u8()?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for GetParameterTypesResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::GetParameterTypes_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Response) ââââââââââââââââââ

#[allow(non_upper_case_globals)]
pub const RESP_FT_TYPES_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Uint8;
impl ::nros_serdes::Message for GetParameterTypesResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/GetParameterTypes_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "types",
        ty: ::nros_serdes::FieldType::Sequence(&RESP_FT_TYPES_ELEM),
        offset: ::core::mem::offset_of!(GetParameterTypesResponse, types),
    }];
}

/// GetParameterTypes service definition
pub struct GetParameterTypes;

impl RosService for GetParameterTypes {
    type Request = GetParameterTypesRequest;
    type Reply = GetParameterTypesResponse;

    const SERVICE_NAME: &'static str = "rcl_interfaces::srv::dds_::GetParameterTypes_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
