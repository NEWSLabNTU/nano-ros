use rosidl_codegen::{
    GeneratorError, generate_action_package, generate_message_package, generate_service_package,
};
use rosidl_parser::{parse_action, parse_message, parse_service};
use std::{collections::HashSet, fs};
use tempfile::TempDir;

#[test]
fn test_generate_simple_message() -> Result<(), GeneratorError> {
    let msg_def = "int32 x\nfloat64 y\nstring name\n";
    let msg = parse_message(msg_def).unwrap();

    let deps = HashSet::new();
    let result = generate_message_package("test_msgs", "Point2D", &msg, &deps)?;

    // Verify Cargo.toml contains package name
    assert!(result.cargo_toml.contains("test_msgs"));
    assert!(result.cargo_toml.contains("rosidl-runtime-rs"));

    // Verify build.rs is generated
    assert!(result.build_rs.contains("cargo:rerun-if-changed"));

    // Verify lib.rs contains message module
    assert!(result.lib_rs.contains("pub mod msg"));

    // Verify RMW layer contains struct and fields
    assert!(result.message_rmw.contains("struct Point2D"));
    assert!(result.message_rmw.contains("pub x: i32"));
    assert!(result.message_rmw.contains("pub y: f64"));
    assert!(result.message_rmw.contains("rosidl_runtime_rs::String"));

    // Verify idiomatic layer
    assert!(result.message_idiomatic.contains("struct Point2D"));
    assert!(result.message_idiomatic.contains("std::string::String"));

    Ok(())
}

#[test]
fn test_generate_message_with_constants() -> Result<(), GeneratorError> {
    let msg_def = "int32 x\nint32 MAX_VALUE=100\nstring DEFAULT_NAME=\"test\"\n";
    let msg = parse_message(msg_def).unwrap();

    let deps = HashSet::new();
    let result = generate_message_package("test_msgs", "ConfigMsg", &msg, &deps)?;

    // Verify constants are generated
    assert!(result.message_rmw.contains("MAX_VALUE"));
    assert!(result.message_idiomatic.contains("MAX_VALUE"));

    Ok(())
}

#[test]
fn test_generate_message_with_arrays() -> Result<(), GeneratorError> {
    let msg_def = "int32[5] small_array\nfloat64[100] large_array\n";
    let msg = parse_message(msg_def).unwrap();

    let deps = HashSet::new();
    let result = generate_message_package("test_msgs", "ArrayMsg", &msg, &deps)?;

    // Verify arrays are generated correctly
    assert!(result.message_rmw.contains("[i32; 5]"));
    assert!(result.message_rmw.contains("[f64; 100]"));

    // Verify big-array feature is added for arrays > 32
    assert!(result.cargo_toml.contains("serde-big-array"));

    Ok(())
}

#[test]
fn test_generate_message_with_sequences() -> Result<(), GeneratorError> {
    let msg_def = "int32[] unbounded_seq\nfloat64[<=10] bounded_seq\n";
    let msg = parse_message(msg_def).unwrap();

    let deps = HashSet::new();
    let result = generate_message_package("test_msgs", "SeqMsg", &msg, &deps)?;

    // Verify RMW layer uses rosidl types
    assert!(result.message_rmw.contains("rosidl_runtime_rs::Sequence"));
    assert!(
        result
            .message_rmw
            .contains("rosidl_runtime_rs::BoundedSequence")
    );

    // Verify idiomatic layer uses Vec
    assert!(result.message_idiomatic.contains("std::vec::Vec"));

    Ok(())
}

#[test]
fn test_generate_message_with_dependencies() -> Result<(), GeneratorError> {
    let msg_def = "geometry_msgs/Point position\nstd_msgs/Header header\n";
    let msg = parse_message(msg_def).unwrap();

    let deps = HashSet::new();
    let result = generate_message_package("nav_msgs", "Pose", &msg, &deps)?;

    // Verify dependencies are added to Cargo.toml
    assert!(result.cargo_toml.contains("geometry_msgs"));
    assert!(result.cargo_toml.contains("std_msgs"));

    // Verify namespaced types in RMW layer (current hierarchy: pkg::msg::rmw::Type)
    assert!(
        result
            .message_rmw
            .contains("geometry_msgs::msg::rmw::Point")
    );
    assert!(result.message_rmw.contains("std_msgs::msg::rmw::Header"));

    // Verify namespaced types in idiomatic layer
    assert!(
        result
            .message_idiomatic
            .contains("geometry_msgs::msg::Point")
    );
    assert!(result.message_idiomatic.contains("std_msgs::msg::Header"));

    Ok(())
}

#[test]
fn test_generate_message_with_rust_keywords() -> Result<(), GeneratorError> {
    let msg_def = "int32 type\nfloat64 match\nstring async\n";
    let msg = parse_message(msg_def).unwrap();

    let deps = HashSet::new();
    let result = generate_message_package("test_msgs", "Keywords", &msg, &deps)?;

    // Verify keywords are escaped with underscore
    assert!(result.message_rmw.contains("pub type_:"));
    assert!(result.message_rmw.contains("pub match_:"));
    assert!(result.message_rmw.contains("pub async_:"));

    Ok(())
}

