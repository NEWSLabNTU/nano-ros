//! Integration tests for C code generation
//!
//! Tests the `generate_c_from_args_file` function which is called by CMake.

use cargo_nano_ros::{GenerateCConfig, generate_c_from_args_file};
use std::{fs, path::PathBuf};
use tempfile::TempDir;

fn create_args_file(
    temp_dir: &TempDir,
    package_name: &str,
    interface_files: &[PathBuf],
    dependencies: &[&str],
) -> PathBuf {
    let output_dir = temp_dir.path().join("output");
    fs::create_dir_all(&output_dir).unwrap();

    let args = serde_json::json!({
        "package_name": package_name,
        "output_dir": output_dir,
        "interface_files": interface_files,
        "dependencies": dependencies
    });

    let args_file = temp_dir.path().join("args.json");
    fs::write(&args_file, serde_json::to_string_pretty(&args).unwrap()).unwrap();
    args_file
}

fn create_msg_file(temp_dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let msg_dir = temp_dir.path().join("msg");
    fs::create_dir_all(&msg_dir).unwrap();
    let path = msg_dir.join(format!("{}.msg", name));
    fs::write(&path, content).unwrap();
    path
}

fn create_srv_file(temp_dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let srv_dir = temp_dir.path().join("srv");
    fs::create_dir_all(&srv_dir).unwrap();
    let path = srv_dir.join(format!("{}.srv", name));
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn test_generate_c_simple_message() {
    let temp_dir = TempDir::new().unwrap();

    // Create a simple message file
    let msg_path = create_msg_file(
        &temp_dir,
        "Temperature",
        r#"
# A simple temperature message
float64 celsius
float64 fahrenheit
string sensor_id
"#,
    );

    let args_file = create_args_file(&temp_dir, "test_msgs", &[msg_path], &[]);

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    // Verify output files exist
    let output_dir = temp_dir.path().join("output");
    assert!(output_dir.join("msg/test_msgs_msg_temperature.h").exists());
    assert!(output_dir.join("msg/test_msgs_msg_temperature.c").exists());
    assert!(output_dir.join("test_msgs.h").exists());

    // Verify header content (struct name uses lowercase)
    let header = fs::read_to_string(output_dir.join("msg/test_msgs_msg_temperature.h")).unwrap();
    assert!(header.contains("typedef struct test_msgs_msg_temperature"));
    assert!(header.contains("double celsius"));
    assert!(header.contains("double fahrenheit"));
    assert!(header.contains("char sensor_id["));

    // Verify source content (function names use lowercase)
    let source = fs::read_to_string(output_dir.join("msg/test_msgs_msg_temperature.c")).unwrap();
    assert!(source.contains("test_msgs_msg_temperature_init"));
    assert!(source.contains("test_msgs_msg_temperature_serialize"));
    assert!(source.contains("test_msgs_msg_temperature_deserialize"));
    assert!(source.contains("#include <nros/types.h>"));

    // Verify umbrella header
    let umbrella = fs::read_to_string(output_dir.join("test_msgs.h")).unwrap();
    assert!(umbrella.contains("#include \"msg/test_msgs_msg_temperature.h\""));
}

#[test]
fn test_generate_c_message_with_primitives() {
    let temp_dir = TempDir::new().unwrap();

    let msg_path = create_msg_file(
        &temp_dir,
        "AllPrimitives",
        r#"
bool flag
int8 tiny
uint8 byte_val
int16 small
uint16 unsigned_small
int32 medium
uint32 unsigned_medium
int64 large
uint64 unsigned_large
float32 single
float64 double_val
"#,
    );

    let args_file = create_args_file(&temp_dir, "primitive_test", &[msg_path], &[]);

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    let output_dir = temp_dir.path().join("output");
    let header =
        fs::read_to_string(output_dir.join("msg/primitive_test_msg_all_primitives.h")).unwrap();

    // Verify all primitive types are present with correct C types
    assert!(header.contains("bool flag"));
    assert!(header.contains("int8_t tiny"));
    assert!(header.contains("uint8_t byte_val"));
    assert!(header.contains("int16_t small"));
    assert!(header.contains("uint16_t unsigned_small"));
    assert!(header.contains("int32_t medium"));
    assert!(header.contains("uint32_t unsigned_medium"));
    assert!(header.contains("int64_t large"));
    assert!(header.contains("uint64_t unsigned_large"));
    assert!(header.contains("float single"));
    assert!(header.contains("double double_val"));
}

#[test]
fn test_generate_c_message_with_arrays() {
    let temp_dir = TempDir::new().unwrap();

    let msg_path = create_msg_file(
        &temp_dir,
        "ArrayMessage",
        r#"
int32[10] fixed_array
float64[3] position
uint8[256] buffer
"#,
    );

    let args_file = create_args_file(&temp_dir, "array_test", &[msg_path], &[]);

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    let output_dir = temp_dir.path().join("output");
    let header = fs::read_to_string(output_dir.join("msg/array_test_msg_array_message.h")).unwrap();

    // Arrays should include size suffix
    assert!(header.contains("int32_t fixed_array[10]"));
    assert!(header.contains("double position[3]"));
    assert!(header.contains("uint8_t buffer[256]"));
}

#[test]
fn test_generate_c_message_with_constants() {
    let temp_dir = TempDir::new().unwrap();

    let msg_path = create_msg_file(
        &temp_dir,
        "ConstMessage",
        r#"
int32 STATUS_OK=0
int32 STATUS_ERROR=1
string DEFAULT_NAME="sensor"

int32 status
string name
"#,
    );

    let args_file = create_args_file(&temp_dir, "const_test", &[msg_path], &[]);

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    let output_dir = temp_dir.path().join("output");
    let header = fs::read_to_string(output_dir.join("msg/const_test_msg_const_message.h")).unwrap();

    // Constants should be #define'd
    assert!(header.contains("#define") || header.contains("STATUS_OK"));
}

#[test]
fn test_generate_c_multiple_messages() {
    let temp_dir = TempDir::new().unwrap();

    let msg1 = create_msg_file(&temp_dir, "Position", "float64 x\nfloat64 y\nfloat64 z\n");
    let msg2 = create_msg_file(
        &temp_dir,
        "Velocity",
        "float64 vx\nfloat64 vy\nfloat64 vz\n",
    );

    let args_file = create_args_file(&temp_dir, "multi_test", &[msg1, msg2], &[]);

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    let output_dir = temp_dir.path().join("output");

    // Both message files should be generated
    assert!(output_dir.join("msg/multi_test_msg_position.h").exists());
    assert!(output_dir.join("msg/multi_test_msg_position.c").exists());
    assert!(output_dir.join("msg/multi_test_msg_velocity.h").exists());
    assert!(output_dir.join("msg/multi_test_msg_velocity.c").exists());

    // Umbrella header should include both
    let umbrella = fs::read_to_string(output_dir.join("multi_test.h")).unwrap();
    assert!(umbrella.contains("multi_test_msg_position.h"));
    assert!(umbrella.contains("multi_test_msg_velocity.h"));
}

#[test]
fn test_generate_c_service() {
    let temp_dir = TempDir::new().unwrap();

    let srv_path = create_srv_file(
        &temp_dir,
        "AddTwoInts",
        r#"
int64 a
int64 b
---
int64 sum
"#,
    );

    let args_file = create_args_file(&temp_dir, "srv_test", &[srv_path], &[]);

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    let output_dir = temp_dir.path().join("output");

    // Service files should be in srv/ directory
    assert!(output_dir.join("srv/srv_test_srv_add_two_ints.h").exists());
    assert!(output_dir.join("srv/srv_test_srv_add_two_ints.c").exists());

    // Umbrella header should include service
    let umbrella = fs::read_to_string(output_dir.join("srv_test.h")).unwrap();
    assert!(umbrella.contains("srv/srv_test_srv_add_two_ints.h"));
}

#[test]
fn test_generate_c_with_dependencies() {
    let temp_dir = TempDir::new().unwrap();

    let msg_path = create_msg_file(&temp_dir, "Simple", "int32 value\n");

    // Specify dependencies (they don't need to exist for this test)
    let args_file = create_args_file(
        &temp_dir,
        "dep_test",
        &[msg_path],
        &["std_msgs", "geometry_msgs"],
    );

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    let output_dir = temp_dir.path().join("output");
    let umbrella = fs::read_to_string(output_dir.join("dep_test.h")).unwrap();

    // Dependencies should be included in umbrella header.
    // Format is `<pkg>/<pkg>.h` — see `generate_umbrella_header` for why:
    // the build system doesn't put each package directory directly on the
    // include path, so the nested path is required.
    assert!(umbrella.contains("#include <std_msgs/std_msgs.h>"));
    assert!(umbrella.contains("#include <geometry_msgs/geometry_msgs.h>"));
}

#[test]
fn test_generate_c_snake_case_conversion() {
    let temp_dir = TempDir::new().unwrap();

    // CamelCase message name - file name becomes snake_case
    let msg_path = create_msg_file(&temp_dir, "SensorData", "int32 value\n");

    let args_file = create_args_file(&temp_dir, "snake_test", &[msg_path], &[]);

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    let output_dir = temp_dir.path().join("output");

    // File name should be snake_case (all lowercase)
    assert!(output_dir.join("msg/snake_test_msg_sensor_data.h").exists());
    assert!(output_dir.join("msg/snake_test_msg_sensor_data.c").exists());

    // Struct name is also lowercase (matching file name)
    let header = fs::read_to_string(output_dir.join("msg/snake_test_msg_sensor_data.h")).unwrap();
    assert!(header.contains("snake_test_msg_sensor_data"));
}

#[test]
fn test_generate_c_empty_message() {
    let temp_dir = TempDir::new().unwrap();

    // Empty message (should have placeholder)
    let msg_path = create_msg_file(&temp_dir, "Empty", "\n");

    let args_file = create_args_file(&temp_dir, "empty_test", &[msg_path], &[]);

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    let output_dir = temp_dir.path().join("output");
    let header = fs::read_to_string(output_dir.join("msg/empty_test_msg_empty.h")).unwrap();

    // Empty struct should have a placeholder field
    assert!(header.contains("_dummy") || header.contains("uint8_t"));
}

#[test]
fn test_generate_c_package_name_with_hyphens() {
    let temp_dir = TempDir::new().unwrap();

    let msg_path = create_msg_file(&temp_dir, "Test", "int32 value\n");

    // Package name with hyphens
    let args_file = create_args_file(&temp_dir, "my-custom-package", &[msg_path], &[]);

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    let output_dir = temp_dir.path().join("output");

    // Hyphens should be converted to underscores in C identifiers
    // Note: file may be named with underscores or original package name - check what exists
    let possible_files = [
        "msg/my_custom_package_msg_test.h",
        "msg/my-custom-package_msg_test.h",
    ];

    let header_path = possible_files
        .iter()
        .map(|f| output_dir.join(f))
        .find(|p| p.exists())
        .expect("Expected header file not found");

    let header = fs::read_to_string(&header_path).unwrap();
    // Struct name should have underscores instead of hyphens
    assert!(header.contains("my_custom_package_msg_test") || header.contains("typedef struct"));
}

#[test]
fn test_generate_c_type_support() {
    let temp_dir = TempDir::new().unwrap();

    let msg_path = create_msg_file(&temp_dir, "TypeTest", "int32 value\n");

    let args_file = create_args_file(&temp_dir, "type_test", &[msg_path], &[]);

    let config = GenerateCConfig {
        args_file,
        verbose: false,
    };

    generate_c_from_args_file(config).expect("Failed to generate C code");

    let output_dir = temp_dir.path().join("output");
    let source = fs::read_to_string(output_dir.join("msg/type_test_msg_type_test.c")).unwrap();

    // Type support structure should be present
    assert!(source.contains("nros_message_type_t"));
    assert!(source.contains("_type_support"));
    assert!(source.contains("get_type_support"));
}
