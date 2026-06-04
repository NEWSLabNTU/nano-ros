use rosidl_parser::{FieldType, Message};
use std::collections::HashSet;

/// Convert a PascalCase or camelCase string to snake_case
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let mut prev_is_lower = false;

    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            // Add underscore before uppercase if:
            // - Not the first character
            // - Previous character was lowercase
            if i > 0 && prev_is_lower {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
            prev_is_lower = false;
        } else {
            result.push(ch);
            prev_is_lower = ch.is_lowercase();
        }
    }

    result
}

/// Extract all package dependencies from a message
pub fn extract_dependencies(message: &Message) -> HashSet<String> {
    let mut deps = HashSet::new();

    for field in &message.fields {
        extract_deps_from_type(&field.field_type, &mut deps);
    }

    for constant in &message.constants {
        extract_deps_from_type(&constant.constant_type, &mut deps);
    }

    deps
}

fn extract_deps_from_type(field_type: &FieldType, deps: &mut HashSet<String>) {
    match field_type {
        FieldType::NamespacedType {
            package: Some(pkg), ..
        } => {
            deps.insert(pkg.clone());
        }
        FieldType::Array { element_type, .. }
        | FieldType::Sequence { element_type, .. }
        | FieldType::BoundedSequence { element_type, .. } => {
            extract_deps_from_type(element_type, deps);
        }
        _ => {}
    }
}

/// Check if a message needs serde's big-array feature (arrays > 32 elements)
pub fn needs_big_array(message: &Message) -> bool {
    for field in &message.fields {
        if has_large_array(&field.field_type) {
            return true;
        }
    }
    false
}

fn has_large_array(field_type: &FieldType) -> bool {
    match field_type {
        FieldType::Array { size, .. } if *size > 32 => true,
        FieldType::Array { element_type, .. }
        | FieldType::Sequence { element_type, .. }
        | FieldType::BoundedSequence { element_type, .. } => has_large_array(element_type),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosidl_parser::{Field, PrimitiveType, parse_message};

    #[test]
    fn test_extract_dependencies() {
        let msg = parse_message("geometry_msgs/Point position\nstd_msgs/Header header\n").unwrap();
        let deps = extract_dependencies(&msg);
        assert!(deps.contains("geometry_msgs"));
        assert!(deps.contains("std_msgs"));
        assert_eq!(deps.len(), 2);
    }

    #[test]
    fn test_no_dependencies() {
        let msg = parse_message("int32 x\nfloat64 y\n").unwrap();
        let deps = extract_dependencies(&msg);
        assert!(deps.is_empty());
    }

    #[test]
    fn test_needs_big_array() {
        let mut msg = Message::new();
        msg.fields.push(Field {
            field_type: FieldType::Array {
                element_type: Box::new(FieldType::Primitive(PrimitiveType::Int32)),
                size: 64,
            },
            name: "large_array".to_string(),
            default_value: None,
        });
        assert!(needs_big_array(&msg));
    }

    #[test]
    fn test_small_array_no_big_array() {
        let mut msg = Message::new();
        msg.fields.push(Field {
            field_type: FieldType::Array {
                element_type: Box::new(FieldType::Primitive(PrimitiveType::Int32)),
                size: 16,
            },
            name: "small_array".to_string(),
            default_value: None,
        });
        assert!(!needs_big_array(&msg));
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("Duration"), "duration");
        assert_eq!(to_snake_case("Time"), "time");
        assert_eq!(
            to_snake_case("TwistWithCovariance"),
            "twist_with_covariance"
        );
        assert_eq!(to_snake_case("PoseStamped"), "pose_stamped");
        assert_eq!(to_snake_case("camelCase"), "camel_case");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }
}
