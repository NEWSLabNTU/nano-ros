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
// Phase 82: service client opaque storage no longer holds the RMW
// transport handle (it lives in the executor's arena). Use
// SERVICE_CLIENT_INTERNAL_OPAQUE_U64S (from build.rs) for the C struct
// instead.

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

// ── Guard Condition ──────────────────────────────────────────────────────

pub const GUARD_HANDLE_OPAQUE_U64S: usize = u64s_for::<nros_node::GuardConditionHandle>();

// ── Lifecycle (no RMW dependency) ────────────────────────────────────────
//
// Phase 87: `NROS_LIFECYCLE_CTX_OPAQUE_U64S` is now derived from
// `size_of::<LifecyclePollingNodeCtx>()` directly (see `constants.rs`),
// so the previous "upper bound" assertion is trivially true and has
// been removed. The C-side `NROS_LIFECYCLE_CTX_SIZE` macro lives in
// `nros_config_generated.h`.

// Phase 87.5 (full): all four `*Internal` shim types are now
// `#[repr(C)]` and embedded directly in their outer `nros_*_t` structs.
//
// `ActionServerInternal` lives in this crate (it embeds C-API pointer
// types) so it can't be exported from `nros::sizes` directly. Instead,
// `nros::sizes::ActionServerInternalLayout` is a layout-mirror struct
// with the same `#[repr(C)]` field shape; we assert at compile time
// that the byte sizes match. Mismatch = the C-side
// `NROS_ACTION_SERVER_INTERNAL_SIZE` macro is wrong, which would
// silently corrupt the `nros_action_server_t` struct layout.
#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]
const _: () = assert!(
    size_of::<crate::action::ActionServerInternal>()
        == size_of::<nros::sizes::ActionServerInternalLayout>(),
    "ActionServerInternal size diverges from nros::sizes::ActionServerInternalLayout — \
     update the layout mirror in `nros/src/sizes.rs` to track any field-shape change"
);
