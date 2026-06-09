//! Serialization traits

use crate::{
    cdr::{CdrReader, CdrWriter},
    error::{DeserError, SerError},
};

/// Trait for types that can be serialized to CDR format
pub trait Serialize {
    /// Serialize this value to the CDR writer
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError>;
}

/// Trait for types that can be deserialized from CDR format
pub trait Deserialize: Sized {
    /// Deserialize a value from the CDR reader
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError>;
}

/// Trait for borrowed (zero-copy) deserialization from CDR format (RFC-0033
/// `borrowed` storage mode, issue 0007).
///
/// Unlike [`Deserialize`], which copies variable-length fields into owned
/// (`heapless`/heap) storage, an implementor borrows `&'a [u8]` / `&'a str`
/// slices directly out of the `CdrReader`'s source buffer. The resulting value
/// is therefore tied to that buffer's lifetime `'a` — it is valid only while
/// the receive buffer it points into is held (i.e. inside a subscription
/// callback). Fixed-size header fields are still read onto the stack; only the
/// unbounded sequence/string fields borrow.
///
/// The reader's zero-copy primitives (`read_slice_u8`, `read_string`,
/// `read_slice_f32_raw`, …) supply the borrowed slices.
pub trait DeserializeBorrowed<'a>: Sized {
    /// Deserialize a value, borrowing variable-length fields from the reader's
    /// source buffer (no copy).
    fn deserialize_borrowed(reader: &mut CdrReader<'a>) -> Result<Self, DeserError>;
}
