//! Integration tests for nros code generation

use rosidl_codegen::{
    CapacityResolver, RosEdition, generate_nros_message_package, generate_nros_service_package,
};
use rosidl_parser::{parse_message, parse_service};
use std::collections::BTreeSet;

#[test]
fn test_generate_std_msgs_int32() {
    let msg_content = "int32 data";
    let msg = parse_message(msg_content).expect("Failed to parse Int32");
    let deps = BTreeSet::new();

    let result = generate_nros_message_package(
        "std_msgs",
        "Int32",
        &msg,
        &deps,
        "5.3.0",
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    );
    assert!(result.is_ok(), "Generation failed: {:?}", result.err());

    let pkg = result.unwrap();

    // Verify Cargo.toml
    assert!(pkg.cargo_toml.contains("name = \"std_msgs\""));
    assert!(pkg.cargo_toml.contains("version = \"5.3.0\""));
    assert!(pkg.cargo_toml.contains("nros-core"));
    assert!(pkg.cargo_toml.contains("nros-serdes"));
    assert!(pkg.cargo_toml.contains("heapless"));

    // Verify lib.rs
    assert!(pkg.lib_rs.contains("#![no_std]"));

    // Verify message
    assert!(pkg.message_rs.contains("pub struct Int32"));
    assert!(pkg.message_rs.contains("pub data: i32"));
    assert!(pkg.message_rs.contains("impl Serialize for Int32"));
    assert!(pkg.message_rs.contains("impl Deserialize for Int32"));
    assert!(pkg.message_rs.contains("impl RosMessage for Int32"));
    assert!(pkg.message_rs.contains("writer.write_i32(self.data)?"));
    assert!(pkg.message_rs.contains("data: reader.read_i32()?"));
}

#[test]
fn test_generate_std_msgs_string() {
    let msg_content = "string data";
    let msg = parse_message(msg_content).expect("Failed to parse String");
    let deps = BTreeSet::new();

    let result = generate_nros_message_package(
        "std_msgs",
        "String",
        &msg,
        &deps,
        "5.3.0",
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    );
    assert!(result.is_ok(), "Generation failed: {:?}", result.err());

    let pkg = result.unwrap();

    // Verify string uses heapless::String
    assert!(pkg.message_rs.contains("heapless::String<256>"));
    assert!(
        pkg.message_rs
            .contains("writer.write_string(self.data.as_str())?")
    );
}

#[test]
fn test_generate_std_msgs_header() {
    let msg_content = "builtin_interfaces/Time stamp\nstring frame_id";
    let msg = parse_message(msg_content).expect("Failed to parse Header");
    let deps = BTreeSet::new();

    let result = generate_nros_message_package(
        "std_msgs",
        "Header",
        &msg,
        &deps,
        "5.3.0",
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    );
    assert!(result.is_ok(), "Generation failed: {:?}", result.err());

    let pkg = result.unwrap();

    // Verify nested type reference
    assert!(pkg.message_rs.contains("builtin_interfaces::msg::Time"));

    // Verify dependency in Cargo.toml
    assert!(pkg.cargo_toml.contains("builtin_interfaces"));
}

#[test]
fn test_generate_geometry_msgs_point() {
    let msg_content = "float64 x\nfloat64 y\nfloat64 z";
    let msg = parse_message(msg_content).expect("Failed to parse Point");
    let deps = BTreeSet::new();

    let result = generate_nros_message_package(
        "geometry_msgs",
        "Point",
        &msg,
        &deps,
        "3.2.0",
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    );
    assert!(result.is_ok(), "Generation failed: {:?}", result.err());

    let pkg = result.unwrap();

    assert!(pkg.message_rs.contains("pub x: f64"));
    assert!(pkg.message_rs.contains("pub y: f64"));
    assert!(pkg.message_rs.contains("pub z: f64"));
}

#[test]
fn test_generate_sensor_msgs_range() {
    // Simplified Range message with various field types
    let msg_content = "uint8 ULTRASOUND=0\nuint8 INFRARED=1\nstd_msgs/Header header\nuint8 radiation_type\nfloat32 field_of_view\nfloat32 min_range\nfloat32 max_range\nfloat32 range";
    let msg = parse_message(msg_content).expect("Failed to parse Range");
    let deps = BTreeSet::new();

    let result = generate_nros_message_package(
        "sensor_msgs",
        "Range",
        &msg,
        &deps,
        "4.1.0",
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    );
    assert!(result.is_ok(), "Generation failed: {:?}", result.err());

    let pkg = result.unwrap();

    // Verify constants
    assert!(pkg.message_rs.contains("pub const ULTRASOUND: u8 = 0"));
    assert!(pkg.message_rs.contains("pub const INFRARED: u8 = 1"));

    // Verify fields
    assert!(pkg.message_rs.contains("pub radiation_type: u8"));
    assert!(pkg.message_rs.contains("pub field_of_view: f32"));
}

