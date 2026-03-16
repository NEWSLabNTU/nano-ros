//! Compiler-derived opaque storage sizes for RMW handles.
//!
//! These constants are computed from `core::mem::size_of` at compile time,
//! so they always match the actual Rust type layout. No manual maintenance
//! needed — if the underlying type changes, these adjust automatically.
//!
//! When no RMW backend is enabled (workspace-level `cargo check`), placeholder
//! values are used. The placeholders are never used at runtime — all RMW code
//! is `#[cfg]`-gated.

use core::mem::size_of;

/// Compute the number of u64 units needed to store T with 8-byte alignment.
const fn u64s_for<T>() -> usize {
    size_of::<T>().div_ceil(8)
}

// When an RMW backend is active, compute exact sizes from the actual types.
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const SESSION_OPAQUE_U64S: usize = u64s_for::<nros::internals::RmwSession>();
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const PUBLISHER_OPAQUE_U64S: usize = u64s_for::<nros::internals::RmwPublisher>();
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const SERVICE_CLIENT_OPAQUE_U64S: usize = u64s_for::<nros::internals::RmwServiceClient>();

// Placeholders for no-RMW workspace builds.
#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
)))]
pub const SESSION_OPAQUE_U64S: usize = 1;
#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
)))]
pub const PUBLISHER_OPAQUE_U64S: usize = 1;
#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
)))]
pub const SERVICE_CLIENT_OPAQUE_U64S: usize = 1;

// ── Guard Condition ──────────────────────────────────────────────────────

pub const GUARD_HANDLE_OPAQUE_U64S: usize = u64s_for::<nros_node::GuardConditionHandle>();

// ── Action Server ──────────────────────────────────────────────────────

#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const ACTION_SERVER_INTERNAL_OPAQUE_U64S: usize =
    u64s_for::<crate::action::ActionServerInternal>();

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
)))]
pub const ACTION_SERVER_INTERNAL_OPAQUE_U64S: usize = 1;

// ── Action Client ──────────────────────────────────────────────────────

#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const ACTION_CLIENT_INTERNAL_OPAQUE_U64S: usize =
    u64s_for::<crate::action::ActionClientInternal>();

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
)))]
pub const ACTION_CLIENT_INTERNAL_OPAQUE_U64S: usize = 1;
