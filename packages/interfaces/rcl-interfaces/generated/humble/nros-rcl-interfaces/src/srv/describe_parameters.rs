// nros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: DescribeParameters

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// DescribeParameters request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DescribeParametersRequest {
    pub names: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for DescribeParametersRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.names.len() as u32)?;
        for item in &self.names {
            writer.write_string(item.as_str())?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for DescribeParametersRequest {
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

impl RosMessage for DescribeParametersRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::DescribeParameters_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Request) ───────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const REQ_FT_NAMES_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::String;
impl ::nros_serdes::Message for DescribeParametersRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/DescribeParameters_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "names",
        ty: ::nros_serdes::FieldType::Sequence(&REQ_FT_NAMES_ELEM),
        offset: ::core::mem::offset_of!(DescribeParametersRequest, names),
    }];
}

/// DescribeParameters response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DescribeParametersResponse {
    pub descriptors: heapless::Vec<crate::msg::ParameterDescriptor, 64>,
}

impl Serialize for DescribeParametersResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.descriptors.len() as u32)?;
        for item in &self.descriptors {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for DescribeParametersResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            descriptors: {
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

impl RosMessage for DescribeParametersResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::DescribeParameters_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Response) ──────────────────

#[allow(non_upper_case_globals)]
pub const RESP_NESTED_DESCRIPTORS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::ParameterDescriptor as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::ParameterDescriptor as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const RESP_FT_DESCRIPTORS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&RESP_NESTED_DESCRIPTORS);
impl ::nros_serdes::Message for DescribeParametersResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/DescribeParameters_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "descriptors",
        ty: ::nros_serdes::FieldType::Sequence(&RESP_FT_DESCRIPTORS_ELEM),
        offset: ::core::mem::offset_of!(DescribeParametersResponse, descriptors),
    }];
}

/// DescribeParameters service definition
pub struct DescribeParameters;

impl RosService for DescribeParameters {
    type Request = DescribeParametersRequest;
    type Reply = DescribeParametersResponse;

    const SERVICE_NAME: &'static str = "rcl_interfaces::srv::dds_::DescribeParameters_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