#[test]
fn test_write_generated_package_to_disk() -> Result<(), GeneratorError> {
    let msg_def = "int32 x\nfloat64 y\n";
    let msg = parse_message(msg_def).unwrap();

    let deps = HashSet::new();
    let result = generate_message_package("test_msgs", "Point", &msg, &deps)?;

    // Create temporary directory
    let temp_dir = TempDir::new().unwrap();
    let pkg_dir = temp_dir.path().join("test_msgs");
    fs::create_dir_all(&pkg_dir).unwrap();

    // Write files
    fs::write(pkg_dir.join("Cargo.toml"), &result.cargo_toml).unwrap();
    fs::write(pkg_dir.join("build.rs"), &result.build_rs).unwrap();

    let src_dir = pkg_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("lib.rs"), &result.lib_rs).unwrap();

    let msg_dir = src_dir.join("msg");
    fs::create_dir_all(&msg_dir).unwrap();

    let rmw_dir = msg_dir.join("rmw");
    fs::create_dir_all(&rmw_dir).unwrap();
    fs::write(rmw_dir.join("mod.rs"), &result.message_rmw).unwrap();

    // Verify files exist
    assert!(pkg_dir.join("Cargo.toml").exists());
    assert!(pkg_dir.join("build.rs").exists());
    assert!(src_dir.join("lib.rs").exists());
    assert!(rmw_dir.join("mod.rs").exists());

    Ok(())
}

// Service integration tests

#[test]
fn test_generate_simple_service() -> Result<(), GeneratorError> {
    let srv_def = "int32 a\nint32 b\n---\nint32 sum\n";
    let srv = parse_service(srv_def).unwrap();

    let deps = HashSet::new();
    let result = generate_service_package("example_interfaces", "AddTwoInts", &srv, &deps)?;

    // Verify Cargo.toml contains package name
    assert!(result.cargo_toml.contains("example_interfaces"));
    assert!(result.cargo_toml.contains("rosidl-runtime-rs"));

    // Verify lib.rs contains service module
    assert!(result.lib_rs.contains("pub mod srv"));

    // Verify RMW layer contains request and response structs
    assert!(result.service_rmw.contains("AddTwoIntsRequest"));
    assert!(result.service_rmw.contains("AddTwoIntsResponse"));
    assert!(result.service_rmw.contains("pub a: i32"));
    assert!(result.service_rmw.contains("pub b: i32"));
    assert!(result.service_rmw.contains("pub sum: i32"));

    // Verify idiomatic layer
    assert!(result.service_idiomatic.contains("AddTwoIntsRequest"));
    assert!(result.service_idiomatic.contains("AddTwoIntsResponse"));

    Ok(())
}

#[test]
fn test_generate_service_with_dependencies() -> Result<(), GeneratorError> {
    let srv_def = "geometry_msgs/Point position\n---\nbool success\nstring message\n";
    let srv = parse_service(srv_def).unwrap();

    let deps = HashSet::new();
    let result = generate_service_package("test_srvs", "CheckPosition", &srv, &deps)?;

    // Verify dependencies are added to Cargo.toml
    assert!(result.cargo_toml.contains("geometry_msgs"));

    // Verify namespaced types in RMW layer (current hierarchy: pkg::msg::rmw::Type)
    assert!(
        result
            .service_rmw
            .contains("geometry_msgs::msg::rmw::Point")
    );

    // Verify namespaced types in idiomatic layer
    assert!(
        result
            .service_idiomatic
            .contains("geometry_msgs::msg::Point")
    );

    Ok(())
}

#[test]
fn test_generate_service_with_constants() -> Result<(), GeneratorError> {
    let srv_def = "int32 input\nint32 MAX_INPUT=100\n---\nint32 output\nint32 MIN_OUTPUT=0\n";
    let srv = parse_service(srv_def).unwrap();

    let deps = HashSet::new();
    let result = generate_service_package("test_srvs", "Process", &srv, &deps)?;

    // Verify constants are generated in both request and response
    assert!(result.service_rmw.contains("MAX_INPUT"));
    assert!(result.service_rmw.contains("MIN_OUTPUT"));
    assert!(result.service_idiomatic.contains("MAX_INPUT"));
    assert!(result.service_idiomatic.contains("MIN_OUTPUT"));

    Ok(())
}

// Action integration tests

#[test]
fn test_generate_simple_action() -> Result<(), GeneratorError> {
    let action_def = "int32 order\n---\nint32[] sequence\n---\nint32[] partial_sequence\n";
    let action = parse_action(action_def).unwrap();

    let deps = HashSet::new();
    let result = generate_action_package("example_interfaces", "Fibonacci", &action, &deps)?;

    // Verify Cargo.toml contains package name
    assert!(result.cargo_toml.contains("example_interfaces"));
    assert!(result.cargo_toml.contains("rosidl-runtime-rs"));

    // Verify lib.rs contains action module
    assert!(result.lib_rs.contains("pub mod action"));

    // Verify RMW layer contains goal, result, and feedback structs
    assert!(result.action_rmw.contains("FibonacciGoal"));
    assert!(result.action_rmw.contains("FibonacciResult"));
    assert!(result.action_rmw.contains("FibonacciFeedback"));
    assert!(result.action_rmw.contains("pub order: i32"));
    assert!(
        result
            .action_rmw
            .contains("rosidl_runtime_rs::Sequence<i32>")
    );

    // Verify idiomatic layer
    assert!(result.action_idiomatic.contains("FibonacciGoal"));
    assert!(result.action_idiomatic.contains("FibonacciResult"));
    assert!(result.action_idiomatic.contains("FibonacciFeedback"));
    assert!(result.action_idiomatic.contains("std::vec::Vec<i32>"));

    Ok(())
}

