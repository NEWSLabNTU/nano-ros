//! RFC-0088 / phase-421 W1 — the message-format check, as a compile error.
//!
//! ROS 2 names its serialization format with a string and answers
//! `rmw_get_serialization_format()` at run time, because
//! `rosidl_typesupport_c` resolves the format's implementation through
//! `dlopen` and the string is the linker key. nano-ros links one image and
//! selects its backend by cargo feature, so the same question has a
//! compile-time answer: [`crate::session::IMAGE_SERIALIZATION_FORMAT_ID`].
//!
//! [`assert_message_format`] compares a message's declared
//! [`nros_core::RosMessage::SERIALIZATION_FORMAT_ID`] against that constant inside an
//! inline `const {}` block. The comparison therefore happens during
//! monomorphisation of the entity-creation call, and costs nothing at run
//! time — no branch appears on the publish path, which is the property
//! RFC-0088 D1 asks for.
//!
//! # What the error looks like
//!
//! Creating a publisher for a `Uorb` message in an image whose backend speaks
//! CDR fails like this. The primary span is the `assert!` below and the
//! offending type + call site arrive as notes, which is how rustc reports a
//! post-monomorphisation const failure:
//!
//! ```text
//! error[E0080]: evaluation panicked: message serialization format does not
//!               match the linked backend (RFC-0088)
//!   --> packages/core/nros-node/src/format_check.rs:83:9
//!    | evaluation of `format_check::assert_message_format::<UorbProbe>::{constant#0}`
//!    | failed here
//!
//! note: the above error was encountered while instantiating
//!       `fn assert_message_format::<UorbProbe>`
//!   --> src/main.rs:12:9
//!    |
//! 12 |     node.create_publisher::<VehicleStatus>("/status")?;
//! ```
//!
//! **It is a `cargo build` error, not a `cargo check` one.** An inline `const`
//! block in a generic function is evaluated by the monomorphisation collector,
//! which only runs during codegen — `cargo check -p nros-node` compiles the
//! mismatch silently. Measured 2026-09-04. `just ci gate` catches it because
//! `test-unit` builds; a lane that only type-checks does not.
//!
//! # Coverage
//!
//! The check reads `nros_core::RosMessage::SERIALIZATION_FORMAT_ID`, and
//! `MessageForRmw` — the bound every typed creator carries — requires
//! `RosMessage` under **every** backend. So the assertion is universal: zenoh,
//! XRCE, Cyclone and uORB alike.
//!
//! Keying it on `nros_serdes::schema::Message` instead would have covered only
//! Cyclone, because `MessageForRmw` requires a schema solely under
//! `cfg(rmw_needs_type_descriptors)` — and would therefore have been absent
//! under uORB, the one backend whose format differs and the reason the check
//! exists. The const is defaulted rather than required for the reason
//! phase-380 W4 recorded: tightening the message contract to serve a build
//! assertion broke `examples/native/rust/custom-msg`, the documented
//! hand-written-message pattern. A default costs those implementors nothing.
//!
//! # Proving the negative case
//!
//! A compile error cannot be asserted by a running test, and this workspace has
//! no `trybuild` (or any compile-fail) harness; adding one for a single case is
//! more machinery than the case is worth. Reproduce it by hand instead —
//! append to this file:
//!
//! ```ignore
//! fn _mismatch() {
//!     struct UorbProbe;
//!     impl nros_core::RosMessage for UorbProbe {
//!         const SERIALIZATION_FORMAT_ID = nros_serdes::format::SerializationFormatId::Uorb;
//!         const TYPE_NAME: &'static str = "px4/msg/UorbProbe";
//!         const TYPE_HASH: &'static str = "";
//!     }
//!     assert_message_format::<UorbProbe>();
//! }
//! ```
//!
//! The probe must be `pub` (or otherwise reachable): a private, never-called
//! function is dropped before the monomorphisation collector runs, and the
//! assertion then never instantiates — measured, having first written the
//! probe private and seen a clean build.
//!
//! and `cargo build -p nros-node --features rmw-cffi` reports the `E0080`
//! above (`cargo check` does not — see above). The runnable half
//! of the claim is `tests::cdr_and_uorb_are_distinguishable`: if the two
//! formats ever stopped differing, the compile error would stop being
//! reachable and every assertion in the tree would pass vacuously.

