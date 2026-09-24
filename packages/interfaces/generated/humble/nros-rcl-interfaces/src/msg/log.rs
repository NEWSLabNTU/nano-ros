// nros message type - pure Rust, no_std compatible
// Package: rcl_interfaces
// Message: Log

use nros_core::{Deserialize, RosMessage, Serialize};
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};
pub const DEBUG: u8 = 10;
pub const INFO: u8 = 20;
pub const WARN: u8 = 30;
pub const ERROR: u8 = 40;
pub const FATAL: u8 = 50;

/// Log message type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Log {
    pub stamp: nros_builtin_interfaces::msg::Time,
    pub level: u8,
    pub name: heapless::String<256>,
    pub msg: heapless::String<256>,
    pub file: heapless::String<256>,
    pub function: heapless::String<256>,
    pub line: u32,
}

impl Serialize for Log {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap for XCDR2 appendable structs.
        // No-op under XCDR1 (byte-identical); under XCDR2 delimits this struct.
        let __dh = writer.begin_dheader()?;
        self.stamp.serialize(writer)?;
        writer.write_u8(self.level)?;
        writer.write_string(self.name.as_str())?;
        writer.write_string(self.msg.as_str())?;
        writer.write_string(self.file.as_str())?;
        writer.write_string(self.function.as_str())?;
        writer.write_u32(self.line)?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for Log {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        // phase-303 W4 (#0267) — read the XCDR2 DHEADER (no-op under XCDR1);
        // end_dheader skips any unknown trailing members (forward compat).
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            stamp: Deserialize::deserialize(reader)?,
            level: reader.read_u8()?,
            name: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            msg: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            file: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            function: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
            line: reader.read_u32()?,
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for Log {
    const TYPE_NAME: &'static str = "rcl_interfaces::msg::dds_::Log_";
    const TYPE_HASH: &'static str = "TypeHashNotSupported";
    // RFC-0052 W3a — Header/Time-leading type: `stamp.sec` at CDR byte
    // 4 (raw-buffer peek for on-target max_age monitors).
    const STAMP_OFFSET: Option<usize> = Some(4);
}

// ── nros_serdes::Message — runtime field schema ─────────────────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const NESTED_STAMP: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <nros_builtin_interfaces::msg::Time as ::nros_serdes::Message>::TYPE_NAME,
    fields: <nros_builtin_interfaces::msg::Time as ::nros_serdes::Message>::FIELDS,
};
impl ::nros_serdes::Message for Log {
    const TYPE_NAME: &'static str = "rcl_interfaces/msg/Log";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "stamp",
            ty: ::nros_serdes::FieldType::Nested(&NESTED_STAMP),
            offset: ::core::mem::offset_of!(Log, stamp),
        },
        ::nros_serdes::Field {
            name: "level",
            ty: ::nros_serdes::FieldType::Uint8,
            offset: ::core::mem::offset_of!(Log, level),
        },
        ::nros_serdes::Field {
            name: "name",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Log, name),
        },
        ::nros_serdes::Field {
            name: "msg",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Log, msg),
        },
        ::nros_serdes::Field {
            name: "file",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Log, file),
        },
        ::nros_serdes::Field {
            name: "function",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(Log, function),
        },
        ::nros_serdes::Field {
            name: "line",
            ty: ::nros_serdes::FieldType::Uint32,
            offset: ::core::mem::offset_of!(Log, line),
        },
    ];
}
