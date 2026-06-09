use super::common::{
    GeneratorError, build_c_field, build_nros_schema_for_struct, determine_field_kind,
    field_to_nros_field, field_to_nros_field_with_mode,
};
use crate::{
    config::CapacityResolver,
    templates::{
        BuildRsTemplate, CConstant, CField, CargoNrosTomlTemplate, CargoTomlTemplate,
        IdiomaticField, LibNrosRsTemplate, LibRsTemplate, MessageConstant, NrosField, RmwField,
        ServiceCHeaderTemplate, ServiceCSourceTemplate, ServiceIdiomaticTemplate,
        ServiceNrosTemplate, ServiceRmwTemplate,
    },
    types::{
        NrosCodegenMode, RosEdition, c_type_for_constant, constant_value_to_rust, escape_keyword,
        nros_type_for_constant, rust_type_for_constant, rust_type_for_field, to_c_package_name,
    },
    utils::{extract_dependencies, needs_big_array, to_snake_case},
};
use askama::Template;
use rosidl_parser::{FieldType, Message, Service};
use std::collections::HashSet;

pub struct GeneratedServicePackage {
    pub cargo_toml: String,
    pub build_rs: String,
    pub lib_rs: String,
    pub service_rmw: String,
    pub service_idiomatic: String,
}

/// Generate a complete ROS 2 service package with both RMW and idiomatic layers
pub fn generate_service_package(
    package_name: &str,
    service_name: &str,
    service: &Service,
    all_dependencies: &HashSet<String>,
) -> Result<GeneratedServicePackage, GeneratorError> {
    // Extract dependencies from request and response
    let mut req_deps = extract_dependencies(&service.request);
    let resp_deps = extract_dependencies(&service.response);
    req_deps.extend(resp_deps);

    // Combine with externally provided dependencies
    let mut all_deps: Vec<String> = all_dependencies.iter().cloned().collect();
    all_deps.extend(req_deps);
    all_deps.sort();
    all_deps.dedup();

    // Check if we need serde's big-array feature
    let needs_big_array_feature =
        needs_big_array(&service.request) || needs_big_array(&service.response);

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
        has_messages: false,
        has_services: true,
        has_actions: false,
    };
    let lib_rs = lib_rs_template.render()?;

    // Helper functions to convert Message to field vectors
    let message_to_rmw_fields = |msg: &Message| {
        msg.fields
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
            .collect()
    };

    let message_to_idiomatic_fields = |msg: &Message| {
        msg.fields
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
            .collect()
    };

    let message_to_constants = |msg: &Message, _rmw_layer: bool| {
        msg.constants
            .iter()
            .map(|c| MessageConstant {
                name: c.name.clone(),
                rust_type: rust_type_for_constant(&c.constant_type),
                value: constant_value_to_rust(&c.value),
            })
            .collect()
    };

    // Generate RMW layer service
    let service_rmw_template = ServiceRmwTemplate {
        package_name,
        service_name,
        request_fields: message_to_rmw_fields(&service.request),
        request_constants: message_to_constants(&service.request, true),
        response_fields: message_to_rmw_fields(&service.response),
        response_constants: message_to_constants(&service.response, true),
    };
    let service_rmw = service_rmw_template.render()?;

    // Generate idiomatic layer service
    let service_idiomatic_template = ServiceIdiomaticTemplate {
        package_name,
        service_name,
        request_fields: message_to_idiomatic_fields(&service.request),
        request_constants: message_to_constants(&service.request, false),
        response_fields: message_to_idiomatic_fields(&service.response),
        response_constants: message_to_constants(&service.response, false),
    };
    let service_idiomatic = service_idiomatic_template.render()?;

    Ok(GeneratedServicePackage {
        cargo_toml,
        build_rs,
        lib_rs,
        service_rmw,
        service_idiomatic,
    })
}

/// Generated nros service package
pub struct GeneratedNrosServicePackage {
    pub cargo_toml: String,
    pub lib_rs: String,
    pub service_rs: String,
}