#[test]
fn test_generate_example_interfaces_add_two_ints() {
    let srv_content = "int64 a\nint64 b\n---\nint64 sum";
    let srv = parse_service(srv_content).expect("Failed to parse AddTwoInts");
    let deps = BTreeSet::new();

    let result = generate_nros_service_package(
        "example_interfaces",
        "AddTwoInts",
        &srv,
        &deps,
        "0.10.0",
        RosEdition::Humble.type_hash(),
        RosEdition::Humble.type_hash(),
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    );
    assert!(result.is_ok(), "Generation failed: {:?}", result.err());

    let pkg = result.unwrap();

    // Verify service types
    assert!(pkg.service_rs.contains("pub struct AddTwoIntsRequest"));
    assert!(pkg.service_rs.contains("pub struct AddTwoIntsResponse"));
    assert!(pkg.service_rs.contains("pub a: i64"));
    assert!(pkg.service_rs.contains("pub b: i64"));
    assert!(pkg.service_rs.contains("pub sum: i64"));

    // Verify RosService impl
    assert!(pkg.service_rs.contains("impl RosService for AddTwoInts"));
    assert!(pkg.service_rs.contains("type Request = AddTwoIntsRequest"));
    assert!(pkg.service_rs.contains("type Reply = AddTwoIntsResponse"));
}

#[test]
fn service_request_field_honors_capacity_config() {
    // RFC-0033 / Phase 229.4: per-field config reaches service request/response
    // fields, keyed by the ROS-convention `<Service>_Request` / `_Response` name.
    let srv = parse_service("uint8[] blob\n---\nstring note\n").unwrap();
    let resolver = CapacityResolver::from_toml_str(
        r#"
        [fields]
        "big_srvs/Upload_Request.blob" = 4096
        "big_srvs/Upload_Response.note" = 16
        "#,
    )
    .unwrap();
    let pkg = generate_nros_service_package(
        "big_srvs",
        "Upload",
        &srv,
        &BTreeSet::new(),
        "0.1.0",
        RosEdition::Humble.type_hash(),
        RosEdition::Humble.type_hash(),
        RosEdition::Humble.type_hash(),
        &resolver,
    )
    .expect("generate");
    assert!(
        pkg.service_rs.contains("heapless::Vec<u8, 4096>"),
        "request field cap not applied:\n{}",
        pkg.service_rs
    );
    assert!(
        pkg.service_rs.contains("heapless::String<16>"),
        "response field cap not applied:\n{}",
        pkg.service_rs
    );
}

#[test]
fn test_generate_message_with_sequence() {
    let msg_content = "int32[] data\nfloat64[] values";
    let msg = parse_message(msg_content).expect("Failed to parse message");
    let deps = BTreeSet::new();

    let result = generate_nros_message_package(
        "test_msgs",
        "Arrays",
        &msg,
        &deps,
        "0.1.0",
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    );
    assert!(result.is_ok(), "Generation failed: {:?}", result.err());

    let pkg = result.unwrap();

    // Verify sequences use heapless::Vec
    assert!(pkg.message_rs.contains("heapless::Vec<i32, 64>"));
    assert!(pkg.message_rs.contains("heapless::Vec<f64, 64>"));

    // Verify sequence serialization/deserialization
    assert!(
        pkg.message_rs
            .contains("writer.write_u32(self.data.len() as u32)?")
    );
    assert!(
        pkg.message_rs
            .contains("let len = reader.read_u32()? as usize")
    );
}

#[test]
fn test_generate_message_with_bounded_sequence() {
    let msg_content = "int32[<=10] data";
    let msg = parse_message(msg_content).expect("Failed to parse message");
    let deps = BTreeSet::new();

    let result = generate_nros_message_package(
        "test_msgs",
        "BoundedSeq",
        &msg,
        &deps,
        "0.1.0",
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    );
    assert!(result.is_ok(), "Generation failed: {:?}", result.err());

    let pkg = result.unwrap();

    // Verify bounded sequence uses the specified max size
    assert!(pkg.message_rs.contains("heapless::Vec<i32, 10>"));
}

#[test]
fn test_generate_message_with_array() {
    let msg_content = "float64[3] position";
    let msg = parse_message(msg_content).expect("Failed to parse message");
    let deps = BTreeSet::new();

    let result = generate_nros_message_package(
        "test_msgs",
        "Position",
        &msg,
        &deps,
        "0.1.0",
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    );
    assert!(result.is_ok(), "Generation failed: {:?}", result.err());

    let pkg = result.unwrap();

    // Verify fixed-size array
    assert!(pkg.message_rs.contains("[f64; 3]"));
}

