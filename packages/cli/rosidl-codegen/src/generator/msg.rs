use super::common::{
    GeneratorError, build_c_field, build_nros_message_schema, determine_field_kind,
    field_to_nros_field, field_to_nros_field_with_mode,
};
use crate::{
    config::CapacityResolver,
    templates::{
        BuildRsTemplate, CConstant, CField, CargoNrosTomlTemplate, CargoTomlTemplate,
        IdiomaticField, LibNrosRsTemplate, LibRsTemplate, MessageCHeaderTemplate,
        MessageCSourceTemplate, MessageConstant, MessageIdiomaticTemplate, MessageNrosTemplate,
        MessageRmwTemplate, NrosField, RmwField,
    },
    types::{
        NrosCodegenMode, RosEdition, c_type_for_constant, constant_value_to_rust, escape_keyword,
        nros_type_for_constant, rust_type_for_constant, rust_type_for_field, to_c_package_name,
    },
    utils::{extract_dependencies, needs_big_array, to_snake_case},
};
use askama::Template;
use rosidl_parser::{FieldType, Message};
use std::collections::HashSet;

pub struct GeneratedPackage {
    pub cargo_toml: String,
    pub build_rs: String,
    pub lib_rs: String,
    pub message_rmw: String,
    pub message_idiomatic: String,
}

/// Generate a complete ROS 2 message package with both RMW and idiomatic layers
pub fn generate_message_package(
    package_name: &str,
    message_name: &str,
    message: &Message,
    all_dependencies: &HashSet<String>,
) -> Result<GeneratedPackage, GeneratorError> {
    // Extract dependencies from this specific message
    let msg_deps = extract_dependencies(message);

    // Combine with externally provided dependencies
    let mut all_deps: Vec<String> = all_dependencies.iter().cloned().collect();
    all_deps.extend(msg_deps);
    all_deps.sort();
    all_deps.dedup();

    // Check if we need serde's big-array feature
    let needs_big_array_feature = needs_big_array(message);

    // Generate Cargo.toml
    let cargo_toml_template = CargoTomlTemplate {
        package_name,
        dependencies: &all_deps,
        needs_big_array: needs_big_array_feature,
    };
    let cargo_toml = cargo_toml_template.render()?;

    // Generate build.rs
    let build_rs_template = BuildRsTemplate;
    let build_rs = build_rs_template.render()?;

    // Generate lib.rs
    let lib_rs_template = LibRsTemplate {
        has_messages: true,
        has_services: false,
        has_actions: false,
    };
    let lib_rs = lib_rs_template.render()?;

    // Generate RMW layer message
    let rmw_fields: Vec<RmwField> = message
        .fields
        .iter()
        .map(|f| RmwField {
            name: escape_keyword(&f.name),
            rust_type: rust_type_for_field(&f.field_type, true, Some(package_name)),
            default_value: f
                .default_value
                .as_ref()
                .map(constant_value_to_rust)
                .unwrap_or_default(),
        })
        .collect();

    let rmw_constants: Vec<MessageConstant> = message
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: rust_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let message_module = &to_snake_case(message_name);

    let message_rmw_template = MessageRmwTemplate {
        package_name,
        message_name,
        message_module,
        fields: rmw_fields,
        constants: rmw_constants,
    };
    let message_rmw = message_rmw_template.render()?;

    // Generate idiomatic layer message
    let idiomatic_fields: Vec<IdiomaticField> = message
        .fields
        .iter()
        .map(|f| IdiomaticField {
            name: escape_keyword(&f.name),
            rust_type: rust_type_for_field(&f.field_type, false, Some(package_name)),
            default_value: f
                .default_value
                .as_ref()
                .map(constant_value_to_rust)
                .unwrap_or_default(),
            kind: determine_field_kind(&f.field_type),
        })
        .collect();

    let idiomatic_constants: Vec<MessageConstant> = message
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: rust_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let message_idiomatic_template = MessageIdiomaticTemplate {
        package_name,
        message_name,
        message_module,
        fields: idiomatic_fields,
        constants: idiomatic_constants,
    };
    let message_idiomatic = message_idiomatic_template.render()?;

    Ok(GeneratedPackage {
        cargo_toml,
        build_rs,
        lib_rs,
        message_rmw,
        message_idiomatic,
    })
}

