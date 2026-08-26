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
#[cfg(feature = "rmw-cffi")]
pub const SESSION_OPAQUE_U64S: usize = u64s_for::<nros::internals::RmwSession>();
#[cfg(feature = "rmw-cffi")]
pub const PUBLISHER_OPAQUE_U64S: usize = u64s_for::<nros::internals::RmwPublisher>();

// Phase 122.3.b — L1 polling-mode subscription storage. Holds a
// `RawSubscription<MESSAGE_BUFFER_SIZE>` (RmwSubscriber + buffer +
// event regs) inline in `nros_subscription_t._opaque`. Sized so the
// Rust value fits exactly.
#[cfg(feature = "rmw-cffi")]
pub const SUBSCRIPTION_OPAQUE_U64S: usize =
    u64s_for::<nros_node::RawSubscription<{ crate::config::MESSAGE_BUFFER_SIZE }>>();
// Phase 122.3.c — L1 polling-mode service storage. Holds the
// `RawServiceServer<REQ, RESP>` / `RawServiceClient<REQ, REPLY>`
// Rust value inline in the corresponding C struct's `_opaque`.
// L2 (callback + executor arena) keeps the existing
// `ServiceServerInternal` shim and entity lives in the executor
// arena — those paths are unaffected.
#[cfg(feature = "rmw-cffi")]
pub const SERVICE_SERVER_OPAQUE_U64S: usize = u64s_for::<
    nros_node::RawServiceServer<
        { crate::config::MESSAGE_BUFFER_SIZE },
        { crate::config::MESSAGE_BUFFER_SIZE },
    >,
>();
#[cfg(feature = "rmw-cffi")]
pub const SERVICE_CLIENT_OPAQUE_U64S: usize = u64s_for::<
    nros_node::RawServiceClient<
        { crate::config::MESSAGE_BUFFER_SIZE },
        { crate::config::MESSAGE_BUFFER_SIZE },
    >,
>();

// Phase 122.3.c.6 — typeless `ActionServerCore` / `ActionClientCore`
// inline storage for L1 polling-mode `nros_action_server_t` /
// `nros_action_client_t`. Buffer + goal-slot sizing matches the
// L2 callback path (`MESSAGE_BUFFER_SIZE` triple buffers,
// `MAX_GOALS = 4`).
#[cfg(feature = "rmw-cffi")]
pub const ACTION_SERVER_OPAQUE_U64S: usize = u64s_for::<
    nros_node::ActionServerCore<
        { crate::config::MESSAGE_BUFFER_SIZE },
        { crate::config::MESSAGE_BUFFER_SIZE },
        { crate::config::MESSAGE_BUFFER_SIZE },
        4,
    >,
>();
#[cfg(feature = "rmw-cffi")]
pub const ACTION_CLIENT_OPAQUE_U64S: usize = u64s_for::<
    nros_node::ActionClientCore<
        { crate::config::MESSAGE_BUFFER_SIZE },
        { crate::config::MESSAGE_BUFFER_SIZE },
        { crate::config::MESSAGE_BUFFER_SIZE },
    >,
>();

// Placeholders for no-RMW workspace builds.
#[cfg(not(feature = "rmw-cffi"))]
pub const SESSION_OPAQUE_U64S: usize = 1;
#[cfg(not(feature = "rmw-cffi"))]
pub const PUBLISHER_OPAQUE_U64S: usize = 1;
#[cfg(not(feature = "rmw-cffi"))]
pub const SUBSCRIPTION_OPAQUE_U64S: usize = 1;
#[cfg(not(feature = "rmw-cffi"))]
pub const SERVICE_SERVER_OPAQUE_U64S: usize = 1;
#[cfg(not(feature = "rmw-cffi"))]
pub const SERVICE_CLIENT_OPAQUE_U64S: usize = 1;
#[cfg(not(feature = "rmw-cffi"))]
pub const ACTION_SERVER_OPAQUE_U64S: usize = 1;
#[cfg(not(feature = "rmw-cffi"))]
pub const ACTION_CLIENT_OPAQUE_U64S: usize = 1;

// ── Guard Condition ──────────────────────────────────────────────────────

pub const GUARD_HANDLE_OPAQUE_U64S: usize = u64s_for::<nros_node::GuardCondition>();

