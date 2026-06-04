pub mod generator;
pub mod idl_generator;
pub mod templates;
pub mod types;
pub mod utils;

pub use generator::{
    GeneratedActionPackage, GeneratedCActionPackage, GeneratedCPackage, GeneratedCServicePackage,
    GeneratedCppActionPackage, GeneratedCppPackage, GeneratedCppServicePackage,
    GeneratedNrosActionPackage, GeneratedNrosPackage, GeneratedNrosServicePackage,
    GeneratedPackage, GeneratedServicePackage, GeneratorError, generate_action_package,
    generate_c_action_package, generate_c_message_package, generate_c_service_package,
    generate_cpp_action_package, generate_cpp_message_package, generate_cpp_service_package,
    generate_message_package, generate_nros_action_package, generate_nros_inline_action,
    generate_nros_inline_message, generate_nros_inline_service, generate_nros_message_package,
    generate_nros_service_package, generate_service_package,
};
pub use idl_generator::{GeneratedIdlCode, extract_annotations, generate_idl_file};
pub use types::{
    C_DEFAULT_SEQUENCE_CAPACITY, C_DEFAULT_STRING_CAPACITY, CPP_DEFAULT_SEQUENCE_CAPACITY,
    CPP_DEFAULT_STRING_CAPACITY, CodegenBackend, FieldTypeExt, IdlTypeExt,
    NROS_DEFAULT_SEQUENCE_CAPACITY, NROS_DEFAULT_STRING_CAPACITY, NrosCodegenMode, RosEdition,
    c_array_suffix_for_field, c_type_for_constant, c_type_for_field, compute_serialized_size_max,
    cpp_type_for_field, escape_keyword, idl_constant_value_to_rust, nros_type_for_constant,
    nros_type_for_field, nros_type_for_field_with_mode, repr_c_type_for_field, rust_type_for_field,
    rust_type_for_idl, rust_type_for_idl_constant, to_c_package_name,
};

#[cfg(test)]
mod tests {
    use super::*;
    use rosidl_parser::{FieldType, PrimitiveType, parse_message};

    #[test]
    fn test_basic_type_mapping() {
        let field_type = FieldType::Primitive(PrimitiveType::Int32);
        let rust_type = rust_type_for_field(&field_type, false, None);
        assert_eq!(rust_type, "i32");
    }

    #[test]
    fn test_keyword_escaping() {
        assert_eq!(escape_keyword("type"), "type_");
        assert_eq!(escape_keyword("match"), "match_");
        assert_eq!(escape_keyword("normal"), "normal");
    }

    #[test]
    fn test_simple_message_generation() {
        let msg = parse_message("int32 x\nfloat64 y\n").unwrap();
        let result = generate_message_package(
            "test_msgs",
            "TestMessage",
            &msg,
            &std::collections::HashSet::new(),
        );
        assert!(result.is_ok());
    }
}
