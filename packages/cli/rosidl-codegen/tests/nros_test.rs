//! Integration tests for nros code generation

use rosidl_codegen::{RosEdition, generate_nros_message_package, generate_nros_service_package};
use rosidl_parser::{parse_message, parse_service};
use std::collections::HashSet;

#[test]
fn test_generate_std_msgs_int32() {
    let msg_content = "int32 data";
    let msg = parse_message(msg_content).expect("Failed to parse Int32");
    let deps = HashSet::new();

    let result = generate_nros_message_package(
        "std_msgs",
        "Int32",
        &msg,
        &deps,
        "5.3.0",
        RosEdition::Humble,
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
    let deps = HashSet::new();

    let result = generate_nros_message_package(
        "std_msgs",
        "String",
        &msg,
        &deps,
        "5.3.0",
        RosEdition::Humble,
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
    let deps = HashSet::new();

    let result = generate_nros_message_package(
        "std_msgs",
        "Header",
        &msg,
        &deps,
        "5.3.0",
        RosEdition::Humble,
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
    let deps = HashSet::new();

    let result = generate_nros_message_package(
        "geometry_msgs",
        "Point",
        &msg,
        &deps,
        "3.2.0",
        RosEdition::Humble,
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
    let deps = HashSet::new();

    let result = generate_nros_message_package(
        "sensor_msgs",
        "Range",
        &msg,
        &deps,
        "4.1.0",
        RosEdition::Humble,
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
    let deps = HashSet::new();

    let result = generate_nros_service_package(
        "example_interfaces",
        "AddTwoInts",
        &srv,
        &deps,
        "0.10.0",
        RosEdition::Humble,
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
fn test_generate_message_with_sequence() {
    let msg_content = "int32[] data\nfloat64[] values";
    let msg = parse_message(msg_content).expect("Failed to parse message");
    let deps = HashSet::new();

    let result = generate_nros_message_package(
        "test_msgs",
        "Arrays",
        &msg,
        &deps,
        "0.1.0",
        RosEdition::Humble,
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
    let deps = HashSet::new();

    let result = generate_nros_message_package(
        "test_msgs",
        "BoundedSeq",
        &msg,
        &deps,
        "0.1.0",
        RosEdition::Humble,
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
    let deps = HashSet::new();

    let result = generate_nros_message_package(
        "test_msgs",
        "Position",
        &msg,
        &deps,
        "0.1.0",
        RosEdition::Humble,
    );
    assert!(result.is_ok(), "Generation failed: {:?}", result.err());

    let pkg = result.unwrap();

    // Verify fixed-size array
    assert!(pkg.message_rs.contains("[f64; 3]"));
}