/// Generate a nros service package
pub fn generate_nros_service_package(
    package_name: &str,
    service_name: &str,
    service: &Service,
    all_dependencies: &HashSet<String>,
    package_version: &str,
    edition: RosEdition,
) -> Result<GeneratedNrosServicePackage, GeneratorError> {
    // Extract dependencies from request and response
    let mut req_deps = extract_dependencies(&service.request);
    let resp_deps = extract_dependencies(&service.response);
    req_deps.extend(resp_deps);

    // Combine with externally provided dependencies
    let mut all_deps: Vec<String> = all_dependencies.iter().cloned().collect();
    all_deps.extend(req_deps);
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
        has_messages: false,
        has_services: true,
        has_actions: false,
    };
    let lib_rs = lib_rs_template.render()?;

    // Per-field capacity config is not yet threaded into services (Phase 229
    // wave: messages first); an empty resolver reproduces built-in defaults.
    let resolver = CapacityResolver::empty();
    let request_msg = format!("{service_name}_Request");
    let response_msg = format!("{service_name}_Response");

    // Generate request fields
    let request_fields: Vec<NrosField> = service
        .request
        .fields
        .iter()
        .map(|f| field_to_nros_field(f, package_name, &request_msg, &resolver))
        .collect::<Result<_, _>>()?;

    let request_constants: Vec<MessageConstant> = service
        .request
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    // Generate response fields
    let response_fields: Vec<NrosField> = service
        .response
        .fields
        .iter()
        .map(|f| field_to_nros_field(f, package_name, &response_msg, &resolver))
        .collect::<Result<_, _>>()?;

    let response_constants: Vec<MessageConstant> = service
        .response
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let type_hash = edition.type_hash();

    let has_request_fields = !request_fields.is_empty();
    let has_response_fields = !response_fields.is_empty();
    let has_request_large_array = request_fields.iter().any(|f| f.is_large_array);
    let has_response_large_array = response_fields.iter().any(|f| f.is_large_array);
    let req_struct = format!("{}Request", service_name);
    let resp_struct = format!("{}Response", service_name);
    let req_schema = build_nros_schema_for_struct(
        package_name,
        &req_struct,
        &format!("{}/srv/{}_Request", package_name, service_name),
        "REQ_",
        &service.request.fields,
    );
    let resp_schema = build_nros_schema_for_struct(
        package_name,
        &resp_struct,
        &format!("{}/srv/{}_Response", package_name, service_name),
        "RESP_",
        &service.response.fields,
    );

    let service_template = ServiceNrosTemplate {
        package_name,
        service_name,
        type_hash,
        request_fields,
        request_constants,
        response_fields,
        response_constants,
        has_request_fields,
        has_response_fields,
        has_request_large_array,
        has_response_large_array,
        inline_mode: false,
        req_schema_helper_consts: req_schema.helper_consts,
        req_schema_fields_block: req_schema.fields_block,
        req_schema_type_name: req_schema.nros_type_name,
        resp_schema_helper_consts: resp_schema.helper_consts,
        resp_schema_fields_block: resp_schema.fields_block,
        resp_schema_type_name: resp_schema.nros_type_name,
    };
    let service_rs = service_template.render()?;

    Ok(GeneratedNrosServicePackage {
        cargo_toml,
        lib_rs,
        service_rs,
    })
}

/// Generate a single service's Rust code in inline mode.
pub fn generate_nros_inline_service(
    package_name: &str,
    service_name: &str,
    service: &Service,
    edition: RosEdition,
) -> Result<String, GeneratorError> {
    let mode = NrosCodegenMode::Inline;
    let resolver = CapacityResolver::empty();
    let request_msg = format!("{service_name}_Request");
    let response_msg = format!("{service_name}_Response");

    let request_fields: Vec<NrosField> = service
        .request
        .fields
        .iter()
        .map(|f| field_to_nros_field_with_mode(f, package_name, &request_msg, &resolver, mode))
        .collect::<Result<_, _>>()?;

    let request_constants: Vec<MessageConstant> = service
        .request
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let response_fields: Vec<NrosField> = service
        .response
        .fields
        .iter()
        .map(|f| field_to_nros_field_with_mode(f, package_name, &response_msg, &resolver, mode))
        .collect::<Result<_, _>>()?;

    let response_constants: Vec<MessageConstant> = service
        .response
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let type_hash = edition.type_hash();
    let has_request_fields = !request_fields.is_empty();
    let has_response_fields = !response_fields.is_empty();
    let has_request_large_array = request_fields.iter().any(|f| f.is_large_array);
    let has_response_large_array = response_fields.iter().any(|f| f.is_large_array);

    let req_struct = format!("{}Request", service_name);
    let resp_struct = format!("{}Response", service_name);
    let req_schema = build_nros_schema_for_struct(
        package_name,
        &req_struct,
        &format!("{}/srv/{}_Request", package_name, service_name),
        "REQ_",
        &service.request.fields,
    );
    let resp_schema = build_nros_schema_for_struct(
        package_name,
        &resp_struct,
        &format!("{}/srv/{}_Response", package_name, service_name),
        "RESP_",
        &service.response.fields,
    );

    let template = ServiceNrosTemplate {
        package_name,
        service_name,
        type_hash,
        request_fields,
        request_constants,
        response_fields,
        response_constants,
        has_request_fields,
        has_response_fields,
        has_request_large_array,
        has_response_large_array,
        inline_mode: true,
        req_schema_helper_consts: req_schema.helper_consts,
        req_schema_fields_block: req_schema.fields_block,
        req_schema_type_name: req_schema.nros_type_name,
        resp_schema_helper_consts: resp_schema.helper_consts,
        resp_schema_fields_block: resp_schema.fields_block,
        resp_schema_type_name: resp_schema.nros_type_name,
    };

    Ok(template.render()?)
}

