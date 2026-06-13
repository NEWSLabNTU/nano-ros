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

    nros_rmw::set_type_descriptor_registrar(None);
}