/// Assert at compile time that `M` is encoded in the format the linked backend
/// speaks.
///
/// Zero-sized and inlined away; the whole effect is the `const {}` block, which
/// is evaluated when this function is monomorphised for `M`. See the module
/// docs for the error a mismatch produces and for what it does *not* cover.
#[inline(always)]
pub fn assert_message_format<M: nros_core::RosMessage>() {
    const {
        assert!(
            <M as nros_core::RosMessage>::SERIALIZATION_FORMAT_ID.as_u8()
                == crate::session::IMAGE_SERIALIZATION_FORMAT_ID.as_u8(),
            "message serialization format does not match the linked backend (RFC-0088)"
        );
    }
}

/// The raw-entity counterpart: assert that `F`, the format a caller states its
/// already-encoded bytes are in, is the one the linked backend speaks.
///
/// `EmbeddedRawPublisher::publish_raw` was documented as taking "raw
/// CDR-encoded data (must include CDR header)" and checked by nothing. This is
/// that sentence, as a bound a caller can name.
///
/// The raw constructors do **not** take `F` today: Rust forbids a default type
/// parameter on a function (`invalid_type_param_default`, deny-by-default
/// future-compat lint), so `create_publisher_raw<F = Cdr>` does not exist and
/// making them generic without a default would break every existing call site's
/// inference. A caller that wants the check states it explicitly:
///
/// ```
/// # use nros_node::format_check::assert_raw_format;
/// assert_raw_format::<nros_serdes::format::Cdr>();
/// ```
#[inline(always)]
pub fn assert_raw_format<F: nros_serdes::format::SerializationFormat>() {
    const {
        assert!(
            <F as nros_serdes::format::SerializationFormat>::ID.as_u8()
                == crate::session::IMAGE_SERIALIZATION_FORMAT_ID.as_u8(),
            "raw payload format does not match the linked backend (RFC-0088)"
        );
    }
}

#[cfg(test)]
mod tests {
    use nros_serdes::format::{Cdr, SerializationFormat, Uorb};

    /// The assertion can only be load-bearing if the two formats it compares
    /// are actually distinguishable. If this ever held, every
    /// `assert_message_format` in the tree would pass vacuously.
    #[test]
    fn cdr_and_uorb_are_distinguishable() {
        assert_ne!(
            <Cdr as SerializationFormat>::ID.as_u8(),
            <Uorb as SerializationFormat>::ID.as_u8(),
            "Cdr and Uorb share a discriminant: the compile-time format check \
             cannot fail, so it checks nothing"
        );
        assert_ne!(
            <Cdr as SerializationFormat>::NAME,
            <Uorb as SerializationFormat>::NAME
        );
    }

    /// The positive case, exercised for real: a CDR message passes the same
    /// assertion the entity creators run, against this image's backend
    /// constant. If the constant or the message's declared format disagreed,
    /// this test would fail to COMPILE — which is the intended failure mode.
    #[test]
    fn a_cdr_message_matches_this_image() {
        struct Probe;
        impl nros_serdes::Serialize for Probe {
            fn serialize(
                &self,
                _w: &mut nros_serdes::cdr::CdrWriter,
            ) -> Result<(), nros_serdes::error::SerError> {
                Ok(())
            }
        }
        impl nros_serdes::Deserialize for Probe {
            fn deserialize(
                _r: &mut nros_serdes::cdr::CdrReader,
            ) -> Result<Self, nros_serdes::error::DeserError> {
                Ok(Probe)
            }
        }
        impl nros_core::RosMessage for Probe {
            const TYPE_NAME: &'static str = "test_msgs/msg/Probe";
            const TYPE_HASH: &'static str = "";
        }

        super::assert_message_format::<Probe>();
        super::assert_raw_format::<Cdr>();

        // The image this test runs in is a CDR image; state that, so a future
        // backend swap is a failing assertion here rather than a silent
        // inversion of what the two tests above mean.
        assert_eq!(
            crate::session::IMAGE_SERIALIZATION_FORMAT_ID.as_u8(),
            <Cdr as SerializationFormat>::ID.as_u8()
        );
        assert_eq!(crate::session::IMAGE_SERIALIZATION_FORMAT, "cdr");
    }
}
