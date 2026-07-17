// nros message type - pure Rust, no_std compatible
// Package: std_msgs
// Message: ColorRGBA

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// ColorRGBA message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorRGBA {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Serialize for ColorRGBA {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_f32(self.r)?;
        writer.write_f32(self.g)?;
        writer.write_f32(self.b)?;
        writer.write_f32(self.a)?;
        Ok(())
    }
}

impl Deserialize for ColorRGBA {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            r: reader.read_f32()?,
            g: reader.read_f32()?,
            b: reader.read_f32()?,
            a: reader.read_f32()?,
        })
    }
}

impl RosMessage for ColorRGBA {
    const TYPE_NAME: &'static str = "std_msgs::msg::dds_::ColorRGBA_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
}

// ââ nros_serdes::Message â runtime field schema âââââââââââââââââââââââââââââ
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, â¦) without per-RMW codegen at compile time.

impl ::nros_serdes::Message for ColorRGBA {
    const TYPE_NAME: &'static str = "std_msgs/msg/ColorRGBA";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "r",
            ty: ::nros_serdes::FieldType::Float32,
            offset: ::core::mem::offset_of!(ColorRGBA, r),
        },
        ::nros_serdes::Field {
            name: "g",
            ty: ::nros_serdes::FieldType::Float32,
            offset: ::core::mem::offset_of!(ColorRGBA, g),
        },
        ::nros_serdes::Field {
            name: "b",
            ty: ::nros_serdes::FieldType::Float32,
            offset: ::core::mem::offset_of!(ColorRGBA, b),
        },
        ::nros_serdes::Field {
            name: "a",
            ty: ::nros_serdes::FieldType::Float32,
            offset: ::core::mem::offset_of!(ColorRGBA, a),
        },
    ];
}
