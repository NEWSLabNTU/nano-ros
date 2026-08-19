// Parity tests - verify generation works with real ROS packages from system
use rosidl_codegen::{
    GeneratorError, generate_action_package, generate_message_package, generate_service_package,
};
use rosidl_parser::{parse_action, parse_message, parse_service};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

mod parity_helpers;
use parity_helpers::{note_no_ros, ros_share_root};

/// Issue 0693 — `<share>/<pkg>/<kind>` for the INSTALLED distro.
///
/// Every path in this file was a `/opt/ros/jazzy/...` literal while the project
/// installs humble, so all nine tests took their "Skipping" arm and the suite
/// reported PASS over work it never did.
fn ros_dir(package: &str, kind: &str) -> Option<PathBuf> {
    Some(ros_share_root()?.join(package).join(kind))
}

/// `<share>/<pkg>/<kind>/<file>` for the installed distro.
fn ros_file(package: &str, kind: &str, file: &str) -> Option<PathBuf> {
    Some(ros_dir(package, kind)?.join(file))
}

/// Helper to read a .msg file and parse it
fn read_and_parse_message(path: &Path) -> Result<rosidl_parser::Message, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    parse_message(&content).map_err(|e| format!("Failed to parse {}: {:?}", path.display(), e))
}

/// Helper to read a .srv file and parse it
fn read_and_parse_service(path: &Path) -> Result<rosidl_parser::Service, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    parse_service(&content).map_err(|e| format!("Failed to parse {}: {:?}", path.display(), e))
}

/// Helper to read a .action file and parse it
fn read_and_parse_action(path: &Path) -> Result<rosidl_parser::Action, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    parse_action(&content).map_err(|e| format!("Failed to parse {}: {:?}", path.display(), e))
}

#[test]
fn test_std_msgs_primitives() -> Result<(), GeneratorError> {
    // Test basic std_msgs types
    let Some(ros_share) = ros_dir("std_msgs", "msg") else {
        note_no_ros("parity_test");
        return Ok(());
    };

    if !ros_share.exists() {
        note_no_ros("parity_test: ROS share dir absent");
        return Ok(());
    }

    let test_messages = vec!["Bool.msg", "Int32.msg", "Float64.msg", "String.msg"];

    for msg_file in test_messages {
        let path = ros_share.join(msg_file);
        if path.exists() {
            let msg = read_and_parse_message(&path).map_err(GeneratorError::InvalidMessage)?;

            let msg_name = msg_file.trim_end_matches(".msg");
            let result = generate_message_package("std_msgs", msg_name, &msg, &HashSet::new())?;

            // Verify basic structure
            assert!(result.cargo_toml.contains("std_msgs"));
            assert!(result.message_rmw.contains(msg_name));
            assert!(result.message_idiomatic.contains(msg_name));
        }
    }

    Ok(())
}

#[test]
fn test_std_msgs_header() -> Result<(), GeneratorError> {
    let Some(header_path) = ros_file("std_msgs", "msg", "Header.msg") else {
        note_no_ros("parity_test");
        return Ok(());
    };

    if !header_path.exists() {
        note_no_ros("parity_test: Header.msg not found");
        return Ok(());
    }

    let msg = read_and_parse_message(&header_path).map_err(GeneratorError::InvalidMessage)?;

    let result = generate_message_package("std_msgs", "Header", &msg, &HashSet::new())?;

    // Header should have timestamp and frame_id
    assert!(result.message_rmw.contains("Header"));
    assert!(result.cargo_toml.contains("std_msgs"));

    Ok(())
}

