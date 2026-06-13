use super::common::{
    GeneratorError, build_action_envelope_schemas, build_c_field, build_nros_schema_for_struct,
    determine_field_kind, field_to_nros_field, field_to_nros_field_with_mode,
};
use crate::{
    config::CapacityResolver,
    templates::{
        ActionCHeaderTemplate, ActionCSourceTemplate, ActionIdiomaticTemplate, ActionNrosTemplate,
        ActionRmwTemplate, BuildRsTemplate, CConstant, CField, CargoNrosTomlTemplate,
        CargoTomlTemplate, IdiomaticField, LibNrosRsTemplate, LibRsTemplate, MessageConstant,
        NrosField, RmwField,
    },
    types::{
        NrosCodegenMode, RosEdition, c_type_for_constant, constant_value_to_rust, escape_keyword,
        nros_type_for_constant, rust_type_for_constant, rust_type_for_field, to_c_package_name,
    },
    utils::{extract_dependencies, needs_big_array, to_snake_case},
};
use askama::Template;
use rosidl_parser::{Action, FieldType, Message};
use std::collections::HashSet;

pub struct GeneratedActionPackage {
    pub cargo_toml: String,
    pub build_rs: String,
    pub lib_rs: String,
    pub action_rmw: String,
    pub action_idiomatic: String,
}

/// Generate a complete ROS 2 action package with both RMW and idiomatic layers
pub fn generate_action_package(
    package_name: &str,
    action_name: &str,
    action: &Action,
    all_dependencies: &HashSet<String>,
) -> Result<GeneratedActionPackage, GeneratorError> {
    // Extract dependencies from goal, result, and feedback
    let mut goal_deps = extract_dependencies(&action.spec.goal);
    let result_deps = extract_dependencies(&action.spec.result);
    let feedback_deps = extract_dependencies(&action.spec.feedback);
    goal_deps.extend(result_deps);
    goal_deps.extend(feedback_deps);

    // Combine with externally provided dependencies
    let mut all_deps: Vec<String> = all_dependencies.iter().cloned().collect();
    all_deps.extend(goal_deps);
    all_deps.sort();
    all_deps.dedup();

    // Check if we need serde's big-array feature
    let needs_big_array_feature = needs_big_array(&action.spec.goal)
        || needs_big_array(&action.spec.result)
        || needs_big_array(&action.spec.feedback);

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
        has_services: false,
        has_actions: true,
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

    // Generate RMW layer action
    let action_rmw_template = ActionRmwTemplate {
        package_name,
        action_name,
        goal_fields: message_to_rmw_fields(&action.spec.goal),
        goal_constants: message_to_constants(&action.spec.goal, true),
        result_fields: message_to_rmw_fields(&action.spec.result),
        result_constants: message_to_constants(&action.spec.result, true),
        feedback_fields: message_to_rmw_fields(&action.spec.feedback),
        feedback_constants: message_to_constants(&action.spec.feedback, true),
    };
    let action_rmw = action_rmw_template.render()?;

    // Generate idiomatic layer action
    let action_idiomatic_template = ActionIdiomaticTemplate {
        package_name,
        action_name,
        goal_fields: message_to_idiomatic_fields(&action.spec.goal),
        goal_constants: message_to_constants(&action.spec.goal, false),
        result_fields: message_to_idiomatic_fields(&action.spec.result),
        result_constants: message_to_constants(&action.spec.result, false),
        feedback_fields: message_to_idiomatic_fields(&action.spec.feedback),
        feedback_constants: message_to_constants(&action.spec.feedback, false),
    };
    let action_idiomatic = action_idiomatic_template.render()?;

    Ok(GeneratedActionPackage {
        cargo_toml,
        build_rs,
        lib_rs,
        action_rmw,
        action_idiomatic,
    })
}

/// Generated nros action package
pub struct GeneratedNrosActionPackage {
    pub cargo_toml: String,
    pub lib_rs: String,
    pub action_rs: String,
}

