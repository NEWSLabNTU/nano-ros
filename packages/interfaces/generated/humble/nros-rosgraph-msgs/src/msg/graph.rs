// nros message type - pure Rust, no_std compatible
// Package: rosgraph_msgs
// Message: Graph

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Graph message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Graph {
    pub nodes: heapless::Vec<crate::msg::Node, 64>,
}

impl Serialize for Graph {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.nodes.len() as u32)?;
        for item in &self.nodes {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Graph {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            nodes: {
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

impl RosMessage for Graph {
    const TYPE_NAME: &'static str = "rosgraph_msgs::msg::dds_::Graph_";
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
pub const NESTED_NODES: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::Node as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::Node as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_NODES_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Nested(&NESTED_NODES);
impl ::nros_serdes::Message for Graph {
    const TYPE_NAME: &'static str = "rosgraph_msgs/msg/Graph";
    const FIELDS: &'static [::nros_serdes::Field] = &[::nros_serdes::Field {
        name: "nodes",
        ty: ::nros_serdes::FieldType::Sequence(&FT_NODES_ELEM),
        offset: ::core::mem::offset_of!(Graph, nodes),
    }];
}
