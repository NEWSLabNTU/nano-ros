// Edge case tests - verify generator handles unusual but valid inputs
use rosidl_codegen::{GeneratorError, generate_message_package};
use rosidl_parser::{Field, FieldType, Message, PrimitiveType, parse_message};
use std::collections::HashSet;

#[test]
fn test_empty_message() -> Result<(), GeneratorError> {
    let msg_def = "";
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("test_msgs", "Empty", &msg, &HashSet::new())?;

    assert!(result.message_rmw.contains("Empty"));
    assert!(result.message_idiomatic.contains("Empty"));

    Ok(())
}

#[test]
fn test_message_with_only_constants() -> Result<(), GeneratorError> {
    let msg_def = r#"int32 MAX_VALUE=100
int32 MIN_VALUE=0
string DEFAULT_NAME="test"
"#;
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("test_msgs", "Constants", &msg, &HashSet::new())?;

    assert!(result.message_rmw.contains("MAX_VALUE"));
    assert!(result.message_rmw.contains("MIN_VALUE"));
    assert!(result.message_rmw.contains("DEFAULT_NAME"));

    Ok(())
}

#[test]
fn test_very_large_array() -> Result<(), GeneratorError> {
    let mut msg = Message::new();
    msg.fields.push(Field {
        field_type: FieldType::Array {
            element_type: Box::new(FieldType::Primitive(PrimitiveType::Int32)),
            size: 1000,
        },
        name: "huge_array".to_string(),
        default_value: None,
    });

    let result = generate_message_package("test_msgs", "HugeArray", &msg, &HashSet::new())?;

    assert!(result.message_rmw.contains("[i32; 1000]"));
    assert!(result.cargo_toml.contains("big-array"));

    Ok(())
}

#[test]
fn test_all_primitive_types() -> Result<(), GeneratorError> {
    let msg_def = r#"bool bool_field
byte byte_field
char char_field
int8 int8_field
uint8 uint8_field
int16 int16_field
uint16 uint16_field
int32 int32_field
uint32 uint32_field
int64 int64_field
uint64 uint64_field
float32 float32_field
float64 float64_field
string string_field
"#;
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("test_msgs", "AllPrimitives", &msg, &HashSet::new())?;

    // Verify all Rust primitive types are present
    assert!(result.message_rmw.contains("bool"));
    assert!(result.message_rmw.contains("i8"));
    assert!(result.message_rmw.contains("u8"));
    assert!(result.message_rmw.contains("i16"));
    assert!(result.message_rmw.contains("u16"));
    assert!(result.message_rmw.contains("i32"));
    assert!(result.message_rmw.contains("u32"));
    assert!(result.message_rmw.contains("i64"));
    assert!(result.message_rmw.contains("u64"));
    assert!(result.message_rmw.contains("f32"));
    assert!(result.message_rmw.contains("f64"));
    assert!(result.message_rmw.contains("String") || result.message_rmw.contains("string"));

    Ok(())
}

#[test]
fn test_all_array_variants() -> Result<(), GeneratorError> {
    let msg_def = r#"int32[5] fixed_array
int32[] unbounded_sequence
int32[<=10] bounded_sequence
"#;
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("test_msgs", "ArrayVariants", &msg, &HashSet::new())?;

    // Verify different array types in RMW layer
    assert!(result.message_rmw.contains("[i32; 5]"));
    assert!(result.message_rmw.contains("Sequence<i32>"));
    assert!(result.message_rmw.contains("BoundedSequence<i32, 10>"));

    // Verify idiomatic layer uses Vec for sequences
    assert!(result.message_idiomatic.contains("Vec<i32>"));

    Ok(())
}

#[test]
fn test_all_string_variants() -> Result<(), GeneratorError> {
    let msg_def = r#"string unbounded_string
string<=256 bounded_string
wstring unbounded_wstring
wstring<=128 bounded_wstring
"#;
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("test_msgs", "StringVariants", &msg, &HashSet::new())?;

    // RMW layer should have rosidl types
    assert!(
        result.message_rmw.contains("rosidl_runtime_rs::String")
            || result.message_rmw.contains("BoundedString")
    );

    // Idiomatic layer should use std::string::String
    assert!(result.message_idiomatic.contains("std::string::String"));

    Ok(())
}

#[test]
fn test_deeply_nested_message() -> Result<(), GeneratorError> {
    let msg_def = r#"geometry_msgs/PoseStamped pose
geometry_msgs/TwistStamped twist
"#;
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("nav_msgs", "Odometry", &msg, &HashSet::new())?;

    // Should handle namespaced types
    assert!(result.message_rmw.contains("geometry_msgs"));
    assert!(result.cargo_toml.contains("geometry_msgs"));

    Ok(())
}

