use rosidl_parser::{FieldType, Message};
use std::collections::BTreeSet;

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

/// rosidl's `convert_camel_case_to_lower_case_underscore`: the file name
/// rosidl_generator_c gives a type's header (`LaserScan` -> `laser_scan`,
/// `UInt8` -> `u_int8`, `Float32MultiArray` -> `float32_multi_array`).
///
/// Two passes, as upstream: an underscore before a capital that starts a
/// lower-case run, then before a capital that follows a lower-case letter or a
/// digit. [`to_snake_case`] is the C++ and Rust surfaces' rule and differs on
/// both the digit case and the acronym case.
pub fn to_rosidl_snake_case(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    // Pass 1: `(.)([A-Z][a-z]+)` -> `\1_\2`, non-overlapping, left to right.
    let mut p1: Vec<char> = Vec::with_capacity(c.len() + 4);
    let mut i = 0;
    while i < c.len() {
        if i + 2 < c.len() && c[i + 1].is_ascii_uppercase() && c[i + 2].is_ascii_lowercase() {
            // match: c[i] then [A-Z][a-z]+
            p1.push(c[i]);
            p1.push('_');
            let mut j = i + 1;
            p1.push(c[j]);
            j += 1;
            while j < c.len() && c[j].is_ascii_lowercase() {
                p1.push(c[j]);
                j += 1;
            }
            i = j;
        } else {
            p1.push(c[i]);
            i += 1;
        }
    }
    // Pass 2: `([a-z0-9])([A-Z])` -> `\1_\2`, non-overlapping.
    let mut out = String::with_capacity(p1.len() + 4);
    let mut k = 0;
    while k < p1.len() {
        if k + 1 < p1.len()
            && (p1[k].is_ascii_lowercase() || p1[k].is_ascii_digit())
            && p1[k + 1].is_ascii_uppercase()
        {
            out.push(p1[k]);
            out.push('_');
            out.push(p1[k + 1]);
            k += 2;
        } else {
            out.push(p1[k]);
            k += 1;
        }
    }
    out.to_lowercase()
}

/// Extract all package dependencies from a message
pub fn extract_dependencies(message: &Message) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();

    for field in &message.fields {
        extract_deps_from_type(&field.field_type, &mut deps);
    }

    for constant in &message.constants {
        extract_deps_from_type(&constant.constant_type, &mut deps);
    }

    deps
}

fn extract_deps_from_type(field_type: &FieldType, deps: &mut BTreeSet<String>) {
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