// ---------------------------------------------------------------------------
// RFC-0033 / Phase 229 — per-field capacity configuration
// ---------------------------------------------------------------------------

fn gen_frame(resolver: &CapacityResolver) -> Result<String, String> {
    // One message with a big unbounded sequence + a small unbounded string.
    let msg = parse_message("uint8[] pixels\nstring label\n").expect("parse");
    generate_nros_message_package(
        "my_msgs",
        "Frame",
        &msg,
        &BTreeSet::new(),
        "0.1.0",
        RosEdition::Humble.type_hash(),
        resolver,
    )
    .map(|p| p.message_rs)
    .map_err(|e| e.to_string())
}

#[test]
fn per_field_config_sets_big_seq_and_small_string_in_one_message() {
    let resolver = CapacityResolver::from_toml_str(
        r#"
        [fields]
        "my_msgs/Frame.pixels" = 921600
        "my_msgs/Frame.label"  = 16
        "#,
    )
    .unwrap();
    let rs = gen_frame(&resolver).expect("owned config should generate");
    assert!(
        rs.contains("heapless::Vec<u8, 921600>"),
        "big sequence capacity not applied:\n{rs}"
    );
    assert!(
        rs.contains("heapless::String<16>"),
        "small string capacity not applied:\n{rs}"
    );
}

#[test]
fn empty_config_keeps_builtin_defaults() {
    let rs = gen_frame(&CapacityResolver::empty()).expect("generate");
    assert!(rs.contains("heapless::Vec<u8, 64>"), "{rs}");
    assert!(rs.contains("heapless::String<256>"), "{rs}");
}

#[test]
fn type_level_default_applies_to_unbounded_fields() {
    let resolver = CapacityResolver::from_toml_str(
        r#"
        [types."my_msgs/Frame"]
        sequence = 4096
        "#,
    )
    .unwrap();
    let rs = gen_frame(&resolver).expect("generate");
    assert!(rs.contains("heapless::Vec<u8, 4096>"), "{rs}");
    // string untouched → builtin default
    assert!(rs.contains("heapless::String<256>"), "{rs}");
}

#[test]
fn bounded_field_ignores_config() {
    // `.msg` bound is authoritative; a conflicting [fields] entry must not win.
    let msg = parse_message("uint8[<=8] payload\n").expect("parse");
    let resolver = CapacityResolver::from_toml_str(
        r#"
        [fields]
        "my_msgs/Bounded.payload" = 999
        "#,
    )
    .unwrap();
    let pkg = generate_nros_message_package(
        "my_msgs",
        "Bounded",
        &msg,
        &BTreeSet::new(),
        "0.1.0",
        RosEdition::Humble.type_hash(),
        &resolver,
    )
    .expect("generate");
    assert!(
        pkg.message_rs.contains("heapless::Vec<u8, 8>"),
        "bound must win over config:\n{}",
        pkg.message_rs
    );
    assert!(!pkg.message_rs.contains("999"));
}

#[test]
fn borrowed_mode_emits_zero_copy_view() {
    // Phase 229.6 (issue 0007): a byte-sequence field marked `view` emits a
    // `{Msg}View<'a>` zero-copy view + a `{Msg}Borrow` marker, alongside the
    // unchanged owned `{Msg}` (which keeps a default-capacity owned container
    // for the publish path).
    let resolver = CapacityResolver::from_toml_str(
        r#"
        [fields]
        "my_msgs/Frame.pixels" = { cap = 1000, mode = "view" }
        "#,
    )
    .unwrap();
    let rs = gen_frame(&resolver).expect("borrowed byte-sequence now generates");
    assert!(
        rs.contains("pub struct FrameView<'a>"),
        "view missing:\n{rs}"
    );
    assert!(
        rs.contains("pub pixels: &'a [u8]"),
        "borrowed slice missing:\n{rs}"
    );
    assert!(
        rs.contains("impl<'a> nros_core::DeserializeView<'a> for FrameView<'a>"),
        "DeserializeView missing:\n{rs}"
    );
    assert!(
        rs.contains("impl nros_core::ViewableMessage for FrameViewable"),
        "marker impl missing:\n{rs}"
    );
    // Owned struct is preserved for publishers.
    assert!(
        rs.contains("pub struct Frame {"),
        "owned struct missing:\n{rs}"
    );
}

