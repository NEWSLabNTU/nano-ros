//! Error types for CDR serialization and deserialization.
//!
//! Two error enums cover the two directions: [`SerError`] for writes and
//! [`DeserError`] for reads. Both are `Copy` and `no_std`-compatible.

use core::fmt;

/// Serialization error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerError {
    /// Buffer is too small to hold the serialized data.
    BufferTooSmall,
    /// String length exceeds `u32::MAX` (CDR uses a 4-byte length prefix).
    StringTooLong,
    /// Sequence length exceeds `u32::MAX` (CDR uses a 4-byte length prefix).
    SequenceTooLong,
}

impl fmt::Display for SerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerError::BufferTooSmall => write!(f, "buffer too small"),
            SerError::StringTooLong => write!(f, "string too long"),
            SerError::SequenceTooLong => write!(f, "sequence too long"),
        }
    }
}

/// Deserialization error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeserError {
    /// Unexpected end of buffer (tried to read past available data).
    UnexpectedEof,
    /// Invalid data encountered (e.g., boolean value other than 0 or 1).
    InvalidData,
    /// Invalid UTF-8 in a CDR string field.
    InvalidUtf8,
    /// Decoded sequence/string length exceeds the `heapless` container capacity.
    CapacityExceeded,
    /// The 4-byte CDR encapsulation header is missing or invalid.
    InvalidHeader,
}

impl fmt::Display for DeserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserError::UnexpectedEof => write!(f, "unexpected end of buffer"),
            DeserError::InvalidData => write!(f, "invalid data"),
            DeserError::InvalidUtf8 => write!(f, "invalid UTF-8"),
            DeserError::CapacityExceeded => write!(f, "capacity exceeded"),
            DeserError::InvalidHeader => write!(f, "invalid encapsulation header"),
        }
    }
}