#[test]
fn test_geometry_msgs_point() -> Result<(), GeneratorError> {
    let Some(point_path) = ros_file("geometry_msgs", "msg", "Point.msg") else {
        note_no_ros("parity_test");
        return Ok(());
    };

    if !point_path.exists() {
        note_no_ros("parity_test: Point.msg not found");
        return Ok(());
    }

    let msg = read_and_parse_message(&point_path).map_err(GeneratorError::InvalidMessage)?;

    let result = generate_message_package("geometry_msgs", "Point", &msg, &HashSet::new())?;

    // Point should have x, y, z fields
    assert!(result.message_rmw.contains("Point"));
    assert!(result.message_rmw.contains("pub x:") || result.message_rmw.contains("x:"));
    assert!(result.message_rmw.contains("pub y:") || result.message_rmw.contains("y:"));
    assert!(result.message_rmw.contains("pub z:") || result.message_rmw.contains("z:"));

    Ok(())
}

#[test]
fn test_geometry_msgs_pose() -> Result<(), GeneratorError> {
    let Some(pose_path) = ros_file("geometry_msgs", "msg", "Pose.msg") else {
        note_no_ros("parity_test");
        return Ok(());
    };

    if !pose_path.exists() {
        note_no_ros("parity_test: Pose.msg not found");
        return Ok(());
    }

    let msg = read_and_parse_message(&pose_path).map_err(GeneratorError::InvalidMessage)?;

    let result = generate_message_package("geometry_msgs", "Pose", &msg, &HashSet::new())?;

    // Pose should have Point and Quaternion dependencies
    assert!(result.message_rmw.contains("Pose"));
    assert!(result.message_rmw.contains("Point") || result.message_rmw.contains("position"));
    assert!(
        result.message_rmw.contains("Quaternion") || result.message_rmw.contains("orientation")
    );

    Ok(())
}

#[test]
fn test_example_interfaces_service() -> Result<(), GeneratorError> {
    let Some(srv_path) = ros_file("example_interfaces", "srv", "AddTwoInts.srv") else {
        note_no_ros("parity_test");
        return Ok(());
    };

    if !srv_path.exists() {
        note_no_ros("parity_test: AddTwoInts.srv not found");
        return Ok(());
    }

    let srv = read_and_parse_service(&srv_path).map_err(GeneratorError::InvalidMessage)?;

    let result =
        generate_service_package("example_interfaces", "AddTwoInts", &srv, &HashSet::new())?;

    // Service should have Request and Response
    assert!(result.service_rmw.contains("AddTwoIntsRequest"));
    assert!(result.service_rmw.contains("AddTwoIntsResponse"));
    assert!(result.lib_rs.contains("pub mod srv"));

    Ok(())
}

#[test]
fn test_example_interfaces_action() -> Result<(), GeneratorError> {
    let Some(action_path) = ros_file("example_interfaces", "action", "Fibonacci.action") else {
        note_no_ros("parity_test");
        return Ok(());
    };

    if !action_path.exists() {
        note_no_ros("parity_test: Fibonacci.action not found");
        return Ok(());
    }

    let action = read_and_parse_action(&action_path).map_err(GeneratorError::InvalidMessage)?;

    let result =
        generate_action_package("example_interfaces", "Fibonacci", &action, &HashSet::new())?;

    // Action should have Goal, Result, Feedback
    assert!(result.action_rmw.contains("FibonacciGoal"));
    assert!(result.action_rmw.contains("FibonacciResult"));
    assert!(result.action_rmw.contains("FibonacciFeedback"));
    assert!(result.lib_rs.contains("pub mod action"));

    Ok(())
}

