//! Serialization format identity (RFC-0088, phase-421 W1).
//!
//! ROS 2 names its serialization format with a string because
//! `rosidl_typesupport_c` resolves the format's implementation through
//! `dlopen` — "if the identifier is the same as this handle's
//! typesupport_identifier, then the handle is simply returned, otherwise it's
//! loaded from a shared library". The string is a dynamic-linker key.
//!
//! nano-ros links one image, so the key can be a **type**. A message declares
//! its format, a backend declares its format, and the two are compared at
//! compile time. Nothing is resolved, dispatched or compared at run time on the
//! publish path.
//!
//! # The discriminant is image-local; the string is not
//!
//! [`SerializationFormatId`] is a `u8` **assigned within one image**. nano-ros
//! cannot allocate a globally unique discriminant to a third-party format, so
//! the value is meaningful only inside the build that produced it. The
//! `NAME` string is the identity that crosses image boundaries — the bridge
//! config, tooling output, and the `get_serialization_format` vtable slot all
//! carry the string, never the number.
//!
//! Treating the discriminant as global is the one mistake this module exists to
//! prevent: two independently built images would disagree about what `3` means,
//! which is a wire-visible bug with a compile-time-looking cause.

/// Image-local discriminant for a serialization format.
///
/// In-tree formats hold low reserved values for readability. A third-party
/// format declared by a provider package (RFC-0087 family `serdes`) is assigned
/// a value by the build from the set of formats that image declares.
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SerializationFormatId {
    /// OMG CDR as ROS 2 puts it on the wire, including the encapsulation
    /// header. Every DDS-derived backend and zenoh speak this.
    Cdr = 1,
    /// PX4's in-memory struct, verbatim — no encoding step at all (RFC-0011).
    Uorb = 2,
}

impl SerializationFormatId {
    /// The cross-image identity for this format.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cdr => "cdr",
            Self::Uorb => "uorb",
        }
    }

    /// The raw discriminant, for a C ABI boundary or a one-byte comparison.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// A serialization format, as a type.
///
/// Implementors are zero-sized markers. The trait carries no methods on
/// purpose: it exists to be *named* in an associated type, so that a mismatch
/// is a compile error rather than a runtime branch.
pub trait SerializationFormat {
    /// Image-local discriminant.
    const ID: SerializationFormatId;
    /// Cross-image identity. Always `ID.as_str()` for in-tree formats.
    const NAME: &'static str;
}

/// ROS 2's wire encoding, and nano-ros's default.
pub struct Cdr;

impl SerializationFormat for Cdr {
    const ID: SerializationFormatId = SerializationFormatId::Cdr;
    const NAME: &'static str = "cdr";
}

/// PX4 uORB: the payload *is* the struct, so there is no encoding step.
pub struct Uorb;

impl SerializationFormat for Uorb {
    const ID: SerializationFormatId = SerializationFormatId::Uorb;
    const NAME: &'static str = "uorb";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_agrees_with_discriminant() {
        assert_eq!(Cdr::NAME, Cdr::ID.as_str());
        assert_eq!(Uorb::NAME, Uorb::ID.as_str());
    }

    #[test]
    fn discriminants_are_distinct_and_stable() {
        assert_eq!(SerializationFormatId::Cdr.as_u8(), 1);
        assert_eq!(SerializationFormatId::Uorb.as_u8(), 2);
    }
}