/// Generated nros message package
pub struct GeneratedNrosPackage {
    pub cargo_toml: String,
    pub lib_rs: String,
    pub message_rs: String,
}

/// Generate a nros message package
pub fn generate_nros_message_package(
    package_name: &str,
    message_name: &str,
    message: &Message,
    all_dependencies: &HashSet<String>,
    package_version: &str,
    edition: RosEdition,
    resolver: &CapacityResolver,
) -> Result<GeneratedNrosPackage, GeneratorError> {
    // Extract dependencies from this specific message
    let msg_deps = extract_dependencies(message);

    // Combine with externally provided dependencies
    let mut all_deps: Vec<String> = all_dependencies.iter().cloned().collect();
    all_deps.extend(msg_deps);
    all_deps.sort();
    all_deps.dedup();

    // Generate Cargo.toml
    let cargo_toml_template = CargoNrosTomlTemplate {
        package_name,
        package_version,
        dependencies: &all_deps,
    };
    let cargo_toml = cargo_toml_template.render()?;

    // Generate lib.rs
    let lib_rs_template = LibNrosRsTemplate {
        has_messages: true,
        has_services: false,
        has_actions: false,
    };
    let lib_rs = lib_rs_template.render()?;

    // Generate message fields
    let fields: Vec<NrosField> = message
        .fields
        .iter()
        .map(|f| field_to_nros_field(f, package_name, message_name, resolver))
        .collect::<Result<_, _>>()?;

    // Generate constants
    let constants: Vec<MessageConstant> = message
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let type_hash = edition.type_hash();

    let has_fields = !fields.is_empty();
    let has_large_array = fields.iter().any(|f| f.is_large_array);
    let schema = build_nros_message_schema(package_name, message_name, &message.fields);
    let message_template = MessageNrosTemplate {
        package_name,
        message_name,
        type_hash,
        fields,
        constants,
        has_fields,
        has_large_array,
        inline_mode: false,
        schema_helper_consts: schema.helper_consts,
        schema_fields_block: schema.fields_block,
        schema_type_name: schema.nros_type_name,
    };
    let message_rs = message_template.render()?;

    Ok(GeneratedNrosPackage {
        cargo_toml,
        lib_rs,
        message_rs,
    })
}

/// Generate a single message's Rust code in inline mode.
///
/// Unlike `generate_nros_message_package`, this only returns the rendered
/// message code (no Cargo.toml or lib.rs). Cross-package references use
/// `super::super::super::pkg::msg::Type` paths.
pub fn generate_nros_inline_message(
    package_name: &str,
    message_name: &str,
    message: &Message,
    edition: RosEdition,
    resolver: &CapacityResolver,
) -> Result<String, GeneratorError> {
    let mode = NrosCodegenMode::Inline;
    let fields: Vec<NrosField> = message
        .fields
        .iter()
        .map(|f| field_to_nros_field_with_mode(f, package_name, message_name, resolver, mode))
        .collect::<Result<_, _>>()?;

    let constants: Vec<MessageConstant> = message
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let type_hash = edition.type_hash();
    let has_fields = !fields.is_empty();
    let has_large_array = fields.iter().any(|f| f.is_large_array);
    let schema = build_nros_message_schema(package_name, message_name, &message.fields);

    let template = MessageNrosTemplate {
        package_name,
        message_name,
        type_hash,
        fields,
        constants,
        has_fields,
        has_large_array,
        inline_mode: true,
        schema_helper_consts: schema.helper_consts,
        schema_fields_block: schema.fields_block,
        schema_type_name: schema.nros_type_name,
    };

    Ok(template.render()?)
}

