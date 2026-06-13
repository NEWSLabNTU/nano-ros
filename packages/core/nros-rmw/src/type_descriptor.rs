//! Phase 248 (C2) — generic per-type descriptor registration seam.
//!
//! Some RMW backends (Cyclone DDS) need to build a per-message-type
//! descriptor at runtime *before* the topic/endpoint is created, so the
//! wire (de)serialiser knows the type's field layout. Zenoh and XRCE
//! don't — they serialise the CDR payload opaquely.
//!
//! Historically the core executor (`nros-node`) reached this need by
//! depending directly on `nros-rmw-cyclonedds` and calling
//! `nros_rmw_cyclonedds::register::<M>()` from every typed creator. That
//! baked a concrete-RMW-named crate into the platform/RMW-agnostic core
//! (issue #60, Tier 1).
//!
//! This module replaces that named dependency with a runtime-pluggable
//! hook, mirroring [`crate::custom_transport`]'s vtable pattern:
//!
//! * the descriptor-needing backend installs a [`TypeDescriptorRegistrar`]
//!   via [`set_type_descriptor_registrar`] when it initialises (from its
//!   own crate — `nros-rmw-cyclonedds`);
//! * the core executor calls [`register_type_descriptor`] from each typed
//!   creator before asking the cffi vtable to create the entity.
//!
//! When no registrar is installed (zenoh / xrce builds, or any build
//! where no descriptor-needing backend is linked), [`register_type_descriptor`]
//! is a no-op returning `Ok(())`, so the core pays nothing.
//!
//! ## Why a fn-pointer slot, not a Rust trait
//!
//! Same rationale as [`crate::custom_transport`]: a `Box<dyn …>` would
//! force `alloc` onto every no_std backend, and the registration data
//! (`&'static str` type name + `&'static [Field]` schema) is already
//! `'static` POD that crosses the seam by value. The schema graph lives
//! in `.rodata`; the registrar never retains it past the call.

use nros_serdes::schema::Field;

use crate::{TransportError, sync::Mutex};

/// Runtime registrar installed by a backend that builds per-type
/// descriptors (e.g. Cyclone DDS).
///
/// Receives the message type's flattened schema — its ROS type name and
/// its `&'static` field table — and registers/builds the backend's
/// descriptor for it. Returns `Err(())` on any backend-side failure
/// (registry full, descriptor build error); the core maps that onto
/// [`TransportError::PublisherCreationFailed`].
///
/// Both arguments are `&'static` so the registrar may cache them by
/// reference in a bounded `'static` registry without copying.
pub type TypeDescriptorRegistrar =
    fn(type_name: &'static str, fields: &'static [Field]) -> Result<(), ()>;

/// Single-slot storage for the installed registrar. `None` until a
/// descriptor-needing backend installs one. Mirrors
/// [`crate::custom_transport`]'s `SLOT`.
static REGISTRAR: Mutex<Option<TypeDescriptorRegistrar>> = Mutex::new(None);

/// Install (or, with `None`, clear) the process-wide type-descriptor
/// registrar.
///
/// Called by the descriptor-needing backend during its initialisation
/// (for Cyclone DDS: `nros_rmw_cyclonedds::install_descriptor_registrar`,
/// driven by the backend's `RMW_INIT_ENTRIES` self-registration). Idempotent
/// — a second install overwrites the slot, which is what re-registering the
/// same backend does.
pub fn set_type_descriptor_registrar(registrar: Option<TypeDescriptorRegistrar>) {
    REGISTRAR.with(|slot| *slot = registrar);
}

/// Returns `true` when a descriptor-needing backend has installed a
/// registrar. Diagnostic / test helper.
pub fn has_type_descriptor_registrar() -> bool {
    REGISTRAR.with(|slot| slot.is_some())
}

/// Register message type `(type_name, fields)`'s descriptor with the
/// installed backend, if any.
///
/// No-op (`Ok(())`) when no registrar is installed — the common case for
/// zenoh / xrce builds. When a registrar is installed, forwards the
/// schema and maps a backend failure onto
/// [`TransportError::PublisherCreationFailed`].
///
/// The core executor calls this from every typed creator
/// (`create_publisher` / `_subscription` / `_client` / `_service` /
/// `create_action_*`) before creating the entity, so the descriptor
/// exists when the backend's `dds_create_topic` (or equivalent) runs.
pub fn register_type_descriptor(
    type_name: &'static str,
    fields: &'static [Field],
) -> Result<(), TransportError> {
    let registrar = REGISTRAR.with(|slot| *slot);
    match registrar {
        Some(reg) => reg(type_name, fields).map_err(|()| TransportError::PublisherCreationFailed),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use nros_serdes::schema::{Field, FieldType};

    static CALLS: AtomicUsize = AtomicUsize::new(0);

    const FIELDS: &[Field] = &[Field {
        name: "x\0",
        ty: FieldType::Int32,
        offset: 0,
    }];

    fn ok_registrar(_t: &'static str, _f: &'static [Field]) -> Result<(), ()> {
        CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn err_registrar(_t: &'static str, _f: &'static [Field]) -> Result<(), ()> {
        Err(())
    }

    /// No registrar installed → register_type_descriptor is a no-op.
    #[test]
    fn no_registrar_is_noop() {
        set_type_descriptor_registrar(None);
        assert!(!has_type_descriptor_registrar());
        assert_eq!(register_type_descriptor("pkg/msg/Foo\0", FIELDS), Ok(()));
    }

    /// Installed registrar is invoked and its error maps onto a transport error.
    #[test]
    fn installed_registrar_invoked() {
        CALLS.store(0, Ordering::SeqCst);
        set_type_descriptor_registrar(Some(ok_registrar));
        assert!(has_type_descriptor_registrar());
        assert_eq!(register_type_descriptor("pkg/msg/Foo\0", FIELDS), Ok(()));
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);

        set_type_descriptor_registrar(Some(err_registrar));
        assert_eq!(
            register_type_descriptor("pkg/msg/Foo\0", FIELDS),
            Err(TransportError::PublisherCreationFailed)
        );

        // Clear so we don't leak the registrar into other tests sharing the
        // process-wide slot.
        set_type_descriptor_registrar(None);
    }
}
