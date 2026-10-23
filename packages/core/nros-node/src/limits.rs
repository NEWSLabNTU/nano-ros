//! Shared string-length and resource caps used by the Rust, C, and C++ APIs.
//!
//! These values are structural — changing any of them alters the layout of
//! `#[repr(C)]` handle types in the C/C++ bindings and requires recompiling
//! both Rust and C/C++ code.

/// Maximum length of a zenoh locator string (e.g., `tcp/127.0.0.1:7447`).
pub const MAX_LOCATOR_LEN: usize = 128;

/// Maximum length of a node name.
pub const MAX_NAME_LEN: usize = 64;

/// Maximum length of a node namespace.
pub const MAX_NAMESPACE_LEN: usize = 128;

/// Maximum length of a topic name.
pub const MAX_TOPIC_LEN: usize = 256;

/// Maximum length of a service name.
pub const MAX_SERVICE_NAME_LEN: usize = 256;

/// Maximum length of an action name.
pub const MAX_ACTION_NAME_LEN: usize = 256;

/// Maximum length of a type name (e.g., `std_msgs::msg::dds_::Int32_`).
pub const MAX_TYPE_NAME_LEN: usize = 256;

/// Maximum length of a type hash (RIHS format).
pub const MAX_TYPE_HASH_LEN: usize = 128;

/// Maximum number of concurrent goals per action server.
pub const MAX_CONCURRENT_GOALS: usize = 4;

/// Issue 2009 — request ids one executor-owned (callback / blocking) service
/// client remembers for its ONE logical call in flight.
///
/// A logical call can be on the wire several times: `rclc_client_call` re-sends
/// its request every 500 ms to ride out the discovery race, and each re-send
/// gets a fresh id. Any of those generations answers the call, so the client
/// accepts a reply naming any of the last `MAX_CLIENT_RESEND_GENERATIONS`
/// generations and drops everything else — including a late reply to a call
/// that already completed or timed out, which used to be handed to the NEXT
/// call. Four generations covers a server answering up to ~2 s after the
/// request it answers; an older generation's reply is dropped.
pub const MAX_CLIENT_RESEND_GENERATIONS: usize = 4;