/// Generate a nros action package
pub fn generate_nros_action_package(
    package_name: &str,
    action_name: &str,
    action: &Action,
    all_dependencies: &HashSet<String>,
    package_version: &str,
    edition: RosEdition,
    resolver: &CapacityResolver,
) -> Result<GeneratedNrosActionPackage, GeneratorError> {
    // Extract dependencies from goal, result, and feedback
    let mut goal_deps = extract_dependencies(&action.spec.goal);
    let result_deps = extract_dependencies(&action.spec.result);
    let feedback_deps = extract_dependencies(&action.spec.feedback);
    goal_deps.extend(result_deps);
    goal_deps.extend(feedback_deps);

    // Combine with externally provided dependencies
    let mut all_deps: Vec<String> = all_dependencies.iter().cloned().collect();
    all_deps.extend(goal_deps);

    // Phase 212.K.7.1.d: action envelope structs reference
    // `unique_identifier_msgs::msg::UUID` (every envelope with a
    // `goal_id`) + `builtin_interfaces::msg::Time` (SendGoal_Response
    // `stamp`). Inject the deps unconditionally so the generated
    // `Cargo.toml` resolves the nested-type `<Pkg::msg::T as Message>`
    // expansions.
    if package_name != "unique_identifier_msgs" {
        all_deps.push("unique_identifier_msgs".to_string());
    }
    if package_name != "builtin_interfaces" {
        all_deps.push("builtin_interfaces".to_string());
    }
    // Phase 244 E3 (RFC-0044) — the generated `impl RosAction::register_protocol_types`
    // names `action_msgs::srv::CancelGoal_{Request,Response}` + `msg::GoalStatusArray`,
    // so the action crate must depend on `action_msgs` (its own envelopes are local,
    // these three are not). `action_msgs` itself has no actions, so no self-dep.
    if package_name != "action_msgs" {
        all_deps.push("action_msgs".to_string());
    }
    all_deps.sort();
    all_deps.dedup();

    // Generate Cargo.toml
    let cargo_toml_template = CargoNrosTomlTemplate {
        package_name,
        package_version,
        dependencies: &all_deps,
        has_actions: true,
    };
    let cargo_toml = cargo_toml_template.render()?;

    // Generate lib.rs
    let lib_rs_template = LibNrosRsTemplate {
        has_messages: false,
        has_services: false,
        has_actions: true,
    };
    let lib_rs = lib_rs_template.render()?;

    let goal_msg = format!("{action_name}_Goal");
    let result_msg = format!("{action_name}_Result");
    let feedback_msg = format!("{action_name}_Feedback");

    // Generate goal fields
    let goal_fields: Vec<NrosField> = action
        .spec
        .goal
        .fields
        .iter()
        .map(|f| field_to_nros_field(f, package_name, &goal_msg, resolver))
        .collect::<Result<_, _>>()?;

    let goal_constants: Vec<MessageConstant> = action
        .spec
        .goal
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    // Generate result fields
    let result_fields: Vec<NrosField> = action
        .spec
        .result
        .fields
        .iter()
        .map(|f| field_to_nros_field(f, package_name, &result_msg, resolver))
        .collect::<Result<_, _>>()?;

    let result_constants: Vec<MessageConstant> = action
        .spec
        .result
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    // Generate feedback fields
    let feedback_fields: Vec<NrosField> = action
        .spec
        .feedback
        .fields
        .iter()
        .map(|f| field_to_nros_field(f, package_name, &feedback_msg, resolver))
        .collect::<Result<_, _>>()?;

    let feedback_constants: Vec<MessageConstant> = action
        .spec
        .feedback
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let type_hash = edition.type_hash();

    let has_goal_fields = !goal_fields.is_empty();
    let has_result_fields = !result_fields.is_empty();
    let has_feedback_fields = !feedback_fields.is_empty();
    let has_goal_large_array = goal_fields.iter().any(|f| f.is_large_array);
    let has_result_large_array = result_fields.iter().any(|f| f.is_large_array);
    let has_feedback_large_array = feedback_fields.iter().any(|f| f.is_large_array);

    let goal_struct = format!("{}Goal", action_name);
    let result_struct = format!("{}Result", action_name);
    let feedback_struct = format!("{}Feedback", action_name);
    let goal_schema = build_nros_schema_for_struct(
        package_name,
        &goal_struct,
        &format!("{}/action/{}_Goal", package_name, action_name),
        "GOAL_",
        &action.spec.goal.fields,
    );
    let result_schema = build_nros_schema_for_struct(
        package_name,
        &result_struct,
        &format!("{}/action/{}_Result", package_name, action_name),
        "RESULT_",
        &action.spec.result.fields,
    );
    let feedback_schema = build_nros_schema_for_struct(
        package_name,
        &feedback_struct,
        &format!("{}/action/{}_Feedback", package_name, action_name),
        "FEEDBACK_",
        &action.spec.feedback.fields,
    );

    let envelopes = build_action_envelope_schemas(package_name, action_name);

    let action_template = ActionNrosTemplate {
        package_name,
        action_name,
        type_hash,
        goal_fields,
        goal_constants,
        result_fields,
        result_constants,
        feedback_fields,
        feedback_constants,
        has_goal_fields,
        has_result_fields,
        has_feedback_fields,
        has_goal_large_array,
        has_result_large_array,
        has_feedback_large_array,
        inline_mode: false,
        goal_schema_helper_consts: goal_schema.helper_consts,
        goal_schema_fields_block: goal_schema.fields_block,
        goal_schema_type_name: goal_schema.nros_type_name,
        result_schema_helper_consts: result_schema.helper_consts,
        result_schema_fields_block: result_schema.fields_block,
        result_schema_type_name: result_schema.nros_type_name,
        feedback_schema_helper_consts: feedback_schema.helper_consts,
        feedback_schema_fields_block: feedback_schema.fields_block,
        feedback_schema_type_name: feedback_schema.nros_type_name,
        send_goal_request_schema_helper_consts: envelopes.send_goal_request.helper_consts,
        send_goal_request_schema_fields_block: envelopes.send_goal_request.fields_block,
        send_goal_request_schema_type_name: envelopes.send_goal_request.nros_type_name,
        send_goal_response_schema_helper_consts: envelopes.send_goal_response.helper_consts,
        send_goal_response_schema_fields_block: envelopes.send_goal_response.fields_block,
        send_goal_response_schema_type_name: envelopes.send_goal_response.nros_type_name,
        get_result_request_schema_helper_consts: envelopes.get_result_request.helper_consts,
        get_result_request_schema_fields_block: envelopes.get_result_request.fields_block,
        get_result_request_schema_type_name: envelopes.get_result_request.nros_type_name,
        get_result_response_schema_helper_consts: envelopes.get_result_response.helper_consts,
        get_result_response_schema_fields_block: envelopes.get_result_response.fields_block,
        get_result_response_schema_type_name: envelopes.get_result_response.nros_type_name,
        feedback_message_schema_helper_consts: envelopes.feedback_message.helper_consts,
        feedback_message_schema_fields_block: envelopes.feedback_message.fields_block,
        feedback_message_schema_type_name: envelopes.feedback_message.nros_type_name,
    };
    let action_rs = action_template.render()?;

    Ok(GeneratedNrosActionPackage {
        cargo_toml,
        lib_rs,
        action_rs,
    })
}

