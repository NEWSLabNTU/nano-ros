// nros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: ListParameters

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};
pub const DEPTH_RECURSIVE: u64 = 0;

/// ListParameters request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListParametersRequest {
    pub prefixes: heapless::Vec<heapless::String<256>, 64>,
    pub depth: u64,
}

impl Serialize for ListParametersRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.prefixes.len() as u32)?;
        for item in &self.prefixes {
            writer.write_string(item.as_str())?;
        }
        writer.write_u64(self.depth)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ListParametersRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            prefixes: {
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
            depth: reader.read_u64()?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ListParametersRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::ListParameters_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Request) âââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const REQ_FT_PREFIXES_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::String;
impl ::nros_serdes::Message for ListParametersRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/ListParameters_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "prefixes",
            ty: ::nros_serdes::FieldType::Sequence(&REQ_FT_PREFIXES_ELEM),
            offset: ::core::mem::offset_of!(ListParametersRequest, prefixes),
        },
        ::nros_serdes::Field {
            name: "depth",
            ty: ::nros_serdes::FieldType::Uint64,
            offset: ::core::mem::offset_of!(ListParametersRequest, depth),
        },
    ];
}

/// ListParameters response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListParametersResponse {
    pub result: crate::msg::ListParametersResult,
}

impl Serialize for ListParametersResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        self.result.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ListParametersResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            result: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ListParametersResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::ListParameters_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Response) ââââââââââââââââââ

#[allow(non_upper_case_globals)]
pub const RESP_NESTED_RESULT: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::ListParametersResult as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::ListParametersResult as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for ListParametersResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/ListParameters_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "result",
        ty: ::nros_serdes::FieldType::Nested(&RESP_NESTED_RESULT),
        offset: ::core::mem::offset_of!(ListParametersResponse, result),
    }];
}

/// ListParameters service definition
pub struct ListParameters;

impl RosService for ListParameters {
    type Request = ListParametersRequest;
    type Reply = ListParametersResponse;

    const SERVICE_NAME: &'static str = "rcl_interfaces::srv::dds_::ListParameters_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
