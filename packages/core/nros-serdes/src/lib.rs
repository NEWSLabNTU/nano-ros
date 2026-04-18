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

/// Length of the CDR encapsulation header (representation identifier + options).
pub const CDR_HEADER_LEN: usize = 4;

/// CDR encapsulation header for little-endian encoding
pub const CDR_LE_HEADER: [u8; CDR_HEADER_LEN] = [0x00, 0x01, 0x00, 0x00];

/// CDR encapsulation header for big-endian encoding
pub const CDR_BE_HEADER: [u8; CDR_HEADER_LEN] = [0x00, 0x00, 0x00, 0x00];

/// Write the little-endian CDR header into the first `CDR_HEADER_LEN` bytes of `dst`.
///
/// Returns the remaining payload slice `&mut dst[CDR_HEADER_LEN..]` on success,
/// or `None` if `dst` is shorter than the header.
#[inline]
pub fn write_cdr_le_header(dst: &mut [u8]) -> Option<&mut [u8]> {
    if dst.len() < CDR_HEADER_LEN {
        return None;
    }
    dst[..CDR_HEADER_LEN].copy_from_slice(&CDR_LE_HEADER);
    Some(&mut dst[CDR_HEADER_LEN..])
}

/// Strip the CDR encapsulation header from `src`, returning the payload slice.
///
/// Does not verify header contents — callers that need to validate the
/// representation identifier should do so separately. Returns `src` unchanged
/// if it is shorter than the header (so downstream parsers fail with a clearer
/// error than an out-of-bounds slice).
#[inline]
pub fn strip_cdr_header(src: &[u8]) -> &[u8] {
    if src.len() >= CDR_HEADER_LEN {
        &src[CDR_HEADER_LEN..]
    } else {
        src
    }
}
