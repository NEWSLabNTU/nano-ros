// nros message type - pure Rust, no_std compatible
// Package: action_msgs
// Message: GoalStatusArray

use nros_core::{RosMessage, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// GoalStatusArray message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GoalStatusArray {
    pub status_list: heapless::Vec<crate::msg::GoalStatus, 64>,
}

impl Serialize for GoalStatusArray {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.status_list.len() as u32)?;
        for item in &self.status_list {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for GoalStatusArray {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            status_list: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    vec.push(Deserialize::deserialize(reader)?).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        })
    }
}

impl RosMessage for GoalStatusArray {
    const TYPE_NAME: &'static str = "action_msgs::msg::dds_::GoalStatusArray_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_STATUS_LIST: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::GoalStatus as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::GoalStatus as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_STATUS_LIST_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Nested(&NESTED_STATUS_LIST);
impl ::nros_serdes::Message for GoalStatusArray {
    const TYPE_NAME: &'static str = "action_msgs/msg/GoalStatusArray";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "status_list",
            ty: ::nros_serdes::FieldType::Sequence(&FT_STATUS_LIST_ELEM),
            offset: ::core::mem::offset_of!(GoalStatusArray, status_list),
        },
];
}