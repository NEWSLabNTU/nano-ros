//! Phase 212.K.7.5 + K.7.8 — integration tests for the bounded type
//! registry, exercised against the in-crate `#[cfg(test)]` bridge
//! stub (no Cyclone DDS link required).
//!
//! Coexists with the C++ tests in `tests/*.cpp` (those are wired by
//! the sibling CMakeLists.txt). Cargo discovers `tests/*.rs`; the
//! C++ files are ignored.

#[cfg(not(feature = "bridge-stub"))]
use core::ffi::{c_char, c_int, c_void};

use nros_rmw_cyclonedds::dynamic_type::{BuildError, DescriptorBuilder};
use nros_serdes::schema::{Field, FieldType, Message, NestedType};

// Integration-test stub for the C++ bridge. Mirrors the in-crate
// `bridge::test_stub`; needs its own copy here because the
// `#[cfg(test)]` stub lives behind the lib crate's unit-test cfg,
// which doesn't apply to integration-test binaries. Pretend the
// build always succeeds and hand back a stable non-NULL pointer.
//
// Under the `bridge-stub` feature (K.7.6.b / K.7.8) the lib crate
// already exports these symbols from `bridge::test_stub`, so this
// local copy MUST be gated out to avoid a duplicate-symbol link
// failure.
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

// ── Fixture: a hand-rolled `impl Message` covering every FieldType
// variant. Stands in for what the K.7.6 codegen template (in the
// standalone `nros-cli` repo) will eventually emit on every msg
// crate.

struct Bench;

const INNER_NESTED: NestedType = NestedType {
    type_name: "test_msgs/msg/Inner",
    fields: &[
        Field {
            name: "a",
            ty: FieldType::Int32,
            offset: 0,
        },
        Field {
            name: "b",
            ty: FieldType::Float64,
            offset: 8,
        },
    ],
};

const I32_TY: FieldType = FieldType::Int32;
const U8_TY: FieldType = FieldType::Uint8;

impl Message for Bench {
    const TYPE_NAME: &'static str = "test_msgs/msg/Bench";
    const FIELDS: &'static [Field] = &[
        Field {
            name: "scalar",
            ty: FieldType::Int64,
            offset: 0,
        },
        Field {
            name: "text",
            ty: FieldType::String,
            offset: 8,
        },
        Field {
            name: "buf",
            ty: FieldType::BoundedString(64),
            offset: 16,
        },
        Field {
            name: "nested",
            ty: FieldType::Nested(&INNER_NESTED),
            offset: 80,
        },
        Field {
            name: "arr",
            ty: FieldType::Array(8, &I32_TY),
            offset: 96,
        },
        Field {
            name: "seq",
            ty: FieldType::Sequence(&U8_TY),
            offset: 128,
        },
        Field {
            name: "bseq",
            ty: FieldType::BoundedSequence(16, &U8_TY),
            offset: 144,
        },
    ];
}

#[test]
fn descriptor_builder_walks_complex_schema() {
    let ptr = DescriptorBuilder::build::<Bench>().expect("complex schema builds");
    assert!(!ptr.is_null());
}

#[test]
fn build_raw_rejects_too_many_fields() {
    // `MAX_FIELDS` default is 64. Force overflow via a long fixed
    // schema. Use a `&'static [Field; N]` literal so the test
    // doesn't allocate.
    const TOO_MANY: &[Field] = &[Field {
        name: "f",
        ty: FieldType::Bool,
        offset: 0,
    }; 65];
    let err = DescriptorBuilder::build_raw("test_msgs/msg/Big", TOO_MANY).unwrap_err();
    assert_eq!(err, BuildError::FieldsOverflow);
}

#[test]
fn nested_depth_guard_kicks_in() {
    // Hand-build a chain of `Nested` types deeper than
    // `MAX_NESTED_DEPTH`. Default is 8; force depth = 9.
    const fn leaf() -> NestedType {
        NestedType {
            type_name: "test_msgs/msg/Leaf",
            fields: &[Field {
                name: "x",
                ty: FieldType::Int32,
                offset: 0,
            }],
        }
    }
    // Build a static chain manually (no recursion in const yet).
    static N0: NestedType = leaf();
    static F0: [Field; 1] = [Field {
        name: "n",
        ty: FieldType::Nested(&N0),
        offset: 0,
    }];
    static N1: NestedType = NestedType {
        type_name: "n1",
        fields: &F0,
    };
    static F1: [Field; 1] = [Field {
        name: "n",
        ty: FieldType::Nested(&N1),
        offset: 0,
    }];
    static N2: NestedType = NestedType {
        type_name: "n2",
        fields: &F1,
    };
    static F2: [Field; 1] = [Field {
        name: "n",
        ty: FieldType::Nested(&N2),
        offset: 0,
    }];
    static N3: NestedType = NestedType {
        type_name: "n3",
        fields: &F2,
    };
    static F3: [Field; 1] = [Field {
        name: "n",
        ty: FieldType::Nested(&N3),
        offset: 0,
    }];
    static N4: NestedType = NestedType {
        type_name: "n4",
        fields: &F3,
    };
    static F4: [Field; 1] = [Field {
        name: "n",
        ty: FieldType::Nested(&N4),
        offset: 0,
    }];
    static N5: NestedType = NestedType {
        type_name: "n5",
        fields: &F4,
    };
    static F5: [Field; 1] = [Field {
        name: "n",
        ty: FieldType::Nested(&N5),
        offset: 0,
    }];
    static N6: NestedType = NestedType {
        type_name: "n6",
        fields: &F5,
    };
    static F6: [Field; 1] = [Field {
        name: "n",
        ty: FieldType::Nested(&N6),
        offset: 0,
    }];
    static N7: NestedType = NestedType {
        type_name: "n7",
        fields: &F6,
    };
    static F7: [Field; 1] = [Field {
        name: "n",
        ty: FieldType::Nested(&N7),
        offset: 0,
    }];
    static N8: NestedType = NestedType {
        type_name: "n8",
        fields: &F7,
    };
    // Top-level uses N8 → walker enters at depth=0, recurses 8 more
    // times to reach the deepest leaf = depth 9 > MAX_NESTED_DEPTH 8.
    static F8: [Field; 1] = [Field {
        name: "n",
        ty: FieldType::Nested(&N8),
        offset: 0,
    }];

    let err = DescriptorBuilder::build_raw("test_msgs/msg/Deep", &F8).unwrap_err();
    assert_eq!(err, BuildError::NestedDepthExceeded);
}