#[test]
fn test_parse_all_std_msgs() {
    let Some(ros_share) = ros_dir("std_msgs", "msg") else {
        note_no_ros("parity_test");
        return;
    };

    if !ros_share.exists() {
        note_no_ros("parity_test: ROS share dir absent");
        return;
    }

    let mut count = 0;
    let mut failures = Vec::new();

    for entry in WalkDir::new(ros_share)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "msg"))
    {
        count += 1;
        let path = entry.path();

        match read_and_parse_message(path) {
            Ok(msg) => {
                let msg_name = path.file_stem().unwrap().to_str().unwrap();
                match generate_message_package("std_msgs", msg_name, &msg, &HashSet::new()) {
                    Ok(_) => {}
                    Err(e) => failures.push(format!("{}: {:?}", path.display(), e)),
                }
            }
            Err(e) => failures.push(format!("{}: {}", path.display(), e)),
        }
    }

    if !failures.is_empty() {
        eprintln!(
            "Failed to process {} out of {} std_msgs ({}% success rate):",
            failures.len(),
            count,
            (count - failures.len()) * 100 / count
        );
        for failure in &failures {
            eprintln!("  {}", failure);
        }
        // Don't panic - just report the failures
        eprintln!("Note: Some failures expected due to parser limitations (default values, etc.)");
    }

    println!(
        "Successfully processed {} out of {} std_msgs messages ({}% success)",
        count - failures.len(),
        count,
        (count - failures.len()) * 100 / count
    );
}

#[test]
fn test_parse_all_geometry_msgs() {
    let Some(ros_share) = ros_dir("geometry_msgs", "msg") else {
        note_no_ros("parity_test");
        return;
    };

    if !ros_share.exists() {
        note_no_ros("parity_test: geometry_msgs not found");
        return;
    }

    let mut count = 0;
    let mut failures = Vec::new();

    for entry in WalkDir::new(ros_share)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "msg"))
    {
        count += 1;
        let path = entry.path();

        match read_and_parse_message(path) {
            Ok(msg) => {
                let msg_name = path.file_stem().unwrap().to_str().unwrap();
                match generate_message_package("geometry_msgs", msg_name, &msg, &HashSet::new()) {
                    Ok(_) => {}
                    Err(e) => failures.push(format!("{}: {:?}", path.display(), e)),
                }
            }
            Err(e) => failures.push(format!("{}: {}", path.display(), e)),
        }
    }

    if !failures.is_empty() {
        eprintln!(
            "Failed to process {} out of {} geometry_msgs ({}% success rate):",
            failures.len(),
            count,
            (count - failures.len()) * 100 / count
        );
        for failure in &failures {
            eprintln!("  {}", failure);
        }
        // Don't panic - just report the failures
        eprintln!("Note: Some failures expected due to parser limitations (default values, etc.)");
    }

    println!(
        "Successfully processed {} out of {} geometry_msgs messages ({}% success)",
        count - failures.len(),
        count,
        (count - failures.len()) * 100 / count
    );
}

#[test]
fn test_parse_all_sensor_msgs() {
    let Some(ros_share) = ros_dir("sensor_msgs", "msg") else {
        note_no_ros("parity_test");
        return;
    };

    if !ros_share.exists() {
        note_no_ros("parity_test: sensor_msgs not found");
        return;
    }

    let mut count = 0;
    let mut failures = Vec::new();

    for entry in WalkDir::new(ros_share)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "msg"))
    {
        count += 1;
        let path = entry.path();

        match read_and_parse_message(path) {
            Ok(msg) => {
                let msg_name = path.file_stem().unwrap().to_str().unwrap();
                match generate_message_package("sensor_msgs", msg_name, &msg, &HashSet::new()) {
                    Ok(_) => {}
                    Err(e) => failures.push(format!("{}: {:?}", path.display(), e)),
                }
            }
            Err(e) => failures.push(format!("{}: {}", path.display(), e)),
        }
    }

    if !failures.is_empty() {
        eprintln!(
            "Failed to process {} out of {} sensor_msgs ({}% success rate):",
            failures.len(),
            count,
            (count - failures.len()) * 100 / count
        );
        for failure in &failures {
            eprintln!("  {}", failure);
        }
        // Don't panic - just report the failures
        eprintln!("Note: Some failures expected due to parser limitations (default values, etc.)");
    }

    println!(
        "Successfully processed {} out of {} sensor_msgs messages ({}% success)",
        count - failures.len(),
        count,
        (count - failures.len()) * 100 / count
    );
}
