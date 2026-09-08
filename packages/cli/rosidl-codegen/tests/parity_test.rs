// Parity tests - verify generation works with real ROS packages from system
use rosidl_codegen::{
    GeneratorError, generate_action_package, generate_message_package, generate_service_package,
};
use rosidl_parser::{parse_action, parse_message, parse_service};
use std::{collections::HashSet, fs, path::Path};

mod parity_helpers;
use parity_helpers::{ros_input, ros_input_dir};

mod parity_ledger;
use parity_ledger::assert_message_dir_parity;

// Issue 0693 — resolve the INSTALLED distro instead of naming one. Every path
// in this file was a `/opt/ros/jazzy/...` literal while the project installs
// humble, so all nine tests took their "Skipping" arm and the suite reported
// PASS over work it never did.
//
// Issue 1160 — the local `ros_dir`/`ros_file` pair that used to sit here handed
// back a path that MIGHT NOT EXIST, so every test opened with two guards, both
// exiting PASS and both printing `[NO-ROS]` — the second of them on hosts that
// have ROS. `ros_input`/`ros_input_dir` in `parity_helpers` own that verdict
// now: `None` means "this host cannot supply the input", the message says which
// of the two reasons it is, and an installed package missing the named file is
// an assertion failure rather than a green.
//
// Issue 1176 — the `test_parse_all_*` walks below are a DIFFERENT class from
// either of those, and were the worse one: their precondition was met, the work
// ran, the failures were real and counted, and the verdict was still green,
// under the comment "Don't panic - just report the failures". They hold to a
// committed ledger now (`parity_ledger`, `tests/parity-expected-failures.txt`),
// exactly and in both directions. `bundled_interfaces_have_no_parity_failures`
// runs the same walk over the vendored sources in `packages/cli/interfaces/`,
// so the ratchet has a verdict on EVERY host — including the ROS-less
// `check-cli-tests` lane, where the three tests below can only return.

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
    let Some(ros_share) = ros_input_dir("parity_test", "std_msgs", "msg") else {
        return Ok(());
    };

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
    let Some(header_path) = ros_input("parity_test", "std_msgs", "msg", "Header.msg") else {
        return Ok(());
    };

    let msg = read_and_parse_message(&header_path).map_err(GeneratorError::InvalidMessage)?;

    let result = generate_message_package("std_msgs", "Header", &msg, &HashSet::new())?;

    // Header should have timestamp and frame_id
    assert!(result.message_rmw.contains("Header"));
    assert!(result.cargo_toml.contains("std_msgs"));

    Ok(())
}

#[test]
fn test_geometry_msgs_point() -> Result<(), GeneratorError> {
    let Some(point_path) = ros_input("parity_test", "geometry_msgs", "msg", "Point.msg") else {
        return Ok(());
    };

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
    let Some(pose_path) = ros_input("parity_test", "geometry_msgs", "msg", "Pose.msg") else {
        return Ok(());
    };

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
    let Some(srv_path) = ros_input("parity_test", "example_interfaces", "srv", "AddTwoInts.srv")
    else {
        return Ok(());
    };

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
    let Some(action_path) = ros_input(
        "parity_test",
        "example_interfaces",
        "action",
        "Fibonacci.action",
    ) else {
        return Ok(());
    };

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

// ---------------------------------------------------------------------------
// The whole-package walks — issue 1176
// ---------------------------------------------------------------------------
//
// Each of these used to end in a swallow: count the failures, print them to a
// stream libtest captures, and pass. They now hold the walk to
// `tests/parity-expected-failures.txt`, exactly: a failure that is not listed
// is red, and a listed entry that now succeeds is red too, so the tolerated set
// cannot grow or rot without a diff someone reviews.

#[test]
fn test_parse_all_std_msgs() {
    let Some(ros_share) = ros_input_dir("parity_test", "std_msgs", "msg") else {
        return;
    };
    assert_message_dir_parity("std_msgs", &ros_share, "test_parse_all_std_msgs");
}

#[test]
fn test_parse_all_geometry_msgs() {
    let Some(ros_share) = ros_input_dir("parity_test", "geometry_msgs", "msg") else {
        return;
    };
    assert_message_dir_parity("geometry_msgs", &ros_share, "test_parse_all_geometry_msgs");
}

#[test]
fn test_parse_all_sensor_msgs() {
    let Some(ros_share) = ros_input_dir("parity_test", "sensor_msgs", "msg") else {
        return;
    };
    assert_message_dir_parity("sensor_msgs", &ros_share, "test_parse_all_sensor_msgs");
}

/// The same walk over the VENDORED interface sources — issue 1176.
///
/// The three tests above can only answer on a host that has ROS 2 installed,
/// and the one lane that runs this suite (`check-cli-tests`) deliberately has
/// none: "It needs no ROS either, and that is a property of the suite rather
/// than an assumption" is a documented property of that job. So the ratchet
/// they carry would have fired on nobody's lane, which is most of the way back
/// to where issue 1176 started.
///
/// `packages/cli/interfaces/` is a vendored copy of the ROS 2 Humble sources
/// for exactly these packages — it exists so codegen works on a ROS-less host,
/// which makes it the same corpus, checked in. Walking it gives the ledger a
/// verdict on EVERY host, including CI.
///
/// Measured 2026-09-08, the first time anything asked: 133 `.msg` files across
/// 10 packages, zero parse failures, zero generate failures. That measurement
/// is why the ledger is empty, and it also retires the claim the old code
/// shipped instead of an assertion — `parse_message` handles scalar, string and
/// array defaults, bounded strings, bounded arrays and constants, so "parser
/// limitations (default values, etc.)" described nothing that was true.
#[test]
fn bundled_interfaces_have_no_parity_failures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("interfaces");
    assert!(
        root.is_dir(),
        "{} is missing — the bundled interface sources are TRACKED, so this is not \
         an absent environment (issue 1176)",
        root.display()
    );

    let mut packages: Vec<String> = fs::read_dir(&root)
        .expect("read packages/cli/interfaces")
        .flatten()
        .filter(|e| e.path().join("msg").is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    packages.sort();
    assert!(
        packages.len() >= 8,
        "only {} bundled packages carry a msg/ directory ({packages:?}); the vendored \
         set covers ten, and a shrunken walk reads exactly like a passing one",
        packages.len()
    );

    for package in &packages {
        assert_message_dir_parity(
            package,
            &root.join(package).join("msg"),
            &format!("bundled_interfaces_have_no_parity_failures[{package}]"),
        );
    }
}
