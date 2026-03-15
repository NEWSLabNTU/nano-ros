//! Compiler-derived opaque storage sizes for RMW handles.
//!
//! These constants are computed from `core::mem::size_of` at compile time,
//! so they always match the actual Rust type layout. No manual maintenance
//! needed — if the underlying type changes, these adjust automatically.

use core::mem::size_of;

/// Compute the number of u64 units needed to store T with 8-byte alignment.
const fn u64s_for<T>() -> usize {
    (size_of::<T>() + 7) / 8
}

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
)))]
compile_error!(
    "nros-c requires exactly one RMW backend feature: rmw-zenoh, rmw-xrce, rmw-dds, or rmw-cffi"
);

// ── Session ──────────────────────────────────────────────────────────────

#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const SESSION_OPAQUE_U64S: usize = u64s_for::<nros::internals::RmwSession>();

// ── Publisher ────────────────────────────────────────────────────────────

#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const PUBLISHER_OPAQUE_U64S: usize = u64s_for::<nros::internals::RmwPublisher>();

// ── Service Client ───────────────────────────────────────────────────────

#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub const SERVICE_CLIENT_OPAQUE_U64S: usize = u64s_for::<nros::internals::RmwServiceClient>();

// ── Guard Condition ──────────────────────────────────────────────────────

pub const GUARD_HANDLE_OPAQUE_U64S: usize = u64s_for::<nros_node::GuardConditionHandle>();
