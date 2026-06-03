//! Phase 212.K.7.4.c — integration test: schemas with
//! `FieldType::Sequence(&Nested(...))` / `Array(N, &Nested(...))` /
//! `BoundedSequence(N, &Nested(...))` flow through the
//! `DescriptorBuilder` → bridge cleanly.
//!
//! Before K.7.4.c the bridge rejected every one of these shapes with
//! `BuildError::UnsupportedFieldType`, blocking native-Rust Cyclone
//! action e2e (action_msgs::srv::dds_::CancelGoalResponse). After
//! K.7.4.c they all return a non-NULL descriptor pointer and the
//! type-registry caches it on the second call.
//!
//! Uses the integration-test bridge stub (same as `registry_smoke.rs`)
//! — the C++ TU is gated behind the `bridge-stub` feature when run via
//! `cargo test --features bridge-stub,std`; otherwise the local
//! no-op stub below stands in.

#[cfg(not(feature = "bridge-stub"))]
use core::ffi::{c_char, c_int, c_void};

use nros_rmw_cyclonedds::{
    dynamic_type::{BuildError, DescriptorBuilder},
    type_registry::TypeRegistry,
};
use nros_serdes::schema::{Field, FieldType, Message, NestedType};

#[cfg(not(feature = "bridge-stub"))]
static STUB_BACKING: u8 = 0;

#[cfg(not(feature = "bridge-stub"))]
#[unsafe(no_mangle)]
extern "C" fn nros_cyclonedds_build_descriptor_from_schema(
    _type_name: *const c_char,
    _fields: *const u8,
    _field_count: u32,
    _kinds: *const u8,
    _kind_count: u32,
    _out_err: *mut c_int,
) -> *const c_void {
    &STUB_BACKING as *const u8 as *const c_void
}

#[cfg(not(feature = "bridge-stub"))]
#[unsafe(no_mangle)]
extern "C" fn nros_rmw_cyclonedds_register_descriptor(
    _type_name: *const c_char,
    _descriptor: *const c_void,
) {
}

// ── Fixture: a CancelGoalResponse-shaped schema ─────────────────────────
//
// Mirrors the blocker type from the design doc:
//   action_msgs/msg/dds_/CancelGoal_Response_ {
//     int8 return_code;
//     sequence<GoalInfo> goals_canceling;
//   }
//   GoalInfo { uint32 a; uint32 b; }  // simplified, no UUID/Time
//
// The real CancelGoal_Response has a nested-EXT chain (GoalInfo →
// goal_id:UUID + stamp:Time); a 2-primitive child suffices to prove
// the SEQ|STU emitter — the EXT path was already working pre-K.7.4.c.

const GOAL_INFO_NESTED: NestedType = NestedType {
    type_name: "action_msgs/msg/GoalInfo",
    fields: &[
        Field {
            name: "a",
            ty: FieldType::Uint32,
            offset: 0,
        },
        Field {
            name: "b",
            ty: FieldType::Uint32,
            offset: 4,
        },
    ],
};

const GOAL_INFO_TY: FieldType = FieldType::Nested(&GOAL_INFO_NESTED);

struct CancelGoalResponseLike;
impl Message for CancelGoalResponseLike {
    const TYPE_NAME: &'static str = "action_msgs/msg/CancelGoalResponseLike";
    const FIELDS: &'static [Field] = &[
        Field {
            name: "return_code",
            ty: FieldType::Int8,
            offset: 0,
        },
        Field {
            name: "goals_canceling",
            ty: FieldType::Sequence(&GOAL_INFO_TY),
            offset: 8,
        },
    ];
}

struct ArrayOfNested;
impl Message for ArrayOfNested {
    const TYPE_NAME: &'static str = "test_msgs/msg/ArrayOfNested";
    const FIELDS: &'static [Field] = &[Field {
        name: "arr",
        ty: FieldType::Array(3, &GOAL_INFO_TY),
        offset: 0,
    }];
}

struct BsqOfNested;
impl Message for BsqOfNested {
    const TYPE_NAME: &'static str = "test_msgs/msg/BsqOfNested";
    const FIELDS: &'static [Field] = &[Field {
        name: "bseq",
        ty: FieldType::BoundedSequence(4, &GOAL_INFO_TY),
        offset: 0,
    }];
}

#[test]
fn sequence_of_nested_builds_pre_k74c_blocker() {
    // Pre-K.7.4.c: BuildError::UnsupportedFieldType. Post: Ok.
    let ptr = DescriptorBuilder::build::<CancelGoalResponseLike>().expect("seq<nested> builds");
    assert!(!ptr.is_null());
}

#[test]
fn array_of_nested_builds() {
    let ptr = DescriptorBuilder::build::<ArrayOfNested>().expect("array<nested> builds");
    assert!(!ptr.is_null());
}

#[test]
fn bounded_sequence_of_nested_builds() {
    let ptr = DescriptorBuilder::build::<BsqOfNested>().expect("bseq<nested> builds");
    assert!(!ptr.is_null());
}

#[test]
fn registry_caches_sequence_of_nested_descriptor() {
    // Registry-level round-trip: first call builds + caches, second
    // call returns the same non-null pointer without re-entering the
    // bridge. Uses a fresh local TypeRegistry instance so we don't
    // collide with the global registry shared across tests.
    let mut registry = TypeRegistry::new();
    let p1 = registry
        .get_or_build::<CancelGoalResponseLike>()
        .expect("first lookup");
    assert!(!p1.is_null());
    let p2 = registry
        .get_or_build::<CancelGoalResponseLike>()
        .expect("second lookup");
    assert_eq!(p1, p2, "second lookup must hit the cache");
}

#[test]
fn primitive_sequence_still_works_post_k74c() {
    // Regression guard: the primitive fast-path inside
    // FieldKind::Sequence wasn't broken by the new NESTED branch.
    struct Pubsub;
    const U8_TY: FieldType = FieldType::Uint8;
    impl Message for Pubsub {
        const TYPE_NAME: &'static str = "std_msgs/msg/UInt8MultiArrayLike";
        const FIELDS: &'static [Field] = &[Field {
            name: "data",
            ty: FieldType::Sequence(&U8_TY),
            offset: 0,
        }];
    }
    let ptr = DescriptorBuilder::build::<Pubsub>().expect("seq<u8> builds");
    assert!(!ptr.is_null());
    let _ = BuildError::DdsError(0); // assertion that BuildError is in scope
}
