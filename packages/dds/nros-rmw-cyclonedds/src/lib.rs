//! Phase 212.K.7 — Rust shim for the Cyclone DDS RMW backend.
//!
//! Co-located with the C++ implementation under
//! `packages/dds/nros-rmw-cyclonedds/`: the C++ TUs (`src/*.cpp`) are
//! built by the sibling `CMakeLists.txt`; this crate adds the Rust
//! side that replaces the per-msg-pkg `cyclonedds` Cargo feature
//! pattern. See `docs/roadmap/phase-212-…-file-consolidation.md`
//! §212.K.7 for the design.
//!
//! # Surface
//!
//! * [`dynamic_type::DescriptorBuilder`] — walks
//!   [`nros_serdes::schema::Message`]'s static field schema and asks
//!   the C++ bridge to construct a Cyclone `dds_topic_descriptor_t`
//!   at runtime via Cyclone's dynamic-type API.
//! * [`type_registry::TypeRegistry`] — bounded
//!   [`heapless::FnvIndexMap`] of type-name → descriptor pointer. The
//!   global [`type_registry::global`] is mutex-guarded; first
//!   [`type_registry::TypeRegistry::get_or_build`] for a given `M`
//!   builds via `DescriptorBuilder` and caches the result.
//!
//! # Contract
//!
//! Hard constraints (enforced by CI + acceptance):
//!
//! * `#![no_std]`. No `alloc`, no `std`. The `std` feature is a
//!   reserved escape hatch for future hosted-only diagnostics; nothing
//!   in this crate touches it today.
//! * Storage is fixed-capacity [`heapless`]; overflow returns
//!   [`dynamic_type::BuildError::RegistryFull`] (no panic, no heap).
//! * Synchronisation: [`spin::Mutex`] on hosted POSIX/macOS,
//!   [`critical_section::Mutex`] on `target_os = "none"`. See
//!   [`sync`].
//! * The C++ bridge allocates descriptors from Cyclone's `ddsrt`
//!   heap (Phase 177.22's pre-budgeted pool on embedded targets), not
//!   from libc malloc.
//!
//! # Integration status
//!
//! K.7.6 ("wire pub/sub paths") is partially landed: the registry
//! exposes [`type_registry::register`] as the public entry, but the
//! existing C++ pub/sub paths
//! (`packages/dds/nros-rmw-cyclonedds/src/publisher.cpp`,
//! `subscriber.cpp`, `service.cpp`) still resolve descriptors through
//! the static-init [`descriptors.cpp`](../descriptors.cpp) table.
//! Wiring the Rust registry into the Rust-side `nros-node`
//! `create_publisher` / `create_subscription` paths is the K.7.7
//! / K.7.6 follow-up that lands once `nros-cli`'s codegen
//! template emits `impl Message for …` on every generated msg
//! crate (K.7.1). Until then, the registry is exercised by the
//! unit tests in `tests/`.
//!
//! # `#[cfg(test)]` shim
//!
//! Tests build under `cargo test -p nros-rmw-cyclonedds` on a hosted
//! toolchain that has no Cyclone DDS C library available. The
//! [`bridge`] module exposes a `#[cfg(test)]` stub for
//! `nros_cyclonedds_build_descriptor_from_schema` so the registry +
//! builder logic can be tested in isolation. Production builds link
//! the real symbol from the C++ TU added in
//! `bridge/dynamic_type_builder.cpp`.

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod bridge;
pub mod dynamic_type;
pub mod sync;
pub mod type_registry;

pub use dynamic_type::{BuildError, DescriptorBuilder};
pub use type_registry::{TypeRegistry, global, register, register_raw};

/// Phase 248 (C2) — the generic descriptor registrar this backend
/// installs into [`nros_rmw`]'s type-descriptor seam.
///
/// Wraps [`type_registry::register_raw`] and flattens its
/// [`BuildError`] to the seam's unit-error contract (the core maps
/// that onto `TransportError::PublisherCreationFailed`).
fn cyclonedds_type_descriptor_registrar(
    type_name: &'static str,
    fields: &'static [nros_serdes::schema::Field],
) -> Result<(), ()> {
    register_raw(type_name, fields)
        .map(|_ptr| ())
        .map_err(|_e| ())
}

/// Phase 248 (C2) — install this backend's per-type descriptor
/// registrar into the generic [`nros_rmw`] seam.
///
/// Call once during backend initialisation, before any Executor opens.
/// The Cyclone `-sys` shim drives this from its `RMW_INIT_ENTRIES`
/// self-registration (and from `register()`), so the platform/RMW-
/// agnostic core (`nros-node`) reaches Cyclone's descriptor builder
/// purely through `nros_rmw::register_type_descriptor` — no named
/// dependency on this crate.
pub fn install_descriptor_registrar() {
    nros_rmw::set_type_descriptor_registrar(Some(cyclonedds_type_descriptor_registrar));
}
