//! Phase 212.K.7.6.b — `nros_rmw_cyclonedds::register::<M>()` bridge.
//!
//! The Cyclone DDS RMW backend resolves topic-type descriptors via a
//! runtime registry (`nros-rmw-cyclonedds::type_registry`) instead of the
//! legacy static-init `descriptors.cpp` table. Each `nros-node` typed
//! creator (`create_publisher`, `create_subscription`, `create_client`,
//! `create_service`, `create_action_*`) routes through
//! [`register_type::<M>`] *before* asking the cffi vtable to create the
//! entity so the descriptor exists in the registry when
//! `dds_create_topic` runs in the C++ bridge.
//!
//! # cfg gating (Phase 214.S.2 — auto-detected, not feature-gated)
//!
//! This module compiles to no-ops unless `cfg(rmw_cyclonedds_present)`
//! is on. The cfg is emitted by `nros-node/build.rs` when:
//!
//! * the sibling `nros-rmw-cyclonedds-sys` shim crate (which carries
//!   `links = "cyclonedds"`) is in the dep graph, surfacing
//!   `DEP_CYCLONEDDS_PRESENT=1` to our build script; **or**
//! * the private internal `__cyclonedds-link` feature is on (the umbrella
//!   `nros/rmw-cyclonedds` activates it; this guarantees a direct edge to
//!   the linking crate exists, since cargo's `DEP_*` env-vars only
//!   propagate to *direct* dependents).
//!
//! Net effect: callers depend on `nros = { features = ["rmw-cyclonedds"] }`
//! and the K.7.6.b hook lights up automatically — no user-facing feature
//! flag on `nros-node` (was: `feature = "rmw-cyclonedds"`, dropped in
//! Phase 214.S.2). Each typed creator calls [`register_type::<M>`]
//! unconditionally; the body is empty when the cfg is off so zenoh/xrce
//! paths pay nothing. With the cfg on, the caller pays one mutex
//! acquisition + one `FnvIndexMap` lookup per creator invocation
//! (idempotent; the registry caches the descriptor pointer on the
//! first hit).
//!
//! # Trait bound — [`MessageForRmw`]
//!
//! The cyclonedds registry needs [`nros_serdes::schema::Message`] for its
//! static field schema, but `nros-node`'s typed creators historically
//! only constrain `M: nros_core::RosMessage`. Adding `Message` as a
//! super-bound on `RosMessage` breaks every existing codegen-emitted msg
//! crate (they impl `RosMessage` but not yet `Message`). Adding it as a
//! per-method bound on every typed creator touches 30+ sites.
//!
//! Compromise: introduce a sealed helper trait
//! [`MessageForRmw`] that is **the bound the typed creators use** in
//! place of bare `M: RosMessage`. It is a blanket impl over
//! `RosMessage` whose extra requirement is `Message` when
//! `cfg(rmw_cyclonedds_present)` is on, and just `RosMessage` when off.
//!
//! Net effect: a msg crate that impls `RosMessage` works as-is for
//! zenoh + xrce builds; for cyclonedds builds it must additionally impl
//! `Message`. The codegen template (`nros-cli` K.7.1.b — separate
//! agent / repo) emits both impls for every generated msg crate.
//!
//! # Error mapping
//!
//! `nros_rmw_cyclonedds::BuildError` flattens onto
//! [`crate::NodeError::Transport`] with
//! `TransportError::PublisherCreationFailed`. Backend-specific diagnostic
//! is lost — the underlying `BuildError` is logged via the `log` crate
//! when that feature is enabled. The choice not to add a dedicated
//! `NodeError::CyclonedsTypeRegistrationFailed` variant is deliberate:
//! the C/C++ FFI shim widens to a single `nros_ret_t`, and the failure
//! mode (out-of-capacity registry, bridge error, etc.) is a "topic could
//! not be created" from the caller's perspective.

use nros_core::RosMessage;