/// Generate a single action's Rust code in inline mode.
pub fn generate_nros_inline_action(
    package_name: &str,
    action_name: &str,
    action: &Action,
    edition: RosEdition,
    resolver: &CapacityResolver,
) -> Result<String, GeneratorError> {
    let mode = NrosCodegenMode::Inline;
    let goal_msg = format!("{action_name}_Goal");
    let result_msg = format!("{action_name}_Result");
    let feedback_msg = format!("{action_name}_Feedback");

    let goal_fields: Vec<NrosField> = action
        .spec
        .goal
        .fields
        .iter()
        .map(|f| field_to_nros_field_with_mode(f, package_name, &goal_msg, resolver, mode))
        .collect::<Result<_, _>>()?;

    let goal_constants: Vec<MessageConstant> = action
        .spec
        .goal
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let result_fields: Vec<NrosField> = action
        .spec
        .result
        .fields
        .iter()
        .map(|f| field_to_nros_field_with_mode(f, package_name, &result_msg, resolver, mode))
        .collect::<Result<_, _>>()?;

    let result_constants: Vec<MessageConstant> = action
        .spec
        .result
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let feedback_fields: Vec<NrosField> = action
        .spec
        .feedback
        .fields
        .iter()
        .map(|f| field_to_nros_field_with_mode(f, package_name, &feedback_msg, resolver, mode))
        .collect::<Result<_, _>>()?;

    let feedback_constants: Vec<MessageConstant> = action
        .spec
        .feedback
        .constants
        .iter()
        .map(|c| MessageConstant {
            name: c.name.clone(),
            rust_type: nros_type_for_constant(&c.constant_type),
            value: constant_value_to_rust(&c.value),
        })
        .collect();

    let type_hash = edition.type_hash();
    let has_goal_fields = !goal_fields.is_empty();
    let has_result_fields = !result_fields.is_empty();
    let has_feedback_fields = !feedback_fields.is_empty();
    let has_goal_large_array = goal_fields.iter().any(|f| f.is_large_array);
    let has_result_large_array = result_fields.iter().any(|f| f.is_large_array);
    let has_feedback_large_array = feedback_fields.iter().any(|f| f.is_large_array);

    let goal_struct = format!("{}Goal", action_name);
    let result_struct = format!("{}Result", action_name);
    let feedback_struct = format!("{}Feedback", action_name);
    let goal_schema = build_nros_schema_for_struct(
        package_name,
        &goal_struct,
        &format!("{}/action/{}_Goal", package_name, action_name),
        "GOAL_",
        &action.spec.goal.fields,
    );
    let result_schema = build_nros_schema_for_struct(
        package_name,
        &result_struct,
        &format!("{}/action/{}_Result", package_name, action_name),
        "RESULT_",
        &action.spec.result.fields,
    );
    let feedback_schema = build_nros_schema_for_struct(
        package_name,
        &feedback_struct,
        &format!("{}/action/{}_Feedback", package_name, action_name),
        "FEEDBACK_",
        &action.spec.feedback.fields,
    );

    let envelopes = build_action_envelope_schemas(package_name, action_name);

    let template = ActionNrosTemplate {
        package_name,
        action_name,
        type_hash,
        goal_fields,
        goal_constants,
        result_fields,
        result_constants,
        feedback_fields,
        feedback_constants,
        has_goal_fields,
        has_result_fields,
        has_feedback_fields,
        has_goal_large_array,
        has_result_large_array,
        has_feedback_large_array,
        inline_mode: true,
        goal_schema_helper_consts: goal_schema.helper_consts,
        goal_schema_fields_block: goal_schema.fields_block,
        goal_schema_type_name: goal_schema.nros_type_name,
        result_schema_helper_consts: result_schema.helper_consts,
        result_schema_fields_block: result_schema.fields_block,
        result_schema_type_name: result_schema.nros_type_name,
        feedback_schema_helper_consts: feedback_schema.helper_consts,
        feedback_schema_fields_block: feedback_schema.fields_block,
        feedback_schema_type_name: feedback_schema.nros_type_name,
        send_goal_request_schema_helper_consts: envelopes.send_goal_request.helper_consts,
        send_goal_request_schema_fields_block: envelopes.send_goal_request.fields_block,
        send_goal_request_schema_type_name: envelopes.send_goal_request.nros_type_name,
        send_goal_response_schema_helper_consts: envelopes.send_goal_response.helper_consts,
        send_goal_response_schema_fields_block: envelopes.send_goal_response.fields_block,
        send_goal_response_schema_type_name: envelopes.send_goal_response.nros_type_name,
        get_result_request_schema_helper_consts: envelopes.get_result_request.helper_consts,
        get_result_request_schema_fields_block: envelopes.get_result_request.fields_block,
        get_result_request_schema_type_name: envelopes.get_result_request.nros_type_name,
        get_result_response_schema_helper_consts: envelopes.get_result_response.helper_consts,
        get_result_response_schema_fields_block: envelopes.get_result_response.fields_block,
        get_result_response_schema_type_name: envelopes.get_result_response.nros_type_name,
        feedback_message_schema_helper_consts: envelopes.feedback_message.helper_consts,
        feedback_message_schema_fields_block: envelopes.feedback_message.fields_block,
        feedback_message_schema_type_name: envelopes.feedback_message.nros_type_name,
    };

    Ok(template.render()?)
}

