// nros service type - pure Rust, no_std compatible
// Package: lifecycle_msgs
// Service: GetAvailableStates

use nros_core::{Deserialize, RosMessage, RosService, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// GetAvailableStates request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetAvailableStatesRequest {}

impl Serialize for GetAvailableStatesRequest {
    // Empty request - no fields to serialize
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        let __dh = writer.begin_dheader()?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for GetAvailableStatesRequest {
    // Empty request - no fields to deserialize
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        reader.end_dheader(__dh)?;
        Ok(Self {})
    }
}

impl RosMessage for GetAvailableStatesRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableStates_Request_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Request) âââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for GetAvailableStatesRequest {
    const TYPE_NAME: &'static str = "lifecycle_msgs/srv/GetAvailableStates_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[];
}

/// GetAvailableStates response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetAvailableStatesResponse {
    pub available_states: heapless::Vec<crate::msg::State, 64>,
}

impl Serialize for GetAvailableStatesResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.available_states.len() as u32)?;
        for item in &self.available_states {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for GetAvailableStatesResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            available_states: {
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

impl RosMessage for GetAvailableStatesResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableStates_Response_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema (Response) ââââââââââââââââââ

#[allow(non_upper_case_globals)]
pub const RESP_NESTED_AVAILABLE_STATES: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::State as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::State as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const RESP_FT_AVAILABLE_STATES_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&RESP_NESTED_AVAILABLE_STATES);
impl ::nros_serdes::Message for GetAvailableStatesResponse {
    const TYPE_NAME: &'static str = "lifecycle_msgs/srv/GetAvailableStates_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "available_states",
        ty: ::nros_serdes::FieldType::Sequence(&RESP_FT_AVAILABLE_STATES_ELEM),
        offset: ::core::mem::offset_of!(GetAvailableStatesResponse, available_states),
    }];
}

/// GetAvailableStates service definition
pub struct GetAvailableStates;

impl RosService for GetAvailableStates {
    type Request = GetAvailableStatesRequest;
    type Reply = GetAvailableStatesResponse;

    const SERVICE_NAME: &'static str = "lifecycle_msgs::srv::dds_::GetAvailableStates_";
    const SERVICE_HASH: &'static str = "TypeHashNotSupported";
}
