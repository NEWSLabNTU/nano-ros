//! Shared constants for nros-c
//!
//! The canonical values live in [`nros_node::limits`]. Literals are mirrored
//! here so `cbindgen` (run with `parse_deps = false`) can emit `#define`
//! values in the generated C header without crossing crate boundaries.
//! A `const _` assertion below catches any drift between the two sites.

/// Maximum length of a zenoh locator string (e.g., "tcp/127.0.0.1:7447")
pub const MAX_LOCATOR_LEN: usize = 128;

/// Maximum length of a node name
pub const MAX_NAME_LEN: usize = 64;

/// Maximum length of a node namespace
pub const MAX_NAMESPACE_LEN: usize = 128;

/// Maximum length of a topic name
pub const MAX_TOPIC_LEN: usize = 256;

/// Maximum length of a service name
pub const MAX_SERVICE_NAME_LEN: usize = 256;

/// Maximum length of an action name
pub const MAX_ACTION_NAME_LEN: usize = 256;

/// Maximum length of a type name (e.g., "std_msgs::msg::dds_::Int32_")
pub const MAX_TYPE_NAME_LEN: usize = 256;

/// Maximum length of a type hash (RIHS format)
pub const MAX_TYPE_HASH_LEN: usize = 128;

/// Maximum number of concurrent goals per action server.
///
/// This is a fixed constant (not configurable via env var) because it
/// affects `nros_action_server_t` struct layout. Changing it requires
/// recompiling both Rust and C code.
pub const NROS_MAX_CONCURRENT_GOALS: usize = 4;

/// Upper-bound inline storage (in `u64`) for
/// `nros_lifecycle_state_machine_t`. Sized generously for the largest
/// supported target; a compile-time assertion in `opaque_sizes.rs` checks
/// that the actual Rust type fits.
pub const NROS_LIFECYCLE_CTX_OPAQUE_U64S: usize = 16;

// Compile-time drift check: these literals must match the canonical values
// exported from `nros_node::limits`.
const _: () = {
    assert!(MAX_LOCATOR_LEN == nros_node::limits::MAX_LOCATOR_LEN);
    assert!(MAX_NAME_LEN == nros_node::limits::MAX_NAME_LEN);
    assert!(MAX_NAMESPACE_LEN == nros_node::limits::MAX_NAMESPACE_LEN);
    assert!(MAX_TOPIC_LEN == nros_node::limits::MAX_TOPIC_LEN);
    assert!(MAX_SERVICE_NAME_LEN == nros_node::limits::MAX_SERVICE_NAME_LEN);
    assert!(MAX_ACTION_NAME_LEN == nros_node::limits::MAX_ACTION_NAME_LEN);
    assert!(MAX_TYPE_NAME_LEN == nros_node::limits::MAX_TYPE_NAME_LEN);
    assert!(MAX_TYPE_HASH_LEN == nros_node::limits::MAX_TYPE_HASH_LEN);
    assert!(NROS_MAX_CONCURRENT_GOALS == nros_node::limits::MAX_CONCURRENT_GOALS);
};

// ── Inline opaque storage sizes ─────────────────────────────────────────
//
// Computed from `core::mem::size_of` at compile time — always matches the
// actual Rust type layout. See `opaque_sizes.rs`.
pub use crate::opaque_sizes::{
    GUARD_HANDLE_OPAQUE_U64S, PUBLISHER_OPAQUE_U64S, SESSION_OPAQUE_U64S,
};
