// emit:ok
// nros service type - pure Rust, no_std compatible
// Package: fingerprint-corpus
// Service: Probe

use nros_core::{RosMessage, RosService, Serialize, Deserialize};
use nros_serdes::{CdrReader, CdrWriter, SerError, DeserError};

/// Probe request message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProbeRequest {
    pub items: nros_core::heap::Vec<i64>,
    pub note: heapless::String<256>,
}

impl Serialize for ProbeRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_u32(self.items.len() as u32)?;
        for item in &self.items {
            writer.write_i64(*item)?;
        }
        writer.write_string(self.note.as_str())?;
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ProbeRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            items: {
                let len = reader.read_u32()? as usize;
                let mut vec = nros_core::heap::Vec::new();
                for _ in 0..len {
                    vec.push(reader.read_i64()?);
                }
                vec
            },
            note: {
                let s = reader.read_string()?;
                heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ProbeRequest {
    const TYPE_NAME: &'static str = "fingerprint-corpus::srv::dds_::Probe_Request_";
    const TYPE_HASH: &'static str = "h";
}

// ── nros_serdes::Message — runtime field schema (Request) ───────────────────
// Consumed by RMW backends that build wire-type descriptors at runtime
// (Cyclone DDS dynamic types, …) without per-RMW codegen at compile time.

#[allow(non_upper_case_globals)]
pub const REQ_FT_ITEMS_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Int64;
impl ::nros_serdes::Message for ProbeRequest {
    const TYPE_NAME: &'static str = "fingerprint-corpus/srv/Probe_Request";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "items",
            ty: ::nros_serdes::FieldType::Sequence(&REQ_FT_ITEMS_ELEM),
            offset: ::core::mem::offset_of!(ProbeRequest, items),
        },
        ::nros_serdes::Field {
            name: "note",
            ty: ::nros_serdes::FieldType::String,
            offset: ::core::mem::offset_of!(ProbeRequest, note),
        },
];
}

/// Probe response message
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProbeResponse {
    pub sum: i64,
    pub lines: heapless::Vec<heapless::String<256>, 64>,
}

impl Serialize for ProbeResponse {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        // phase-303 W4 (#0267) — DHEADER wrap (no-op under XCDR1).
        let __dh = writer.begin_dheader()?;
        writer.write_i64(self.sum)?;
        writer.write_u32(self.lines.len() as u32)?;
        for item in &self.lines {
            writer.write_string(item.as_str())?;
        }
        writer.end_dheader(__dh)?;
        Ok(())
    }
}

impl Deserialize for ProbeResponse {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let __dh = reader.begin_dheader()?;
        let __value = Self {
            sum: reader.read_i64()?,
            lines: {
                let len = reader.read_u32()? as usize;
                let mut vec = heapless::Vec::new();
                for _ in 0..len {
                    let s = reader.read_string()?;
                    let elem = heapless::String::try_from(s).map_err(|_| DeserError::CapacityExceeded)?;
                    vec.push(elem).map_err(|_| DeserError::CapacityExceeded)?;
                }
                vec
            },
        };
        reader.end_dheader(__dh)?;
        Ok(__value)
    }
}

impl RosMessage for ProbeResponse {
    const TYPE_NAME: &'static str = "fingerprint-corpus::srv::dds_::Probe_Response_";
    const TYPE_HASH: &'static str = "h";
}

// ── nros_serdes::Message — runtime field schema (Response) ──────────────────

#[allow(non_upper_case_globals)]
pub const RESP_FT_LINES_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::String;
impl ::nros_serdes::Message for ProbeResponse {
    const TYPE_NAME: &'static str = "fingerprint-corpus/srv/Probe_Response";
    const FIELDS: &'static [::nros_serdes::Field] = &[
        ::nros_serdes::Field {
            name: "sum",
            ty: ::nros_serdes::FieldType::Int64,
            offset: ::core::mem::offset_of!(ProbeResponse, sum),
        },
        ::nros_serdes::Field {
            name: "lines",
            ty: ::nros_serdes::FieldType::Sequence(&RESP_FT_LINES_ELEM),
            offset: ::core::mem::offset_of!(ProbeResponse, lines),
        },
];
}

/// Probe service definition
pub struct Probe;

impl RosService for Probe {
    type Request = ProbeRequest;
    type Reply = ProbeResponse;

    const SERVICE_NAME: &'static str = "fingerprint-corpus::srv::dds_::Probe_";
    const SERVICE_HASH: &'static str = "h";
}