// ============================================================================
// Phase 214.S.4.b — link-graph keep-alive for `nros-rmw-cyclonedds-sys`
// ============================================================================
//
// The Cyclone backend's C++ register entry (`nros_rmw_cyclonedds_register`)
// and its linkme self-register section live inside `nros-rmw-cyclonedds-sys`
// (and its C++ companion `nros-rmw-cyclonedds`). Without an explicit Rust-
// side reference to a symbol from that crate, rust-lld drops the rlib from
// the final link and the C++ static-init register TU is pruned along with
// it.
//
// Before S.4.b, example `src/main.rs` carried `nros_rmw_cyclonedds_sys::
// register()` directly which doubled as the symbol-drag. S.4.b moves that
// keep-alive in here so user crates can collapse their `rmw-cyclonedds`
// feature to strict 1-entry parity (`["nros/rmw-cyclonedds"]`) with the
// zenoh / xrce siblings — no `dep:nros-rmw-cyclonedds-sys` activator and
// no direct `register()` call in user code. The linkme self-register
// section inside `-sys` (Phase 128.B.3) fires the register on
// `nros::init` via the cffi-rmw walker.
//
// Mirrors the existing pattern in `nros-platform::__FORCE_LINK_CFFI`
// (`packages/core/nros-platform/src/lib.rs:83`) which keeps
// `nros-platform-cffi` alive through `nros-rmw-zenoh`.
//
// Gated on a separate cfg from `rmw_cyclonedds_present` because the
// `cyclonedds_register_smoke` test enables the latter via the lighter
// `__cyclonedds-detect` feature path (no `-sys` dep, bridge symbols
// from `nros-rmw-cyclonedds[bridge-stub]` dev-dep). Production
// callers go through the umbrella's `__cyclonedds-link` which emits
// BOTH `rmw_cyclonedds_present` AND `cyclonedds_link_keepalive`.
// Pinning the `-sys` rlib under the bare detect path would force
// rust-lld to whole-archive the production C++ libnros_rmw_cyclonedds
// in alongside the bridge-stub Rust impls → duplicate-symbol link
// failure on the smoke test.
#[cfg(cyclonedds_link_keepalive)]
#[used]
#[doc(hidden)]
pub static __FORCE_LINK_CYCLONEDDS_SYS: fn() -> Result<(), nros_rmw_cyclonedds_sys::RegisterError> =
    nros_rmw_cyclonedds_sys::register;

/// Bound used in place of bare `RosMessage` on typed creators.
///
/// Equivalent to `RosMessage` without `cfg(rmw_cyclonedds_present)`;
/// equal to `RosMessage + nros_serdes::schema::Message` with it.
///
/// See module-level docs for the rationale.
#[cfg(rmw_cyclonedds_present)]
pub trait MessageForRmw: RosMessage + nros_serdes::schema::Message {}

#[cfg(rmw_cyclonedds_present)]
impl<T> MessageForRmw for T where T: RosMessage + nros_serdes::schema::Message {}

#[cfg(not(rmw_cyclonedds_present))]
pub trait MessageForRmw: RosMessage {}

#[cfg(not(rmw_cyclonedds_present))]
impl<T> MessageForRmw for T where T: RosMessage {}

// ============================================================================
// register_type::<M>() — the K.7.6.b hook
// ============================================================================

/// Register `M`'s cyclonedds topic descriptor with the runtime registry.
///
/// No-op when `cfg(rmw_cyclonedds_present)` is off. With the cfg on,
/// delegates to `nros_rmw_cyclonedds::register::<M>()`. The first call
/// for a given `M::TYPE_NAME` builds the descriptor via the C++ bridge
/// and caches it; subsequent calls are O(1) lookups.
///
/// Returns `Ok(())` on success or `NodeError::Transport(
/// TransportError::PublisherCreationFailed)` on any
/// [`nros_rmw_cyclonedds::BuildError`].
#[allow(unused_variables)] // M unused without the cfg
#[inline]
pub fn register_type<M: MessageForRmw>() -> Result<(), crate::NodeError> {
    #[cfg(rmw_cyclonedds_present)]
    {
        // SAFETY-OF-INPUT: `M: Message` is enforced by the `MessageForRmw`
        // bound under `rmw_cyclonedds_present`, so `register::<M>()`
        // receives the schema it expects.
        nros_rmw_cyclonedds::register::<M>().map_err(|err| {
            #[cfg(feature = "log")]
            log::error!(
                "nros_rmw_cyclonedds::register::<{}>() failed: {:?}",
                <M as nros_serdes::schema::Message>::TYPE_NAME,
                err
            );
            let _ = err; // silence unused without log
            crate::NodeError::Transport(nros_rmw::TransportError::PublisherCreationFailed)
        })?;
    }
    Ok(())
}
