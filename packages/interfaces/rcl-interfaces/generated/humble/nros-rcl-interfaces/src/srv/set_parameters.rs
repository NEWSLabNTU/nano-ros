// nros service type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Service: SetParameters

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// SetParameters request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetParametersRequest {
    pub parameters: heapless::Vec<crate::msg::Parameter, 64>,
}

impl Serialize for SetParametersRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.parameters.len() as u32)?;
        for item in &self.parameters {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for SetParametersRequest {
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

impl RosMessage for SetParametersRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::SetParameters_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Request) ───────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const REQ_NESTED_PARAMETERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Parameter as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Parameter as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const REQ_FT_PARAMETERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&REQ_NESTED_PARAMETERS);
impl ::nros_serdes::Message for SetParametersRequest {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/SetParameters_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "parameters",
        ty: ::nros_serdes::FieldType::Sequence(&REQ_FT_PARAMETERS_ELEM),
        offset: ::core::mem::offset_of!(SetParametersRequest, parameters),
    }];
}

/// SetParameters response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetParametersResponse {
    pub results: heapless::Vec<crate::msg::SetParametersResult, 64>,
}

impl Serialize for SetParametersResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.results.len() as u32)?;
        for item in &self.results {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for SetParametersResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            results: {
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

impl RosMessage for SetParametersResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces::srv::dds_::SetParameters_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema (Response) ──────────────────

#[allow(non_upper_case_globals)]
pub const RESP_NESTED_RESULTS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::SetParametersResult as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::SetParametersResult as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const RESP_FT_RESULTS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&RESP_NESTED_RESULTS);
impl ::nros_serdes::Message for SetParametersResponse {
    const TYPE_NAME: &'static str = "rcl_interfaces/srv/SetParameters_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "results",
        ty: ::nros_serdes::FieldType::Sequence(&RESP_FT_RESULTS_ELEM),
        offset: ::core::mem::offset_of!(SetParametersResponse, results),
    }];
}

/// SetParameters service definition
pub struct SetParameters;

impl RosService for SetParameters {
    type Request = SetParametersRequest;
    type Reply = SetParametersResponse;

    const SERVICE_NAME: &'static str = "rcl_interfaces::srv::dds_::SetParameters_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
