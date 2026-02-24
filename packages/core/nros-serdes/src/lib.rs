//! CDR serialization/deserialization for nros.
//!
//! Implements OMG Common Data Representation (CDR) encoding compatible with
//! ROS 2. All types use little-endian byte order with natural alignment.
//!
//! # Examples
//!
//! ```
//! use nros_serdes::{CdrWriter, CdrReader, Serialize, Deserialize, SerError, DeserError};
//!
//! // Serialize a u32 into a CDR buffer
//! let mut buf = [0u8; 64];
//! let mut writer = CdrWriter::new_with_header(&mut buf).unwrap();
//! 42u32.serialize(&mut writer).unwrap();
//! let len = writer.position();
//!
//! // Deserialize it back
//! let mut reader = CdrReader::new_with_header(&buf[..len]).unwrap();
//! let value = u32::deserialize(&mut reader).unwrap();
//! assert_eq!(value, 42);
//! ```
//!
//! # Features
//!
//! - `std` — Enable standard library support
//! - `alloc` — Enable heap allocation (`String`, `Vec<T>`)

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod cdr;
pub mod error;
pub mod primitives;
pub mod traits;

#[cfg(test)]
mod compat_tests;

pub use cdr::{CdrReader, CdrWriter};
pub use error::{DeserError, SerError};
pub use traits::{Deserialize, Serialize};

/// CDR encapsulation header for little-endian encoding
pub const CDR_LE_HEADER: [u8; 4] = [0x00, 0x01, 0x00, 0x00];

/// CDR encapsulation header for big-endian encoding
pub const CDR_BE_HEADER: [u8; 4] = [0x00, 0x00, 0x00, 0x00];
