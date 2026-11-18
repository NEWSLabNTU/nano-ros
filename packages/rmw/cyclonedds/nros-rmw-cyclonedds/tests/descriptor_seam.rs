//! Phase 248 (C2) — the generic descriptor seam reaches this backend's
//! registry after `install_descriptor_registrar`.
//!
//! Replaces the former `nros-node/tests/cyclonedds_register_smoke.rs`
//! (which depended on `nros-rmw-cyclonedds` from the platform/RMW-agnostic
//! core). This lives in the Cyclone crate's own integration-test binary —
//! a separate process from the lib unit tests, so it does not race the
//! shared global `BUILD_COUNTER` those tests assert on.
//!
//! Gated on `bridge-stub` (run via
//! `cargo test -p nros-rmw-cyclonedds --features bridge-stub`), which
//! supplies the C++ descriptor-bridge symbols without the real `libddsc`.
#![cfg(feature = "bridge-stub")]

use nros_rmw_cyclonedds::{
    TypeRegistry, global, install_descriptor_registrar, sync::RegistryMutexExt,
};
use nros_serdes::schema::{Field, FieldType, Message};

struct Foo;
impl Message for Foo {
    const TYPE_NAME: &'static str = "smoke_msgs/msg/Foo\0";
    const FIELDS: &'static [Field] = &[Field {
        name: "data\0",
        ty: FieldType::Int32,
        offset: 0,
    }];
}

#[test]
fn generic_seam_populates_registry_after_install() {
    global().with(|r: &mut TypeRegistry| r.clear_for_test());
    nros_rmw::set_type_descriptor_registrar(None);
    assert!(!nros_rmw::has_type_descriptor_registrar());

    // The Cyclone backend installs its registrar from its own crate.
    install_descriptor_registrar();
    assert!(nros_rmw::has_type_descriptor_registrar());

    // Before: empty.
    assert!(
        global()
            .with(|r: &mut TypeRegistry| r.get(<Foo as Message>::TYPE_NAME))
            .is_none()
    );

    // Act through the generic, core-facing seam (what `nros-node`'s
    // `register_type::<M>()` calls).
    nros_rmw::register_type_descriptor(<Foo as Message>::TYPE_NAME, <Foo as Message>::FIELDS)
        .expect("seam registration should succeed under the bridge stub");

    // After: descriptor cached + non-NULL.
    let ptr = global()
        .with(|r: &mut TypeRegistry| r.get(<Foo as Message>::TYPE_NAME))
        .expect("registry should hold Foo after seam registration");
    assert!(!ptr.is_null());

    // Idempotent cache hit.
    nros_rmw::register_type_descriptor(<Foo as Message>::TYPE_NAME, <Foo as Message>::FIELDS)
        .expect("second seam registration should be a cache hit");
    let ptr2 = global()
        .with(|r: &mut TypeRegistry| r.get(<Foo as Message>::TYPE_NAME))
        .expect("registry should still hold Foo");
    assert_eq!(ptr, ptr2);

    // Issue 2704 -- a type the C++ descriptor table has no room for is
    // REFUSED through the seam (the core fails the entity create with it and
    // names the type), not registered and dropped. In this test rather than
    // its own: the registrar slot is process-global and tests run in
    // parallel.
    assert!(
        <Full as Message>::TYPE_NAME
            .starts_with(nros_rmw_cyclonedds::bridge::test_stub::TABLE_FULL_PREFIX)
    );
    assert_eq!(
        nros_rmw::register_type_descriptor(<Full as Message>::TYPE_NAME, <Full as Message>::FIELDS),
        Err(nros_rmw::TransportError::PublisherCreationFailed),
        "a registration that does not fit the table must refuse"
    );
    assert!(
        global()
            .with(|r: &mut TypeRegistry| r.get(<Full as Message>::TYPE_NAME))
            .is_none(),
        "the refused type is not cached as if it were registered"
    );

    nros_rmw::set_type_descriptor_registrar(None);
}

struct Full;
impl Message for Full {
    const TYPE_NAME: &'static str = "table_full_msgs/msg/Full\0";
    const FIELDS: &'static [Field] = &[Field {
        name: "data\0",
        ty: FieldType::Int32,
        offset: 0,
    }];
}
