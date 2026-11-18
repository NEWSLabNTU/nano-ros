// nros message type - pure Rust, no_std compatible
// Package: rosgraph_msgs
// Message: Node

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Node message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub name: heapless::String<256>,
    pub parameters: heapless::Vec<nros_rcl_interfaces::msg::ParameterDescriptor, 64>,
    pub parameter_values: heapless::Vec<nros_rcl_interfaces::msg::ParameterValue, 64>,
    pub publishers: heapless::Vec<crate::msg::Topic, 64>,
    pub subscriptions: heapless::Vec<crate::msg::Topic, 64>,
    pub service_clients: heapless::Vec<crate::msg::Service, 64>,
    pub service_servers: heapless::Vec<crate::msg::Service, 64>,
    pub action_clients: heapless::Vec<crate::msg::Action, 64>,
    pub action_servers: heapless::Vec<crate::msg::Action, 64>,
}

impl Serialize for Node {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.name.as_str())?;
        writer.write_u32(self.parameters.len() as u32)?;
        for item in &self.parameters {
            item.serialize(writer)?;
        }
        writer.write_u32(self.parameter_values.len() as u32)?;
        for item in &self.parameter_values {
            item.serialize(writer)?;
        }
        writer.write_u32(self.publishers.len() as u32)?;
        for item in &self.publishers {
            item.serialize(writer)?;
        }
        writer.write_u32(self.subscriptions.len() as u32)?;
        for item in &self.subscriptions {
            item.serialize(writer)?;
        }
        writer.write_u32(self.service_clients.len() as u32)?;
        for item in &self.service_clients {
            item.serialize(writer)?;
        }
        writer.write_u32(self.service_servers.len() as u32)?;
        for item in &self.service_servers {
            item.serialize(writer)?;
        }
        writer.write_u32(self.action_clients.len() as u32)?;
        for item in &self.action_clients {
            item.serialize(writer)?;
        }
        writer.write_u32(self.action_servers.len() as u32)?;
        for item in &self.action_servers {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Node {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            parameters: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            parameter_values: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            publishers: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            subscriptions: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            service_clients: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            service_servers: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            action_clients: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?)
                        .map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
            action_servers: {
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

impl RosMessage for Node {
    const TYPE_NAME: &'static str = "rosgraph_msgs::msg::dds_::Node_";
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
pub const NESTED_PARAMETERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <nros_rcl_interfaces::msg::ParameterDescriptor as ::nros_serdes::Message>::TYPE_NAME,
    fields: <nros_rcl_interfaces::msg::ParameterDescriptor as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_PARAMETERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_PARAMETERS);
#[allow(non_upper_case_globals)]
pub const NESTED_PARAMETER_VALUES: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <nros_rcl_interfaces::msg::ParameterValue as ::nros_serdes::Message>::TYPE_NAME,
    fields: <nros_rcl_interfaces::msg::ParameterValue as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_PARAMETER_VALUES_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_PARAMETER_VALUES);
#[allow(non_upper_case_globals)]
pub const NESTED_PUBLISHERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Topic as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Topic as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_PUBLISHERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_PUBLISHERS);
#[allow(non_upper_case_globals)]
pub const NESTED_SUBSCRIPTIONS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Topic as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Topic as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_SUBSCRIPTIONS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_SUBSCRIPTIONS);
#[allow(non_upper_case_globals)]
pub const NESTED_SERVICE_CLIENTS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Service as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Service as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_SERVICE_CLIENTS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_SERVICE_CLIENTS);
#[allow(non_upper_case_globals)]
pub const NESTED_SERVICE_SERVERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Service as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Service as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_SERVICE_SERVERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_SERVICE_SERVERS);
#[allow(non_upper_case_globals)]
pub const NESTED_ACTION_CLIENTS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Action as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Action as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_ACTION_CLIENTS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_ACTION_CLIENTS);
#[allow(non_upper_case_globals)]
pub const NESTED_ACTION_SERVERS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Action as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Action as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_ACTION_SERVERS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_ACTION_SERVERS);
impl ::nros_serdes::Message for Node {
    const TYPE_NAME: &'static str = "rosgraph_msgs/msg/Node";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "name",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Node, name),
        },
        ::nros_serdes::Field {
            name: "parameters",
            ty: ::nros_serdes::FieldType::Sequence(&FT_PARAMETERS_ELEM),
            offset: ::core::mem::offset_of!(Node, parameters),
        },
        ::nros_serdes::Field {
            name: "parameter_values",
            ty: ::nros_serdes::FieldType::Sequence(&FT_PARAMETER_VALUES_ELEM),
            offset: ::core::mem::offset_of!(Node, parameter_values),
        },
        ::nros_serdes::Field {
            name: "publishers",
            ty: ::nros_serdes::FieldType::Sequence(&FT_PUBLISHERS_ELEM),
            offset: ::core::mem::offset_of!(Node, publishers),
        },
        ::nros_serdes::Field {
            name: "subscriptions",
            ty: ::nros_serdes::FieldType::Sequence(&FT_SUBSCRIPTIONS_ELEM),
            offset: ::core::mem::offset_of!(Node, subscriptions),
        },
        ::nros_serdes::Field {
            name: "service_clients",
            ty: ::nros_serdes::FieldType::Sequence(&FT_SERVICE_CLIENTS_ELEM),
            offset: ::core::mem::offset_of!(Node, service_clients),
        },
        ::nros_serdes::Field {
            name: "service_servers",
            ty: ::nros_serdes::FieldType::Sequence(&FT_SERVICE_SERVERS_ELEM),
            offset: ::core::mem::offset_of!(Node, service_servers),
        },
        ::nros_serdes::Field {
            name: "action_clients",
            ty: ::nros_serdes::FieldType::Sequence(&FT_ACTION_CLIENTS_ELEM),
            offset: ::core::mem::offset_of!(Node, action_clients),
        },
        ::nros_serdes::Field {
            name: "action_servers",
            ty: ::nros_serdes::FieldType::Sequence(&FT_ACTION_SERVERS_ELEM),
            offset: ::core::mem::offset_of!(Node, action_servers),
        },
    ];
}