/// Generated C service package
pub struct GeneratedCServicePackage {
    /// Header file content (.h)
    pub header: String,
    /// Source file content (.c)
    pub source: String,
    /// Header filename
    pub header_name: String,
    /// Source filename
    pub source_name: String,
}

/// Generate C code for a service type
pub fn generate_c_service_package(
    package_name: &str,
    service_name: &str,
    service: &Service,
    type_hash: &str,
) -> Result<GeneratedCServicePackage, GeneratorError> {
    let c_pkg_name = to_c_package_name(package_name);
    let srv_snake = to_snake_case(service_name);

    // Build struct and guard names
    let service_struct_name = format!("{}_srv_{}", c_pkg_name, srv_snake);
    let request_struct_name = format!("{}_srv_{}_request", c_pkg_name, srv_snake);
    let response_struct_name = format!("{}_srv_{}_response", c_pkg_name, srv_snake);
    let guard_name = format!(
        "{}_SRV_{}_H",
        c_pkg_name.to_uppercase(),
        srv_snake.to_uppercase()
    );
    let constant_prefix = format!(
        "{}_SRV_{}",
        c_pkg_name.to_uppercase(),
        srv_snake.to_uppercase()
    );
    let header_name = format!("{}_srv_{}.h", c_pkg_name, srv_snake);
    let source_name = format!("{}_srv_{}.c", c_pkg_name, srv_snake);

    // Extract dependencies from both request and response.
    // Mirrors the logic in msg.rs: produce one `type_includes` entry per
    // referenced struct (same-package OR cross-package) and a separate
    // `dependencies` list of other packages (used elsewhere for metadata).
    let mut dependencies = Vec::new();
    let mut type_includes = Vec::new();
    for field in service
        .request
        .fields
        .iter()
        .chain(service.response.fields.iter())
    {
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
                // Cross-package: include with subdirectory path.
                if !dependencies.contains(&dep) {
                    dependencies.push(dep.clone());
                }
                format!("{}/msg/{}", dep, header_filename)
            } else {
                // Intra-package: the srv .h lives at <pkg>/srv/, so the
                // msg header is one level up in ../msg/.
                format!("../msg/{}", header_filename)
            };
            if !type_includes.contains(&type_header) {
                type_includes.push(type_header);
            }
        }
    }
    dependencies.sort();
    type_includes.sort();

    // Build C fields for request
    let request_fields: Vec<CField> = service
        .request
        .fields
        .iter()
        .map(|field| build_c_field(&field.name, &field.field_type, Some(package_name)))
        .collect();

    let request_constants: Vec<CConstant> = service
        .request
        .constants
        .iter()
        .map(|constant| CConstant {
            name: constant.name.clone(),
            c_type: c_type_for_constant(&constant.constant_type),
            value: constant_value_to_rust(&constant.value),
        })
        .collect();

    // Build C fields for response
    let response_fields: Vec<CField> = service
        .response
        .fields
        .iter()
        .map(|field| build_c_field(&field.name, &field.field_type, Some(package_name)))
        .collect();

    let response_constants: Vec<CConstant> = service
        .response
        .constants
        .iter()
        .map(|constant| CConstant {
            name: constant.name.clone(),
            c_type: c_type_for_constant(&constant.constant_type),
            value: constant_value_to_rust(&constant.value),
        })
        .collect();

    let has_request_fields = !request_fields.is_empty();
    let has_response_fields = !response_fields.is_empty();

    // Generate header
    let header_template = ServiceCHeaderTemplate {
        package_name,
        service_name,
        type_hash,
        guard_name,
        service_struct_name: service_struct_name.clone(),
        request_struct_name: request_struct_name.clone(),
        response_struct_name: response_struct_name.clone(),
        constant_prefix,
        request_fields: request_fields.clone(),
        request_constants,
        response_fields: response_fields.clone(),
        response_constants,
        dependencies,
        type_includes,
        has_request_fields,
        has_response_fields,
    };
    let header = header_template.render()?;

    // Generate source
    let source_template = ServiceCSourceTemplate {
        package_name,
        service_name,
        type_hash,
        header_name: header_name.clone(),
        service_struct_name,
        request_struct_name,
        response_struct_name,
        request_fields,
        response_fields,
        has_request_fields,
        has_response_fields,
    };
    let source = source_template.render()?;

    Ok(GeneratedCServicePackage {
        header,
        source,
        header_name,
        source_name,
    })
}
