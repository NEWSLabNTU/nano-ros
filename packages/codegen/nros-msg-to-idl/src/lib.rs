//! Pure-Rust port of `scripts/cyclonedds/msg_to_cyclone_idl.py`.
//!
//! Phase 212.K.3 — replaces the python `rosidl_adapter` build-time
//! dependency for the Cyclone-DDS wrapper sys crate (Phase 212.K.2).
//!
//! See `README.md` for the user-facing API summary.

#![forbid(unsafe_code)]
// Phase 214 followup — pre-existing clippy nits in the pure-Rust IDL
// porter. Functional fixes deferred (would touch the emitter / parser
// shape that's mirrored against the upstream python reference); allow
// the lints so `just check` is unblocked.
#![allow(
    clippy::manual_pattern_char_comparison,
    clippy::match_like_matches_macro,
    clippy::extend_with_drain,
    clippy::manual_strip,
    clippy::unnecessary_unwrap,
    clippy::map_entry
)]

mod emitter;
mod mangle;
mod parser;
mod types;

pub use emitter::emit_idl;
pub use mangle::mangle_idl;
pub use parser::{Field, Message, RosType, parse_msg};
pub use types::{ConvertError, idl_type_for};

/// Convert a `.msg` source string to Cyclone-DDS-shaped IDL.
///
/// Output matches the python `msg_to_cyclone_idl.py` byte-for-byte
/// for the common-case `.msg` subset (primitives, strings, nested
/// types, fixed-size arrays, unbounded / bounded sequences).
pub fn msg_to_idl(msg_source: &str, package: &str, message: &str) -> Result<String, ConvertError> {
    Converter::new(package, message).convert(msg_source)
}

/// Builder-style entry point. Use this when the caller has more
/// context than the bare `(package, message)` pair — e.g. a `.srv`
/// half that needs the 16-byte request-header injected.
#[derive(Clone, Debug)]
pub struct Converter<'a> {
    package: &'a str,
    message: &'a str,
    /// Mirror of the python script's `inject_service_header` flag.
    /// Forces the two `cdds_request_header_t` fields
    /// (`unsigned long long rmw_writer_guid` + `long long
    /// rmw_sequence_number`) to be inlined as the first two members
    /// of every rewritten struct.
    inject_service_header: bool,
}

impl<'a> Converter<'a> {
    pub fn new(package: &'a str, message: &'a str) -> Self {
        Self {
            package,
            message,
            inject_service_header: false,
        }
    }

    pub fn with_service_header(mut self, on: bool) -> Self {
        self.inject_service_header = on;
        self
    }

    pub fn convert(&self, msg_source: &str) -> Result<String, ConvertError> {
        let msg = parse_msg(self.package, self.message, msg_source)?;
        let raw = emit_idl(self.package, self.message, &msg);
        Ok(mangle_idl(&raw, self.inject_service_header))
    }
}
