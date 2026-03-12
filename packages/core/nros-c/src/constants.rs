//! Shared constants for nros-c
//!
//! These constants define the maximum sizes for various string buffers
//! used in the C API. They are exported to C through cbindgen.

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

// ── Inline opaque storage sizes ─────────────────────────────────────────
//
// These constants define the inline storage (in `u64` units) for RMW handles
// embedded directly in nros C API structs, avoiding heap allocation.
// Compile-time assertions verify that each constant is large enough for
// the active RMW backend.  If you add a new backend whose handles are
// larger, increase the relevant constant.

/// Inline storage for `RmwSession` inside `nros_support_t` (in u64 units).
pub const SESSION_OPAQUE_U64S: usize = 64;

/// Inline storage for `RmwPublisher` inside `nros_publisher_t` (in u64 units).
pub const PUBLISHER_OPAQUE_U64S: usize = 48;

/// Inline storage for `RmwServiceClient` inside `nros_client_t` (in u64 units).
pub const SERVICE_CLIENT_OPAQUE_U64S: usize = 48;

/// Inline storage for `GuardConditionHandle` inside `nros_guard_condition_t` (in u64 units).
pub const GUARD_HANDLE_OPAQUE_U64S: usize = 4;
