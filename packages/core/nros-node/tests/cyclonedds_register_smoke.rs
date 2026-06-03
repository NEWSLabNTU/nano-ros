//! Phase 212.K.7.6.b — smoke that the Rust typed creators trigger
//! `nros_rmw_cyclonedds::register::<M>()` so the global type registry
//! is populated **before** the cffi vtable would try to create the
//! Cyclone topic.
//!
//! Gated on the `rmw-cyclonedds` feature; without it the file
//! compiles to an empty `lib` (cargo test still passes, no asserts).
//!
//! Implementation note: we use the `bridge-stub` feature on
//! `nros-rmw-cyclonedds` (wired in via `nros-node`'s dev-dependency
//! shim, see `nros-node/Cargo.toml`) so the linker can resolve the
//! C++ bridge entries without dragging in the actual `libddsc` or the
//! Cyclone build. The stub hands back unique non-NULL pointers per
//! call, which is exactly what the registry expects to cache.

#![cfg(feature = "rmw-cyclonedds")]

use core::sync::atomic::Ordering;

use nros_core::{CdrReader, CdrWriter, DeserError, Deserialize, RosMessage, SerError, Serialize};
use nros_node::cyclonedds_register::register_type;
use nros_rmw_cyclonedds::{global, type_registry::TypeRegistry};
use nros_serdes::schema::{Field, FieldType, Message};

/// Tiny `Foo {data: i32}` fixture impls both `RosMessage` (so the cffi
/// vtable would accept it) and `Message` (so the cyclonedds registry
/// can build a descriptor).
#[derive(Debug, Clone, Default, PartialEq)]
#[repr(C)]
struct Foo {
    data: i32,
}

impl RosMessage for Foo {
    const TYPE_NAME: &'static str = "smoke_msgs::msg::dds_::Foo_";
    const TYPE_HASH: &'static str = "smoke_hash_0000";
}

impl Serialize for Foo {
    fn serialize(&self, w: &mut CdrWriter) -> Result<(), SerError> {
        w.write_i32(self.data)
    }
}

impl Deserialize for Foo {
    fn deserialize(r: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: r.read_i32()?,
        })
    }
}

impl Message for Foo {
    const TYPE_NAME: &'static str = "smoke_msgs/msg/Foo";
    const FIELDS: &'static [Field] = &[Field {
        name: "data",
        ty: FieldType::Int32,
        offset: 0,
    }];
}

#[test]
fn register_type_populates_global_registry() {
    // Clear any cross-test state and reset the bridge counter so this
    // test's assertions are independent of test order. Clearing the
    // registry is `#[cfg(test)]`-only on `TypeRegistry`; the
    // `with` closure uses the spin/critical-section mutex the
    // production path uses.
    use nros_rmw_cyclonedds::sync::RegistryMutexExt;
    global().with(|r: &mut TypeRegistry| r.clear_for_test());

    // Before: no entry.
    let before = global().with(|r: &mut TypeRegistry| r.get(<Foo as Message>::TYPE_NAME));
    assert!(before.is_none(), "registry should start empty for Foo");

    // Act: the K.7.6.b hook.
    register_type::<Foo>().expect("register_type::<Foo> should succeed under bridge stub");

    // After: descriptor cached. We don't care about the exact pointer
    // value (the stub hands out a backing-array index), only that it
    // is non-NULL and stable across lookups.
    let after = global()
        .with(|r: &mut TypeRegistry| r.get(<Foo as Message>::TYPE_NAME))
        .expect("registry should hold an entry for Foo after register_type");
    assert!(!after.is_null(), "descriptor pointer should be non-NULL");

    // Idempotency: a second call should return the same cached pointer.
    register_type::<Foo>().expect("second register_type::<Foo> should still succeed");
    let after2 = global()
        .with(|r: &mut TypeRegistry| r.get(<Foo as Message>::TYPE_NAME))
        .expect("registry should still hold an entry for Foo");
    assert_eq!(
        after, after2,
        "second register_type call should be a cache hit"
    );
}

#[test]
fn register_type_is_no_op_for_distinct_types() {
    use nros_rmw_cyclonedds::{bridge::test_stub::BUILD_COUNTER, sync::RegistryMutexExt};

    /// Second fixture so we can observe the bridge builder firing.
    #[derive(Debug, Clone, Default, PartialEq)]
    #[repr(C)]
    struct Bar {
        v: u32,
    }

    impl RosMessage for Bar {
        const TYPE_NAME: &'static str = "smoke_msgs::msg::dds_::Bar_";
        const TYPE_HASH: &'static str = "smoke_hash_0001";
    }

    impl Serialize for Bar {
        fn serialize(&self, w: &mut CdrWriter) -> Result<(), SerError> {
            w.write_u32(self.v)
        }
    }

    impl Deserialize for Bar {
        fn deserialize(r: &mut CdrReader) -> Result<Self, DeserError> {
            Ok(Self { v: r.read_u32()? })
        }
    }

    impl Message for Bar {
        const TYPE_NAME: &'static str = "smoke_msgs/msg/Bar";
        const FIELDS: &'static [Field] = &[Field {
            name: "v",
            ty: FieldType::Uint32,
            offset: 0,
        }];
    }

    global().with(|r: &mut TypeRegistry| r.clear_for_test());
    let baseline = BUILD_COUNTER.load(Ordering::SeqCst);

    register_type::<Bar>().expect("Bar register");
    let after_first = BUILD_COUNTER.load(Ordering::SeqCst);
    assert_eq!(
        after_first - baseline,
        1,
        "first register_type::<Bar> should call the bridge once"
    );

    // Second call is a cache hit — no new bridge invocation.
    register_type::<Bar>().expect("Bar re-register cache hit");
    let after_second = BUILD_COUNTER.load(Ordering::SeqCst);
    assert_eq!(
        after_second - baseline,
        1,
        "second register_type::<Bar> should NOT re-call the bridge"
    );
}
