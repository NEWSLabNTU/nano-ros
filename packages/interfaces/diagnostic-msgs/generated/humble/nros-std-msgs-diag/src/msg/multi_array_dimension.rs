// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: MultiArrayDimension

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// MultiArrayDimension message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MultiArrayDimension {
    pub label: heapless::String<256>,
    pub size: u32,
    pub stride: u32,
}

impl Serialize for MultiArrayDimension {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) â DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        writer.write_string(self.label.as_str())?;
        writer.write_u32(self.size)?;
        writer.write_u32(self.stride)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for MultiArrayDimension {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) â read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            label: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            size: reader.read_u32()?,
            stride: reader.read_u32()?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for MultiArrayDimension {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::MultiArrayDimension_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for MultiArrayDimension {
    const TYPE_NAME: &'static str = "std_msgs/msg/MultiArrayDimension";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "label",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(MultiArrayDimension, label),
        },
        ::nros_serdes::Field {
            name: "size",
            ty: ::nros_serdes::FieldType::Uint32,
            offset: ::core::mem::offset_of!(MultiArrayDimension, size),
        },
        ::nros_serdes::Field {
            name: "stride",
            ty: ::nros_serdes::FieldType::Uint32,
            offset: ::core::mem::offset_of!(MultiArrayDimension, stride),
        },
    ];
}