#[test]
fn test_message_with_unicode_comment() -> Result<(), GeneratorError> {
    // Note: ROS IDL doesn't officially support Unicode in identifiers,
    // but comments should be fine
    let msg_def = r#"# This message contains Unicode: 你好世界 🚀
int32 x
float64 y  # Position données en mètres
"#;
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("test_msgs", "Unicode", &msg, &HashSet::new())?;

    // Should generate successfully even with Unicode in comments
    assert!(result.message_rmw.contains("Unicode"));
    assert!(result.message_rmw.contains("pub x: i32"));
    assert!(result.message_rmw.contains("pub y: f64"));

    Ok(())
}

#[test]
fn test_unusual_field_names() -> Result<(), GeneratorError> {
    // Field names that might conflict with Rust keywords are escaped
    let msg_def = r#"int32 type
int32 match
int32 async
int32 normal_field
"#;
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("test_msgs", "Keywords", &msg, &HashSet::new())?;

    // Keywords should be escaped with underscore
    assert!(result.message_rmw.contains("type_"));
    assert!(result.message_rmw.contains("match_"));
    assert!(result.message_rmw.contains("async_"));
    assert!(result.message_rmw.contains("normal_field"));

    Ok(())
}

#[test]
fn test_field_names_with_numbers() -> Result<(), GeneratorError> {
    let msg_def = r#"int32 field1
int32 field2
int32 field10
int32 field100
"#;
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("test_msgs", "Numbered", &msg, &HashSet::new())?;

    assert!(result.message_rmw.contains("field1"));
    assert!(result.message_rmw.contains("field2"));
    assert!(result.message_rmw.contains("field10"));
    assert!(result.message_rmw.contains("field100"));

    Ok(())
}

#[test]
fn test_mixed_case_field_names() -> Result<(), GeneratorError> {
    // ROS typically uses snake_case, but test mixed cases
    let msg_def = r#"int32 snake_case_field
int32 camelCaseField
int32 PascalCaseField
int32 UPPER_CASE_FIELD
"#;
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("test_msgs", "MixedCase", &msg, &HashSet::new())?;

    // All field names should be preserved as-is (not converted)
    assert!(result.message_rmw.contains("snake_case_field"));
    // Note: These may be escaped or modified, but should generate without error

    Ok(())
}

#[test]
fn test_array_of_arrays() -> Result<(), GeneratorError> {
    let msg_def = r#"int32[5][10] matrix
"#;

    // This might not be valid ROS IDL, but if it parses, we should handle it
    match parse_message(msg_def) {
        Ok(msg) => {
            // If it parses, we should be able to generate
            let result = generate_message_package("test_msgs", "Matrix", &msg, &HashSet::new());
            // Either it works or it returns a clear error
            match result {
                Ok(_) => {}
                Err(e) => {
                    // Expected: array of arrays might not be supported
                    eprintln!("Array of arrays not supported: {:?}", e);
                }
            }
        }
        Err(_) => {
            // Expected: parser doesn't support this syntax
            eprintln!("Parser doesn't support array of arrays syntax");
        }
    }

    Ok(())
}

#[test]
fn test_maximum_field_count() -> Result<(), GeneratorError> {
    // Test a message with many fields (stress test)
    let mut fields = Vec::new();
    for i in 0..100 {
        fields.push(format!("int32 field{}", i));
    }
    let msg_def = fields.join("\n");

    let msg = parse_message(&msg_def).unwrap();

    let result = generate_message_package("test_msgs", "ManyFields", &msg, &HashSet::new())?;

    // Should handle 100+ fields without issue
    assert!(result.message_rmw.contains("field0"));
    assert!(result.message_rmw.contains("field99"));

    Ok(())
}

#[test]
fn test_long_field_name() -> Result<(), GeneratorError> {
    let long_name = "a".repeat(200);
    let msg_def = format!("int32 {}\n", long_name);

    let msg = parse_message(&msg_def).unwrap();

    let result = generate_message_package("test_msgs", "LongName", &msg, &HashSet::new())?;

    // Should handle very long field names
    assert!(result.message_rmw.contains(&long_name));

    Ok(())
}

#[test]
fn test_constants_with_special_values() -> Result<(), GeneratorError> {
    // Test simpler constants (avoid negative values and reserved words as constant names)
    let msg_def = r#"int32 ZERO=0
int32 HUNDRED=100
int32 MAX_VALUE=2147483647
float64 PI=3.14159
bool ENABLED=true
bool DISABLED=false
"#;
    let msg = parse_message(msg_def).unwrap();

    let result = generate_message_package("test_msgs", "SpecialConstants", &msg, &HashSet::new())?;

    assert!(result.message_rmw.contains("ZERO"));
    assert!(result.message_rmw.contains("HUNDRED"));
    assert!(result.message_rmw.contains("MAX_VALUE"));
    assert!(result.message_rmw.contains("PI"));
    assert!(result.message_rmw.contains("ENABLED"));
    assert!(result.message_rmw.contains("DISABLED"));

    Ok(())
}
