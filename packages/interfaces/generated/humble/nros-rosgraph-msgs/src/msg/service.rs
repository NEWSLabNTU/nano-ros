// nros message type - pure Rust, no_std compatible
// Package: rosgraph_msgs
// Message: Service

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Service message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Service {
    pub name: heapless::String<256>,
    pub request_type: crate::msg::InterfaceType,
    pub request_qos: crate::msg::QoSProfile,
    pub response_type: crate::msg::InterfaceType,
    pub response_qos: crate::msg::QoSProfile,
}

impl Serialize for Service {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.name.as_str())?;
        self.request_type.serialize(writer)?;
        self.request_qos.serialize(writer)?;
        self.response_type.serialize(writer)?;
        self.response_qos.serialize(writer)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Service {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            request_type: Deserialize::deserialize(reader)?,
            request_qos: Deserialize::deserialize(reader)?,
            response_type: Deserialize::deserialize(reader)?,
            response_qos: Deserialize::deserialize(reader)?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Service {
    const TYPE_NAME: &'static str = "rosgraph_msgs::msg::dds_::Service_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
    // Issue 1179 -- the type's bound on `RosMessage`, the one trait every
    // backend's registration can read. Computed from the schema below, not
    // copied, so it cannot drift from it.
    const MAX_SERIALIZED_BOUND: Option<usize> = ::nros_serdes::size::max_serialized_bound::<Self>();
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_REQUEST_TYPE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::InterfaceType as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::InterfaceType as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_REQUEST_QOS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::QoSProfile as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::QoSProfile as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_RESPONSE_TYPE: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::InterfaceType as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::InterfaceType as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_RESPONSE_QOS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::QoSProfile as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::QoSProfile as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Service {
    const TYPE_NAME: &'static str = "rosgraph_msgs/msg/Service";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "name",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Service, name),
        },
        ::nros_serdes::Field {
            name: "request_type",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_REQUEST_TYPE),
            offset: ::core::mem::offset_of!(Service, request_type),
        },
        ::nros_serdes::Field {
            name: "request_qos",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_REQUEST_QOS),
            offset: ::core::mem::offset_of!(Service, request_qos),
        },
        ::nros_serdes::Field {
            name: "response_type",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_RESPONSE_TYPE),
            offset: ::core::mem::offset_of!(Service, response_type),
        },
        ::nros_serdes::Field {
            name: "response_qos",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_RESPONSE_QOS),
            offset: ::core::mem::offset_of!(Service, response_qos),
        },
    ];
}
