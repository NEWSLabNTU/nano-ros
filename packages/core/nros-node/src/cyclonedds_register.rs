//! Per-type descriptor registration — generic seam (Phase 248 C2).
//!
//! Some RMW backends (Cyclone DDS) resolve topic-type descriptors via a
//! runtime registry instead of a static-init table. Each `nros-node`
//! typed creator (`create_publisher`, `create_subscription`,
//! `create_client`, `create_service`, `create_action_*`) routes through
//! [`register_type::<M>`] *before* asking the cffi vtable to create the
//! entity so the descriptor exists when the backend's `dds_create_topic`
//! (or equivalent) runs.
//!
//! # No named-backend dependency (issue #60, Tier 1)
//!
//! Previously this module depended directly on `nros-rmw-cyclonedds` and
//! called `nros_rmw_cyclonedds::register::<M>()`, baking a concrete-RMW
//! crate into the platform/RMW-agnostic core executor. It now forwards
//! the message's flattened schema through the generic
//! [`nros_rmw::register_type_descriptor`] seam. The Cyclone backend
//! installs its registrar from its own crate
//! (`nros-rmw-cyclonedds::install_descriptor_registrar`, driven by the
//! `-sys` shim's `RMW_INIT_ENTRIES` self-registration); zenoh / xrce
//! install nothing, so the seam is a no-op there.
//!
//! # cfg gating (auto-detected, not feature-gated)
//!
//! This module's schema-passing body compiles to a no-op unless
//! `cfg(rmw_cyclonedds_present)` is on. The cfg is emitted by
//! `nros-node/build.rs` from the private internal `__cyclonedds-link`
//! marker feature (no dep edge), which the umbrella `nros/rmw-cyclonedds`
//! activates alongside its own `dep:nros-rmw-cyclonedds-sys`. Callers
//! depend on `nros = { features = ["rmw-cyclonedds"] }`; the hook lights
//! up automatically — no user-facing feature flag on `nros-node`. Each
//! typed creator calls [`register_type::<M>`] unconditionally; the body
//! is empty when the cfg is off so zenoh/xrce paths pay nothing. With the
//! cfg on, the caller pays one mutex acquisition + one lookup per creator
//! invocation (idempotent; the backend caches the descriptor on first
//! hit).
//!
//! # Trait bound — [`MessageForRmw`]
//!
//! A descriptor-needing backend needs [`nros_serdes::schema::Message`]
//! for the static field schema, but `nros-node`'s typed creators
//! historically only constrain `M: nros_core::RosMessage`. Adding
//! `Message` as a super-bound on `RosMessage` breaks every existing
//! codegen-emitted msg crate (they impl `RosMessage` but not yet
//! `Message`). Adding it as a per-method bound on every typed creator
//! touches 30+ sites.
//!
//! Compromise: introduce a helper trait [`MessageForRmw`] that is **the
//! bound the typed creators use** in place of bare `M: RosMessage`. It is
//! a blanket impl over `RosMessage` whose extra requirement is `Message`
//! when `cfg(rmw_cyclonedds_present)` is on, and just `RosMessage` when
//! off.
//!
//! Net effect: a msg crate that impls `RosMessage` works as-is for zenoh
//! + xrce builds; for cyclonedds builds it must additionally impl
//! `Message`. The codegen template (`nros-cli` — separate repo) emits
//! both impls for every generated msg crate.
//!
//! # Error mapping
//!
//! A registrar failure flattens onto [`crate::NodeError::Transport`] with
//! [`nros_rmw::TransportError::PublisherCreationFailed`]. The choice not
//! to add a dedicated `NodeError` variant is deliberate: the C/C++ FFI
//! shim widens to a single `nros_ret_t`, and the failure mode
//! (out-of-capacity registry, descriptor build error, etc.) is a "topic
//! could not be created" from the caller's perspective.

use nros_core::RosMessage;

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

/// Register `M`'s topic-type descriptor with whichever RMW backend
/// installed the generic descriptor seam (`nros_rmw::register_type_descriptor`).
///
/// No-op when `cfg(rmw_cyclonedds_present)` is off (zenoh / xrce builds
/// never compile the schema-passing body). With the cfg on, flattens
/// `M`'s static schema (`TYPE_NAME` + `FIELDS`) and forwards it to the
/// installed [`nros_rmw::TypeDescriptorRegistrar`] — Cyclone DDS installs
/// one from its own crate (`nros-rmw-cyclonedds::install_descriptor_registrar`),
/// so the core executor no longer needs a named dependency on the Cyclone
/// shim. The first call for a given type builds + caches the descriptor;
/// subsequent calls are O(1) lookups inside the backend.
///
/// Returns `Ok(())` on success (including the "no descriptor-needing
/// backend installed" no-op), or
/// `NodeError::Transport(TransportError::PublisherCreationFailed)` on a
/// backend-side build/registry failure.
#[allow(unused_variables)] // M unused without the cfg
#[inline]
pub fn register_type<M: MessageForRmw>() -> Result<(), crate::NodeError> {
    #[cfg(rmw_cyclonedds_present)]
    {
        // SAFETY-OF-INPUT: `M: Message` is enforced by the `MessageForRmw`
        // bound under `rmw_cyclonedds_present`, so `TYPE_NAME` / `FIELDS`
        // are available and the registrar receives the schema it expects.
        nros_rmw::register_type_descriptor(
            <M as nros_serdes::schema::Message>::TYPE_NAME,
            <M as nros_serdes::schema::Message>::FIELDS,
        )
        .map_err(|err| {
            #[cfg(feature = "log")]
            log::error!(
                "nros_rmw::register_type_descriptor::<{}>() failed: {:?}",
                <M as nros_serdes::schema::Message>::TYPE_NAME,
                err
            );
            let _ = err; // silence unused without log
            crate::NodeError::Transport(err)
        })?;
    }
    Ok(())
}