/// Generated C action package
pub struct GeneratedCActionPackage {
    /// Header file content (.h)
    pub header: String,
    /// Source file content (.c)
    pub source: String,
    /// Header filename
    pub header_name: String,
    /// Source filename
    pub source_name: String,
}

/// Generate C code for an action type
pub fn generate_c_action_package(
    package_name: &str,
    action_name: &str,
    action: &Action,
    type_hash: &str,
    resolver: &CapacityResolver,
) -> Result<GeneratedCActionPackage, GeneratorError> {
    let c_pkg_name = to_c_package_name(package_name);
    let action_snake = to_snake_case(action_name);

    // Build struct and guard names
    let action_struct_name = format!("{}_action_{}", c_pkg_name, action_snake);
    let goal_struct_name = format!("{}_action_{}_goal", c_pkg_name, action_snake);
    let result_struct_name = format!("{}_action_{}_result", c_pkg_name, action_snake);
    let feedback_struct_name = format!("{}_action_{}_feedback", c_pkg_name, action_snake);
    let guard_name = format!(
        "{}_ACTION_{}_H",
        c_pkg_name.to_uppercase(),
        action_snake.to_uppercase()
    );
    let constant_prefix = format!(
        "{}_ACTION_{}",
        c_pkg_name.to_uppercase(),
        action_snake.to_uppercase()
    );
    let header_name = format!("{}_action_{}.h", c_pkg_name, action_snake);
    let source_name = format!("{}_action_{}.c", c_pkg_name, action_snake);

    // Extract dependencies from goal, result, and feedback.
    // Same pattern as msg.rs: per-type `type_includes` with correct paths,
    // plus a flat `dependencies` list for metadata.
    let mut dependencies = Vec::new();
    let mut type_includes = Vec::new();
    for field in action
        .spec
        .goal
        .fields
        .iter()
        .chain(action.spec.result.fields.iter())
        .chain(action.spec.feedback.fields.iter())
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
                // Intra-package: the action .h lives at <pkg>/action/, so the
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

    let goal_msg = format!("{action_name}_Goal");
    let result_msg = format!("{action_name}_Result");
    let feedback_msg = format!("{action_name}_Feedback");

    // Build C fields for goal
    let goal_fields: Vec<CField> = action
        .spec
        .goal
        .fields
        .iter()
        .map(|field| {
            build_c_field(
                &field.name,
                &field.field_type,
                Some(package_name),
                &goal_msg,
                resolver,
            )
        })
        .collect::<Result<_, _>>()?;

    let goal_constants: Vec<CConstant> = action
        .spec
        .goal
        .constants
        .iter()
        .map(|constant| CConstant {
            name: constant.name.clone(),
            c_type: c_type_for_constant(&constant.constant_type),
            value: constant_value_to_rust(&constant.value),
        })
        .collect();

    // Build C fields for result
    let result_fields: Vec<CField> = action
        .spec
        .result
        .fields
        .iter()
        .map(|field| {
            build_c_field(
                &field.name,
                &field.field_type,
                Some(package_name),
                &result_msg,
                resolver,
            )
        })
        .collect::<Result<_, _>>()?;

    let result_constants: Vec<CConstant> = action
        .spec
        .result
        .constants
        .iter()
        .map(|constant| CConstant {
            name: constant.name.clone(),
            c_type: c_type_for_constant(&constant.constant_type),
            value: constant_value_to_rust(&constant.value),
        })
        .collect();

    // Build C fields for feedback
    let feedback_fields: Vec<CField> = action
        .spec
        .feedback
        .fields
        .iter()
        .map(|field| {
            build_c_field(
                &field.name,
                &field.field_type,
                Some(package_name),
                &feedback_msg,
                resolver,
            )
        })
        .collect::<Result<_, _>>()?;

    let feedback_constants: Vec<CConstant> = action
        .spec
        .feedback
        .constants
        .iter()
        .map(|constant| CConstant {
            name: constant.name.clone(),
            c_type: c_type_for_constant(&constant.constant_type),
            value: constant_value_to_rust(&constant.value),
        })
        .collect();

    let has_goal_fields = !goal_fields.is_empty();
    let has_result_fields = !result_fields.is_empty();
    let has_feedback_fields = !feedback_fields.is_empty();

    // Generate header
    let header_template = ActionCHeaderTemplate {
        package_name,
        action_name,
        type_hash,
        guard_name,
        action_struct_name: action_struct_name.clone(),
        goal_struct_name: goal_struct_name.clone(),
        result_struct_name: result_struct_name.clone(),
        feedback_struct_name: feedback_struct_name.clone(),
        constant_prefix,
        goal_fields: goal_fields.clone(),
        goal_constants,
        result_fields: result_fields.clone(),
        result_constants,
        feedback_fields: feedback_fields.clone(),
        feedback_constants,
        dependencies,
        type_includes,
        has_goal_fields,
        has_result_fields,
        has_feedback_fields,
    };
    let header = header_template.render()?;

    // Generate source
    let source_template = ActionCSourceTemplate {
        package_name,
        action_name,
        type_hash,
        header_name: header_name.clone(),
        action_struct_name,
        goal_struct_name,
        result_struct_name,
        feedback_struct_name,
        goal_fields,
        result_fields,
        feedback_fields,
        has_goal_fields,
        has_result_fields,
        has_feedback_fields,
    };
    let source = source_template.render()?;

    Ok(GeneratedCActionPackage {
        header,
        source,
        header_name,
        source_name,
    })
}