/// Generated C message package
pub struct GeneratedCPackage {
    /// Header file content (.h)
    pub header: String,
    /// Source file content (.c)
    pub source: String,
    /// Header filename
    pub header_name: String,
    /// Source filename
    pub source_name: String,
}

/// Generate C code for a message type
pub fn generate_c_message_package(
    package_name: &str,
    message_name: &str,
    message: &Message,
    type_hash: &str,
    resolver: &CapacityResolver,
) -> Result<GeneratedCPackage, GeneratorError> {
    let c_pkg_name = to_c_package_name(package_name);
    let msg_snake = to_snake_case(message_name);

    // Build struct and guard names
    let struct_name = format!("{}_msg_{}", c_pkg_name, msg_snake);
    let guard_name = format!(
        "{}_MSG_{}_H",
        c_pkg_name.to_uppercase(),
        msg_snake.to_uppercase()
    );
    let constant_prefix = format!(
        "{}_MSG_{}",
        c_pkg_name.to_uppercase(),
        msg_snake.to_uppercase()
    );
    let header_name = format!("{}_msg_{}.h", c_pkg_name, msg_snake);
    let source_name = format!("{}_msg_{}.c", c_pkg_name, msg_snake);

    // Extract dependencies — both cross-package (umbrella includes) and
    // intra-package (per-type includes for types in the same package).
    let mut dependencies = Vec::new();
    let mut type_includes = Vec::new();
    for field in &message.fields {
        let field_type = match &field.field_type {
            FieldType::NamespacedType { .. } => Some(&field.field_type),
            FieldType::Array { element_type, .. }
            | FieldType::Sequence { element_type }
            | FieldType::BoundedSequence { element_type, .. } => {
                if matches!(element_type.as_ref(), FieldType::NamespacedType { .. }) {
                    Some(element_type.as_ref())
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(FieldType::NamespacedType { package, name }) = field_type {
            let pkg = package.as_deref().unwrap_or(package_name);
            let dep = to_c_package_name(pkg);
            let header_filename =
                format!("{}_msg_{}.h", to_c_package_name(pkg), to_snake_case(name));
            let type_header = if dep != c_pkg_name {
                // Cross-package: include with subdirectory path
                if !dependencies.contains(&dep) {
                    dependencies.push(dep.clone());
                }
                format!("{}/msg/{}", dep, header_filename)
            } else {
                // Intra-package: include from same msg/ directory
                header_filename
            };
            if !type_includes.contains(&type_header) {
                type_includes.push(type_header);
            }
        }
    }
    dependencies.sort();
    type_includes.sort();

    // Build C fields
    let fields: Vec<CField> = message
        .fields
        .iter()
        .map(|field| {
            build_c_field(
                &field.name,
                &field.field_type,
                Some(package_name),
                message_name,
                resolver,
            )
        })
        .collect::<Result<_, _>>()?;

    // Build C constants
    let constants: Vec<CConstant> = message
        .constants
        .iter()
        .map(|constant| CConstant {
            name: constant.name.clone(),
            c_type: c_type_for_constant(&constant.constant_type),
            value: constant_value_to_rust(&constant.value),
        })
        .collect();

    let has_fields = !fields.is_empty();

    // Generate header
    let header_template = MessageCHeaderTemplate {
        package_name,
        message_name,
        type_hash,
        guard_name,
        struct_name: struct_name.clone(),
        constant_prefix,
        fields: fields.clone(),
        constants,
        dependencies,
        type_includes,
        has_fields,
    };
    let header = header_template.render()?;

    // Generate source
    let source_template = MessageCSourceTemplate {
        package_name,
        message_name,
        type_hash,
        header_name: header_name.clone(),
        struct_name,
        fields,
        has_fields,
    };
    let source = source_template.render()?;

    Ok(GeneratedCPackage {
        header,
        source,
        header_name,
        source_name,
    })
}
