//! IDL-specific code generation.
//!
//! This module handles generation of Rust code from IDL files, including:
//! - Constant modules
//! - Enums
//! - Structs with annotations

use crate::types::{
    annotation_value_to_constant_value, escape_keyword, idl_constant_value_to_rust,
    idl_primitive_to_primitive, rust_type_for_idl, rust_type_for_idl_constant, to_snake_case,
};
use rosidl_parser::{
    ast::{FieldType, Message},
    idl::{
        ast::{
            Annotation, AnnotationValue, ConstantModule, EnumDef, IdlFile, IdlModule, IdlStruct,
            StructMember,
        },
        types::IdlType,
    },
};
use std::collections::HashSet;

/// Generate Rust code for an IDL file.
pub fn generate_idl_file(
    package_name: &str,
    idl_file: &IdlFile,
    dependencies: &HashSet<String>,
) -> Result<GeneratedIdlCode, String> {
    let mut code = GeneratedIdlCode::new();

    // Generate code for the module hierarchy
    generate_module_code(&idl_file.module, package_name, dependencies, &mut code)?;

    Ok(code)
}

/// Generated IDL code.
#[derive(Debug)]
pub struct GeneratedIdlCode {
    /// Generated structs (messages)
    pub structs: Vec<(String, String)>, // (name, code)
    /// Generated constant modules
    pub constant_modules: Vec<(String, String)>, // (name, code)
    /// Generated enums
    pub enums: Vec<(String, String)>, // (name, code)
}

impl GeneratedIdlCode {
    fn new() -> Self {
        Self {
            structs: Vec::new(),
            constant_modules: Vec::new(),
            enums: Vec::new(),
        }
    }
}

/// Generate code for a module (recursive).
fn generate_module_code(
    module: &IdlModule,
    package_name: &str,
    dependencies: &HashSet<String>,
    code: &mut GeneratedIdlCode,
) -> Result<(), String> {
    // Generate constant modules
    for const_mod in &module.constant_modules {
        let module_code = generate_constant_module(const_mod)?;
        code.constant_modules
            .push((const_mod.name.clone(), module_code));
    }

    // Generate enums
    for enum_def in &module.enums {
        let enum_code = generate_enum(enum_def)?;
        code.enums.push((enum_def.name.clone(), enum_code));
    }

    // Generate structs
    for struct_def in &module.structs {
        let struct_code = generate_struct(struct_def, package_name, dependencies)?;
        code.structs.push((struct_def.name.clone(), struct_code));
    }

    // Recursively process nested modules
    for nested_module in &module.modules {
        generate_module_code(nested_module, package_name, dependencies, code)?;
    }

    Ok(())
}

/// Generate code for a constant module.
fn generate_constant_module(const_mod: &ConstantModule) -> Result<String, String> {
    let mut code = String::new();

    // Module doc comment
    code.push_str(&format!("/// Constants for {}\n", const_mod.name));
    code.push_str(&format!("pub mod {} {{\n", to_snake_case(&const_mod.name)));

    // Generate constants
    for constant in &const_mod.constants {
        let const_name = constant.name.to_uppercase();
        let const_type = rust_type_for_idl_constant(&constant.const_type);
        let const_value = idl_constant_value_to_rust(&constant.value);

        code.push_str(&format!(
            "    pub const {}: {} = {};\n",
            const_name, const_type, const_value
        ));
    }

    code.push_str("}\n");
    Ok(code)
}

/// Generate code for an enum.
fn generate_enum(enum_def: &EnumDef) -> Result<String, String> {
    let mut code = String::new();

    // Enum doc comment
    code.push_str(&format!("/// Enum: {}\n", enum_def.name));

    // Enum definition
    code.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    code.push_str("#[repr(i32)]\n");
    code.push_str(&format!("pub enum {} {{\n", enum_def.name));

    // Generate variants
    for (idx, variant) in enum_def.variants.iter().enumerate() {
        if let Some(value) = variant.value {
            code.push_str(&format!("    {} = {},\n", variant.name, value));
        } else {
            // Auto-assign value based on index
            code.push_str(&format!("    {} = {},\n", variant.name, idx));
        }
    }

    code.push_str("}\n");
    Ok(code)
}

/// Generate code for a struct (message).
fn generate_struct(
    struct_def: &IdlStruct,
    package_name: &str,
    _dependencies: &HashSet<String>,
) -> Result<String, String> {
    let mut code = String::new();

    // Add documentation from @verbatim annotation if present
    if let Some(verbatim) = struct_def.get_annotation("verbatim")
        && let Some(AnnotationValue::String(text)) = verbatim.get_param("text")
    {
        code.push_str(&format!("/// {}\n", text));
    }

    // Struct definition
    code.push_str("#[derive(Debug, Clone, PartialEq)]\n");
    code.push_str(&format!("pub struct {} {{\n", struct_def.name));

    // Generate fields
    for member in &struct_def.members {
        generate_field(&mut code, member, package_name)?;
    }

    code.push_str("}\n\n");

    // Generate Default trait if any fields have @default annotation
    if struct_def
        .members
        .iter()
        .any(|m| m.has_annotation("default"))
    {
        generate_default_impl(&mut code, struct_def, package_name)?;
    }

    Ok(code)
}