#[test]
fn test_generate_action_with_dependencies() -> Result<(), GeneratorError> {
    let action_def =
        "geometry_msgs/Point target\n---\nfloat64 distance\n---\nfloat64 current_distance\n";
    let action = parse_action(action_def).unwrap();

    let deps = HashSet::new();
    let result = generate_action_package("nav_actions", "Navigate", &action, &deps)?;

    // Verify dependencies are added to Cargo.toml
    assert!(result.cargo_toml.contains("geometry_msgs"));

    // Verify namespaced types in RMW layer (current hierarchy: pkg::msg::rmw::Type)
    assert!(result.action_rmw.contains("geometry_msgs::msg::rmw::Point"));

    // Verify namespaced types in idiomatic layer
    assert!(
        result
            .action_idiomatic
            .contains("geometry_msgs::msg::Point")
    );

    Ok(())
}

#[test]
fn test_generate_action_with_constants() -> Result<(), GeneratorError> {
    let action_def = "int32 target\nint32 MAX_TARGET=100\n---\nbool success\n---\nint32 progress\nint32 MAX_PROGRESS=100\n";
    let action = parse_action(action_def).unwrap();

    let deps = HashSet::new();
    let result = generate_action_package("test_actions", "CountTo", &action, &deps)?;

    // Verify constants are generated in goal and feedback
    assert!(result.action_rmw.contains("MAX_TARGET"));
    assert!(result.action_rmw.contains("MAX_PROGRESS"));
    assert!(result.action_idiomatic.contains("MAX_TARGET"));
    assert!(result.action_idiomatic.contains("MAX_PROGRESS"));

    Ok(())
}

#[test]
fn test_write_generated_service_to_disk() -> Result<(), GeneratorError> {
    let srv_def = "int32 a\n---\nint32 result\n";
    let srv = parse_service(srv_def).unwrap();

    let deps = HashSet::new();
    let result = generate_service_package("test_srvs", "Compute", &srv, &deps)?;

    // Create temporary directory
    let temp_dir = TempDir::new().unwrap();
    let pkg_dir = temp_dir.path().join("test_srvs");
    fs::create_dir_all(&pkg_dir).unwrap();

    // Write files
    fs::write(pkg_dir.join("Cargo.toml"), &result.cargo_toml).unwrap();
    fs::write(pkg_dir.join("build.rs"), &result.build_rs).unwrap();

    let src_dir = pkg_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("lib.rs"), &result.lib_rs).unwrap();

    let srv_dir = src_dir.join("srv");
    fs::create_dir_all(&srv_dir).unwrap();

    let rmw_dir = srv_dir.join("rmw");
    fs::create_dir_all(&rmw_dir).unwrap();
    fs::write(rmw_dir.join("mod.rs"), &result.service_rmw).unwrap();

    // Verify files exist
    assert!(pkg_dir.join("Cargo.toml").exists());
    assert!(pkg_dir.join("build.rs").exists());
    assert!(src_dir.join("lib.rs").exists());
    assert!(rmw_dir.join("mod.rs").exists());

    Ok(())
}

#[test]
fn test_write_generated_action_to_disk() -> Result<(), GeneratorError> {
    let action_def = "int32 goal\n---\nint32 result\n---\nint32 feedback\n";
    let action = parse_action(action_def).unwrap();

    let deps = HashSet::new();
    let result = generate_action_package("test_actions", "Simple", &action, &deps)?;

    // Create temporary directory
    let temp_dir = TempDir::new().unwrap();
    let pkg_dir = temp_dir.path().join("test_actions");
    fs::create_dir_all(&pkg_dir).unwrap();

    // Write files
    fs::write(pkg_dir.join("Cargo.toml"), &result.cargo_toml).unwrap();
    fs::write(pkg_dir.join("build.rs"), &result.build_rs).unwrap();

    let src_dir = pkg_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("lib.rs"), &result.lib_rs).unwrap();

    let action_dir = src_dir.join("action");
    fs::create_dir_all(&action_dir).unwrap();

    let rmw_dir = action_dir.join("rmw");
    fs::create_dir_all(&rmw_dir).unwrap();
    fs::write(rmw_dir.join("mod.rs"), &result.action_rmw).unwrap();

    // Verify files exist
    assert!(pkg_dir.join("Cargo.toml").exists());
    assert!(pkg_dir.join("build.rs").exists());
    assert!(src_dir.join("lib.rs").exists());
    assert!(rmw_dir.join("mod.rs").exists());

    Ok(())
}