#[test]
fn heap_mode_emits_alloc_containers() {
    // RFC-0033 mode = "heap": growable alloc-backed Vec/String, no fixed capacity.
    let resolver = CapacityResolver::from_toml_str(
        r#"
        [fields]
        "my_msgs/Frame.pixels" = { cap = 0, mode = "heap" }
        "my_msgs/Frame.label"  = { cap = 0, mode = "heap" }
        "#,
    )
    .unwrap();
    let rs = gen_frame(&resolver).expect("heap config should generate");
    // Field types use the alloc-backed re-export.
    assert!(
        rs.contains("nros_core::heap::Vec<u8>"),
        "heap Vec type missing:\n{rs}"
    );
    assert!(
        rs.contains("nros_core::heap::String"),
        "heap String type missing:\n{rs}"
    );
    // Deserialize is growable: no fixed-capacity Vec + no CapacityExceeded on push.
    assert!(rs.contains("nros_core::heap::Vec::new()"), "{rs}");
    assert!(rs.contains("nros_core::heap::String::from(s)"), "{rs}");
    assert!(!rs.contains("heapless::Vec<u8"));
}

/// The body of `impl <ty> { … }` in `src`, the block that holds the type's
/// associated constants.
fn impl_block<'a>(src: &'a str, ty: &str) -> &'a str {
    let head = format!("\nimpl {ty} {{\n");
    let start = src
        .find(&head)
        .unwrap_or_else(|| panic!("no `impl {ty}` block in:\n{src}"))
        + head.len();
    &src[start..start + src[start..].find("\n}").unwrap()]
}

/// `.msg` constants are associated consts of the message type, so a node
/// names them `IslandMode::MODE_NOMINAL` as in rclrs. They used to be emitted
/// at the top of the message's private module, which no user can reach.
#[test]
fn msg_constants_are_associated_consts_of_the_type() {
    let msg = parse_message(
        "uint8 MODE_NOMINAL=0\nuint8 MODE_DEGRADED=1\nstring NAME=\"island\"\nuint8 mode\n",
    )
    .unwrap();
    let pkg = generate_nros_message_package(
        "island_msgs",
        "IslandMode",
        &msg,
        &BTreeSet::new(),
        "0.0.0",
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    )
    .unwrap();
    let block = impl_block(&pkg.message_rs, "IslandMode");
    assert!(block.contains("pub const MODE_NOMINAL: u8 = 0;"), "{block}");
    assert!(
        block.contains("pub const MODE_DEGRADED: u8 = 1;"),
        "{block}"
    );
    assert!(
        block.contains("pub const NAME: &'static str = \"island\";"),
        "{block}"
    );
    assert!(
        !pkg.message_rs.contains("\npub const MODE_NOMINAL"),
        "a module-level copy is unreachable and must not be emitted"
    );
}

/// The same for a service: each half's constants belong to that half.
#[test]
fn srv_constants_belong_to_their_half() {
    let srv = parse_service("int32 LIMIT=10\nint32 a\n---\nint32 OK=0\nint32 code\n").unwrap();
    let pkg = generate_nros_service_package(
        "island_srvs",
        "Check",
        &srv,
        &BTreeSet::new(),
        "0.0.0",
        RosEdition::Humble.type_hash(),
        RosEdition::Humble.type_hash(),
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    )
    .unwrap();
    assert!(impl_block(&pkg.service_rs, "CheckRequest").contains("pub const LIMIT: i32 = 10;"));
    assert!(impl_block(&pkg.service_rs, "CheckResponse").contains("pub const OK: i32 = 0;"));
}

/// And for an action, under the constant's own name rather than the old
/// `<ACTION>_GOAL_<NAME>` module-level spelling.
#[test]
fn action_constants_belong_to_their_part() {
    let action = rosidl_parser::parse_action(
        "int32 MAX_TARGET=100\nint32 target\n---\nbool DONE=true\nbool ok\n---\nint32 STEP=5\nint32 progress\n",
    )
    .unwrap();
    let pkg = rosidl_codegen::generate_nros_action_package(
        "island_actions",
        "Reach",
        &action,
        &BTreeSet::new(),
        "0.0.0",
        &rosidl_codegen::rihs::ActionTypeHashes::uniform(RosEdition::Humble.type_hash()),
        &CapacityResolver::empty(),
    )
    .unwrap();
    assert!(impl_block(&pkg.action_rs, "ReachGoal").contains("pub const MAX_TARGET: i32 = 100;"));
    assert!(impl_block(&pkg.action_rs, "ReachResult").contains("pub const DONE: bool = true;"));
    assert!(impl_block(&pkg.action_rs, "ReachFeedback").contains("pub const STEP: i32 = 5;"));
    assert!(!pkg.action_rs.contains("REACH_GOAL_MAX_TARGET"));
}