/// Generate a field in a struct.
fn generate_field(
    code: &mut String,
    member: &StructMember,
    package_name: &str,
) -> Result<(), String> {
    // Add field documentation from @verbatim if present
    if let Some(verbatim) = member.get_annotation("verbatim")
        && let Some(AnnotationValue::String(text)) = verbatim.get_param("text")
    {
        code.push_str(&format!("    /// {}\n", text));
    }

    // Add comment for @key annotation
    if member.has_annotation("key") {
        code.push_str("    /// Key field for DDS keyed topics\n");
    }

    // Field definition
    let field_name = escape_keyword(&member.name);
    let field_type = rust_type_for_idl(&member.member_type, false, Some(package_name));

    code.push_str(&format!("    pub {}: {},\n", field_name, field_type));

    Ok(())
}

/// Generate Default trait implementation for a struct.
fn generate_default_impl(
    code: &mut String,
    struct_def: &IdlStruct,
    _package_name: &str,
) -> Result<(), String> {
    code.push_str(&format!("impl Default for {} {{\n", struct_def.name));
    code.push_str("    fn default() -> Self {\n");
    code.push_str("        Self {\n");

    for member in &struct_def.members {
        let field_name = escape_keyword(&member.name);

        // Check for @default annotation
        if let Some(default_value) = member.get_default_value() {
            let value_str = match default_value {
                AnnotationValue::Integer(i) => i.to_string(),
                AnnotationValue::Float(f) => {
                    if f.fract() == 0.0 {
                        format!("{:.1}", f)
                    } else {
                        f.to_string()
                    }
                }
                AnnotationValue::Boolean(b) => b.to_string(),
                AnnotationValue::String(s) => format!("\"{}\".to_string()", s.escape_default()),
                AnnotationValue::Identifier(id) => id.clone(),
            };

            code.push_str(&format!("            {}: {},\n", field_name, value_str));
        } else {
            // Use Default::default() for fields without @default
            code.push_str(&format!(
                "            {}: Default::default(),\n",
                field_name
            ));
        }
    }

    code.push_str("        }\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    Ok(())
}

/// Extract annotations from a struct.
pub fn extract_annotations(struct_def: &IdlStruct) -> Vec<(String, Annotation)> {
    struct_def
        .annotations
        .iter()
        .map(|a| (a.name.clone(), a.clone()))
        .collect()
}

/// Convert an IDL struct to a Message (for RMW layer generation).
///
/// This allows IDL structs to use the same code generation templates as .msg files.
pub fn idl_struct_to_message(struct_def: &IdlStruct, package_name: &str) -> Message {
    let mut message = Message::new();

    // Convert struct members to message fields
    for member in &struct_def.members {
        let field_type = idl_type_to_field_type(&member.member_type, package_name);

        // Check if this member has a @default annotation
        let default_value = member
            .get_default_value()
            .map(annotation_value_to_constant_value);

        message.fields.push(rosidl_parser::ast::Field {
            field_type,
            name: member.name.clone(),
            default_value,
        });
    }

    message
}

/// Convert an IDL type to a FieldType (for .msg compatibility).
fn idl_type_to_field_type(idl_type: &IdlType, package_name: &str) -> FieldType {
    match idl_type {
        IdlType::Primitive(prim) => FieldType::Primitive(idl_primitive_to_primitive(prim)),
        IdlType::String(bound) => match bound {
            Some(size) => FieldType::BoundedString(*size),
            None => FieldType::String,
        },
        IdlType::WString(bound) => match bound {
            Some(size) => FieldType::BoundedWString(*size),
            None => FieldType::WString,
        },
        IdlType::Sequence(inner, bound) => {
            let element_type = Box::new(idl_type_to_field_type(inner, package_name));
            match bound {
                Some(max_size) => FieldType::BoundedSequence {
                    element_type,
                    max_size: *max_size,
                },
                None => FieldType::Sequence { element_type },
            }
        }
        IdlType::Array(inner, dimensions) => {
            // Issue 0250 — multi-dimensional IDL arrays lower to NESTED fixed
            // arrays, outermost dimension last in the fold (row-major). CDR
            // fixed arrays are contiguous with no per-element headers, so
            // nested arrays are byte-identical to the flat multi-dim layout —
            // the old first-dimension-only lowering silently produced a
            // wire-incompatible type. A dimensionless array (shouldn't
            // parse) degenerates to the bare element type.
            let mut ty = idl_type_to_field_type(inner, package_name);
            for size in dimensions.iter().rev() {
                ty = FieldType::Array {
                    element_type: Box::new(ty),
                    size: *size,
                };
            }
            ty
        }
        IdlType::UserDefined(type_name) => {
            // Check if it's a scoped name (package::Type)
            if type_name.contains("::") {
                // Parse scoped name
                let parts: Vec<&str> = type_name.split("::").collect();
                if parts.len() == 2 {
                    FieldType::NamespacedType {
                        package: Some(parts[0].to_string()),
                        name: parts[1].to_string(),
                    }
                } else {
                    // Fall back to using current package
                    FieldType::NamespacedType {
                        package: Some(package_name.to_string()),
                        name: type_name.clone(),
                    }
                }
            } else {
                // Type from same package
                FieldType::NamespacedType {
                    package: Some(package_name.to_string()),
                    name: type_name.clone(),
                }
            }
        }
        IdlType::Scoped(parts) => {
            // Scoped name like module::submodule::Type
            if parts.len() >= 2 {
                let package = parts[0].clone();
                let name = parts.last().unwrap().clone();
                FieldType::NamespacedType {
                    package: Some(package),
                    name,
                }
            } else if parts.len() == 1 {
                // Single part - type from current package
                FieldType::NamespacedType {
                    package: Some(package_name.to_string()),
                    name: parts[0].clone(),
                }
            } else {
                // Empty parts - shouldn't happen, but handle gracefully
                FieldType::NamespacedType {
                    package: Some(package_name.to_string()),
                    name: "Unknown".to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosidl_parser::idl::{
        ast::{
            AnnotationValue, ConstantModule, ConstantValue as IdlConstantValue, EnumDef,
            EnumVariant, IdlConstant, IdlStruct, StructMember,
        },
        types::{IdlPrimitiveType, IdlType},
    };

    /// Issue 0250 — multi-dim IDL arrays lower to nested fixed arrays
    /// (outer dimension outermost), never a first-dimension-only flatten.
    #[test]
    fn test_multidim_array_lowers_to_nested_arrays() {
        let idl = IdlType::Array(
            Box::new(IdlType::Primitive(IdlPrimitiveType::Float)),
            vec![3, 4],
        );
        let ft = idl_type_to_field_type(&idl, "pkg");
        match ft {
            FieldType::Array { element_type, size } => {
                assert_eq!(size, 3, "outer dimension first");
                match *element_type {
                    FieldType::Array {
                        element_type: inner,
                        size,
                    } => {
                        assert_eq!(size, 4, "inner dimension nested");
                        assert!(matches!(*inner, FieldType::Primitive(_)));
                    }
                    other => panic!("inner not an array: {other:?}"),
                }
            }
            other => panic!("outer not an array: {other:?}"),
        }
    }

    #[test]
    fn test_generate_constant_module() {
        let mut const_mod = ConstantModule::new("MyMessage_Constants".to_string());
        const_mod.add_constant(IdlConstant {
            name: "MAX_VALUE".to_string(),
            const_type: IdlType::Primitive(IdlPrimitiveType::Short),
            value: IdlConstantValue::Integer(42),
        });

        let code = generate_constant_module(&const_mod).unwrap();
        assert!(code.contains("pub mod my_message_constants"));
        assert!(code.contains("pub const MAX_VALUE: i16 = 42;"));
    }

    #[test]
    fn test_generate_enum() {
        let mut enum_def = EnumDef::new("Status".to_string());
        enum_def.add_variant(EnumVariant {
            name: "OK".to_string(),
            value: Some(0),
        });
        enum_def.add_variant(EnumVariant {
            name: "ERROR".to_string(),
            value: Some(1),
        });

        let code = generate_enum(&enum_def).unwrap();
        assert!(code.contains("pub enum Status"));
        assert!(code.contains("OK = 0"));
        assert!(code.contains("ERROR = 1"));
    }

    #[test]
    fn test_generate_struct() {
        let mut struct_def = IdlStruct::new("TestMessage".to_string());
        struct_def.add_member(StructMember::new(
            "value".to_string(),
            IdlType::Primitive(IdlPrimitiveType::Long),
        ));

        let code = generate_struct(&struct_def, "test_pkg", &HashSet::new()).unwrap();
        assert!(code.contains("pub struct TestMessage"));
        assert!(code.contains("pub value: i32"));
    }

    #[test]
    fn test_generate_struct_with_default() {
        let mut struct_def = IdlStruct::new("TestMessage".to_string());

        let mut member = StructMember::new(
            "count".to_string(),
            IdlType::Primitive(IdlPrimitiveType::UnsignedShort),
        );

        let mut default_annotation = Annotation::new("default".to_string());
        default_annotation.add_param("value".to_string(), AnnotationValue::Integer(123));
        member.add_annotation(default_annotation);

        struct_def.add_member(member);

        let code = generate_struct(&struct_def, "test_pkg", &HashSet::new()).unwrap();
        assert!(code.contains("impl Default for TestMessage"));
        assert!(code.contains("count: 123"));
    }
}
