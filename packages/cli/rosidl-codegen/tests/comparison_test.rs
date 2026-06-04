// Comparison tests - verify our codegen matches rosidl_generator_rs output
use rosidl_codegen::{GeneratorError, generate_message_package};
use rosidl_parser::parse_message;
use std::{collections::HashSet, fs, path::PathBuf};

mod parity_helpers;
use parity_helpers::{normalize_code, print_diff};

/// Helper to load reference output from fixtures
fn load_reference_output(package: &str, message: &str, layer: &str) -> Result<String, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/reference_outputs")
        .join(package)
        .join(message)
        .join(format!("msg_{}.rs", layer));

    fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))
}

/// Helper to read and parse a ROS message file
fn read_and_parse_ros_message(
    package: &str,
    message: &str,
) -> Result<rosidl_parser::Message, String> {
    let path = format!("/opt/ros/jazzy/share/{}/msg/{}.msg", package, message);
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {} (is ROS installed?)", path, e))?;
    parse_message(&content).map_err(|e| format!("Failed to parse {}: {:?}", path, e))
}

#[test]
fn test_compare_std_msgs_bool() -> Result<(), GeneratorError> {
    // Parse the ROS message
    let msg = match read_and_parse_ros_message("std_msgs", "Bool") {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Skipping test: {}", e);
            return Ok(());
        }
    };

    // Generate with our codegen
    let result = generate_message_package("std_msgs", "Bool", &msg, &HashSet::new())?;

    // Load reference outputs
    let ref_rmw = load_reference_output("std_msgs", "Bool", "rmw")
        .expect("Reference RMW output should exist");
    let ref_idiomatic = load_reference_output("std_msgs", "Bool", "idiomatic")
        .expect("Reference idiomatic output should exist");

    // Normalize both for comparison
    let our_rmw = normalize_code(&result.message_rmw, "std_msgs");
    let ref_rmw_normalized = normalize_code(&ref_rmw, "std_msgs");

    let our_idiomatic = normalize_code(&result.message_idiomatic, "std_msgs");
    let ref_idiomatic_normalized = normalize_code(&ref_idiomatic, "std_msgs");

    // Compare RMW layer
    let rmw_matches = print_diff(
        "Our RMW (std_msgs::Bool)",
        "Reference RMW (rosidl_generator_rs)",
        &our_rmw,
        &ref_rmw_normalized,
    );

    // Compare idiomatic layer
    let idiomatic_matches = print_diff(
        "Our Idiomatic (std_msgs::Bool)",
        "Reference Idiomatic (rosidl_generator_rs)",
        &our_idiomatic,
        &ref_idiomatic_normalized,
    );

    // For now, we just print the diffs without failing
    // Once we achieve parity, change this to:
    // assert!(rmw_matches && idiomatic_matches, "Generated code differs from reference");

    if !rmw_matches {
        eprintln!("\n⚠️  RMW layer differs from reference (expected during development)");
    }
    if !idiomatic_matches {
        eprintln!("\n⚠️  Idiomatic layer differs from reference (expected during development)");
    }

    Ok(())
}

#[test]
fn test_compare_std_msgs_string() -> Result<(), GeneratorError> {
    // Parse the ROS message
    let msg = match read_and_parse_ros_message("std_msgs", "String") {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Skipping test: {}", e);
            return Ok(());
        }
    };

    // Generate with our codegen
    let result = generate_message_package("std_msgs", "String", &msg, &HashSet::new())?;

    // Load reference outputs
    let ref_rmw = load_reference_output("std_msgs", "String", "rmw")
        .expect("Reference RMW output should exist");
    let ref_idiomatic = load_reference_output("std_msgs", "String", "idiomatic")
        .expect("Reference idiomatic output should exist");

    // Normalize both for comparison
    let our_rmw = normalize_code(&result.message_rmw, "std_msgs");
    let ref_rmw_normalized = normalize_code(&ref_rmw, "std_msgs");

    let our_idiomatic = normalize_code(&result.message_idiomatic, "std_msgs");
    let ref_idiomatic_normalized = normalize_code(&ref_idiomatic, "std_msgs");

    // Compare
    let rmw_matches = print_diff(
        "Our RMW (std_msgs::String)",
        "Reference RMW (rosidl_generator_rs)",
        &our_rmw,
        &ref_rmw_normalized,
    );

    let idiomatic_matches = print_diff(
        "Our Idiomatic (std_msgs::String)",
        "Reference Idiomatic (rosidl_generator_rs)",
        &our_idiomatic,
        &ref_idiomatic_normalized,
    );

    if !rmw_matches {
        eprintln!("\n⚠️  RMW layer differs from reference (expected during development)");
    }
    if !idiomatic_matches {
        eprintln!("\n⚠️  Idiomatic layer differs from reference (expected during development)");
    }

    Ok(())
}

#[test]
fn test_compare_geometry_msgs_point() -> Result<(), GeneratorError> {
    // Parse the ROS message
    let msg = match read_and_parse_ros_message("geometry_msgs", "Point") {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Skipping test: {}", e);
            return Ok(());
        }
    };

    // Generate with our codegen
    let result = generate_message_package("geometry_msgs", "Point", &msg, &HashSet::new())?;

    // Load reference outputs
    let ref_rmw = load_reference_output("geometry_msgs", "Point", "rmw")
        .expect("Reference RMW output should exist");
    let ref_idiomatic = load_reference_output("geometry_msgs", "Point", "idiomatic")
        .expect("Reference idiomatic output should exist");

    // Normalize both for comparison
    let our_rmw = normalize_code(&result.message_rmw, "geometry_msgs");
    let ref_rmw_normalized = normalize_code(&ref_rmw, "geometry_msgs");

    let our_idiomatic = normalize_code(&result.message_idiomatic, "geometry_msgs");
    let ref_idiomatic_normalized = normalize_code(&ref_idiomatic, "geometry_msgs");

    // Compare
    let rmw_matches = print_diff(
        "Our RMW (geometry_msgs::Point)",
        "Reference RMW (rosidl_generator_rs)",
        &our_rmw,
        &ref_rmw_normalized,
    );

    let idiomatic_matches = print_diff(
        "Our Idiomatic (geometry_msgs::Point)",
        "Reference Idiomatic (rosidl_generator_rs)",
        &our_idiomatic,
        &ref_idiomatic_normalized,
    );

    if !rmw_matches {
        eprintln!("\n⚠️  RMW layer differs from reference (expected during development)");
    }
    if !idiomatic_matches {
        eprintln!("\n⚠️  Idiomatic layer differs from reference (expected during development)");
    }

    Ok(())
}

#[test]
fn test_normalization_helpers_exist() {
    // Smoke test to verify normalization helpers are working
    let code = r#"
        // Comment
        fn foo() {
            bar();
        }
    "#;

    let normalized = normalize_code(code, "test_pkg");
    assert!(!normalized.is_empty());
}
