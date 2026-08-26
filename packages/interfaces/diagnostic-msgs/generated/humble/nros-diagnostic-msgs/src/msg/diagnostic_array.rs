// nros message type - pure Rust, no_std compatible
// Package: diagnostic_msgs
// Message: DiagnosticArray

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// DiagnosticArray message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiagnosticArray {
    pub header: nros_std_msgs_diag::msg::Header,
    pub status: heapless::Vec<crate::msg::DiagnosticStatus, 4>,
}

impl Serialize for DiagnosticArray {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) Ã¢ÂÂ DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        self.header.serialize(writer)?;
        writer.write_u32(self.status.len() as u32)?;
        for item in &self.status {
            item.serialize(writer)?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for DiagnosticArray {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) Ã¢ÂÂ read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            header: Deserialize::deserialize(reader)?,
            status: {
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

impl RosMessage for DiagnosticArray {
    const TYPE_NAME: &'static str = "diagnostic_msgs::msg::dds_::DiagnosticArray_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
    // RFC-0052 W3a Ã¢ÂÂ Header/Time-leading type: `stamp.sec` at CDR byte
    // 4 (raw-buffer peek for on-target max_age monitors).
    const STAMP_OFFSET: Option<usize> = Some(4);
}

// Ã¢ÂÂÃ¢ÂÂ nros_serdes::Message Ã¢ÂÂ runtime field schema Ã¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂÃ¢ÂÂ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, Ã¢ÂÂ¦) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_HEADER: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <nros_std_msgs_diag::msg::Header as ::nros_serdes::Message>::TYPE_NAME,
    fields: <nros_std_msgs_diag::msg::Header as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const NESTED_STATUS: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <crate::msg::DiagnosticStatus as ::nros_serdes::Message>::TYPE_NAME,
    fields: <crate::msg::DiagnosticStatus as ::nros_serdes::Message>::FIELDS,
};
#[allow(non_upper_case_globals)]
pub const FT_STATUS_ELEM: ::nros_serdes::FieldType =
    ::nros_serdes::FieldType::Nested(&NESTED_STATUS);
impl ::nros_serdes::Message for DiagnosticArray {
    const TYPE_NAME: &'static str = "diagnostic_msgs/msg/DiagnosticArray";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "header",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_HEADER),
            offset: ::core::mem::offset_of!(DiagnosticArray, header),
        },
        ::nros_serdes::Field {
            name: "status",
            ty: ::nros_serdes::FieldType::Sequence(&FT_STATUS_ELEM),
            offset: ::core::mem::offset_of!(DiagnosticArray, status),
        },
    ];
}