// ── Opaque-storage guards (issue 0472) ───────────────────────────────────
//
// A C caller allocates `uint64_t _opaque[<MACRO>]` and the runtime writes a
// Rust value into it. The macro's value comes from PROBING a compiled rlib
// (`nros-build-helpers::c`); the value written is `size_of::<T>()`. Two
// derivations of one fact — and if the probe's is smaller, the write runs past
// the buffer. In C. At a distance from the cause.
//
// Exactly two of the fifteen macros carried an assertion before this
// (`EXECUTOR_OPAQUE_U64S`, `CPP_EXECUTOR_OPAQUE_U64S`), and the executor's had
// already earned its keep: issue 0464 records it catching a committed NuttX
// constant that had rotted ~11 % low. The rest could only fail as corruption.
//
// `>=`, not `==`: over-sizing wastes bytes and is safe, under-sizing is the
// bug. The probe's `max(8)` floor for the raw handles makes an exact
// comparison wrong for small types anyway.
//
// SKIPPED when the probe returned 0, which means "no rlib to read" — a
// `cargo check --no-default-features` run. Hard-failing that would break
// `just check`, and issue 0472 records the accommodation as legitimate; what it
// also records as MISSING is enforcement at link time, so a `1`-sized macro
// cannot be linked. That is its item 2 and is not addressed here.
// Gated to match its ONLY consumer, the `rmw-cffi` block below. Without this a
// `--no-default-features` build (which `check-workspace-features` runs) cfg's
// every invocation out and leaves the definition unused, which `-D warnings`
// rejects as `unused_macros`. Gating the definition rather than `#[allow]`ing
// it keeps definition and uses on ONE condition, so a future use outside that
// cfg fails loudly instead of silently compiling against nothing.
//
// A SECOND trigger, found independently on the feature-contract branch: with
// this crate's `default` emptied, a plain `cargo check -p nros-c` resolves
// without `rmw-cffi` too — so the lane is not the only way in.
#[cfg(feature = "rmw-cffi")]
macro_rules! guard_opaque {
    ($stated:expr, $ty:ty, $what:literal) => {
        const _: () = assert!(
            $stated == 0 || size_of::<$ty>() <= $stated * 8,
            concat!(
                $what,
                ": the generated header states a SMALLER opaque size than the Rust type needs, ",
                "so a C caller's `_opaque` buffer would be written past. The header value is ",
                "probe-derived (nros-build-helpers::c); the type's size is the truth. Rebuild ",
                "with a clean `build/sizes-probe`, and see issue 0472."
            )
        );
    };
}

#[cfg(feature = "rmw-cffi")]
const _: () = {
    guard_opaque!(
        crate::config::PROBE_SESSION_U64S,
        nros::internals::RmwSession,
        "SESSION_OPAQUE_U64S"
    );
    guard_opaque!(
        crate::config::PROBE_PUBLISHER_U64S,
        nros::internals::RmwPublisher,
        "PUBLISHER_OPAQUE_U64S"
    );
    guard_opaque!(
        crate::config::PROBE_RAW_SUBSCRIPTION_U64S,
        nros_node::RawSubscription<{ crate::config::MESSAGE_BUFFER_SIZE }>,
        "SUBSCRIPTION_OPAQUE_U64S"
    );
    guard_opaque!(
        crate::config::PROBE_RAW_SERVICE_SERVER_U64S,
        nros_node::RawServiceServer<
            { crate::config::MESSAGE_BUFFER_SIZE },
            { crate::config::MESSAGE_BUFFER_SIZE },
        >,
        "SERVICE_SERVER_OPAQUE_U64S"
    );
    guard_opaque!(
        crate::config::PROBE_RAW_SERVICE_CLIENT_U64S,
        nros_node::RawServiceClient<
            { crate::config::MESSAGE_BUFFER_SIZE },
            { crate::config::MESSAGE_BUFFER_SIZE },
        >,
        "SERVICE_CLIENT_OPAQUE_U64S"
    );
    guard_opaque!(
        crate::config::PROBE_RAW_ACTION_SERVER_U64S,
        nros_node::ActionServerCore<
            { crate::config::MESSAGE_BUFFER_SIZE },
            { crate::config::MESSAGE_BUFFER_SIZE },
            { crate::config::MESSAGE_BUFFER_SIZE },
            4,
        >,
        "ACTION_SERVER_OPAQUE_U64S"
    );
    guard_opaque!(
        crate::config::PROBE_RAW_ACTION_CLIENT_U64S,
        nros_node::ActionClientCore<
            { crate::config::MESSAGE_BUFFER_SIZE },
            { crate::config::MESSAGE_BUFFER_SIZE },
            { crate::config::MESSAGE_BUFFER_SIZE },
        >,
        "ACTION_CLIENT_OPAQUE_U64S"
    );
    // These two types are backend-independent, but the guard still rides the
    // same cfg: `crate::config` is the GENERATED module, and it exists only
    // under `rmw-cffi` (lib.rs). Without the feature there is no probe, no
    // emitted header and nothing linking against one, so there is nothing to
    // disagree about.
    guard_opaque!(
        crate::config::PROBE_GUARD_HANDLE_U64S,
        nros_node::GuardCondition,
        "GUARD_HANDLE_OPAQUE_U64S"
    );
    guard_opaque!(
        crate::config::PROBE_LIFECYCLE_CTX_U64S,
        nros_node::lifecycle::LifecyclePollingNodeCtx,
        "NROS_LIFECYCLE_CTX_OPAQUE_U64S"
    );
};

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
#[cfg(feature = "rmw-cffi")]
const _: () = assert!(
    size_of::<crate::action::ActionServerInternal>()
        == size_of::<nros::sizes::ActionServerInternalLayout>(),
    "ActionServerInternal size diverges from nros::sizes::ActionServerInternalLayout — \
     update the layout mirror in `nros/src/sizes.rs` to track any field-shape change"
);
