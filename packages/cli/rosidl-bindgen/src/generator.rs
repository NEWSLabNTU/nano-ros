//! Generator integration for generating nros Rust bindings from ROS 2 interface packages.
//!
//! This module integrates with rosidl-codegen to:
//! - Parse interface files (.msg, .srv)
//! - Generate pure Rust, no_std compatible code for messages and services
//! - Write generated code to output directory with proper structure
//!
//! Note: This is the nros fork which generates single-layer pure Rust bindings
//! using heapless types, suitable for embedded systems.

use crate::ament::Package;
use eyre::{Result, WrapErr};
use rosidl_codegen::{
    CapacityResolver, RosEdition, generate_nros_action_package, generate_nros_message_package,
    generate_nros_service_package,
    utils::{extract_dependencies, to_snake_case},
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

/// Idempotent write — skip the rewrite when content matches so the file's
/// mtime doesn't bump on every codegen run (cmake's mtime-driven rebuilds
/// otherwise force cargo to recompile every downstream FFI crate).
fn write_if_changed<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> std::io::Result<()> {
    let path = path.as_ref();
    let new = contents.as_ref();
    if std::fs::read(path).is_ok_and(|existing| existing == new) {
        return Ok(());
    }
    std::fs::write(path, new)
}

/// Generated nros Rust package structure.
///
/// Single-layer architecture with pure Rust, no_std compatible types:
/// - `pkg::msg::Type` - Message types using heapless collections
/// - `pkg::srv::Type` - Service request/response types
/// - `pkg::action::Type` - Action Goal/Result/Feedback types
#[derive(Debug)]
pub struct GeneratedRustPackage {
    /// Package name
    pub name: String,
    /// Output directory where code was written
    pub output_dir: PathBuf,
    /// Number of messages generated
    pub message_count: usize,
    /// Number of services generated
    pub service_count: usize,
    /// Number of actions generated
    pub action_count: usize,
}

/// Generate nros Rust bindings for a ROS 2 package
///
/// This generates pure Rust, no_std compatible bindings using heapless types.
/// Unlike the rclrs backend, this does NOT require ROS 2 C libraries.
pub fn generate_package(
    package: &Package,
    output_dir: &Path,
    edition: RosEdition,
    resolver: &CapacityResolver,
) -> Result<GeneratedRustPackage> {
    let package_output = output_dir.join(&package.name);
    std::fs::create_dir_all(&package_output).wrap_err_with(|| {
        format!(
            "Failed to create output directory: {}",
            package_output.display()
        )
    })?;

    let mut message_count = 0;
    let mut service_count = 0;
    let mut all_dependencies = HashSet::new();

    // Create src/msg directory
    let src_dir = package_output.join("src");
    let msg_dir = src_dir.join("msg");
    std::fs::create_dir_all(&msg_dir)?;

    // Generate messages
    for msg_name in &package.interfaces.messages {
        let msg_path = package.get_message_path(msg_name);
        let content = std::fs::read_to_string(&msg_path)
            .wrap_err_with(|| format!("Failed to read message file: {}", msg_path.display()))?;

        let parsed_msg = rosidl_parser::parse_message(&content)
            .wrap_err_with(|| format!("Failed to parse message: {}", msg_name))?;

        // Extract dependencies
        let msg_deps = extract_dependencies(&parsed_msg);
        all_dependencies.extend(msg_deps);

        let generated = generate_nros_message_package(
            &package.name,
            msg_name,
            &parsed_msg,
            &all_dependencies,
            &package.version,
            edition,
            resolver,
        )
        .wrap_err_with(|| format!("Failed to generate nros message: {}", msg_name))?;

        // Write message file
        let msg_file = msg_dir.join(format!("{}.rs", to_snake_case(msg_name)));
        write_if_changed(&msg_file, &generated.message_rs)?;
        message_count += 1;
    }

    // Create src/srv directory if needed
    if !package.interfaces.services.is_empty() {
        let srv_dir = src_dir.join("srv");
        std::fs::create_dir_all(&srv_dir)?;

        // Generate services
        for srv_name in &package.interfaces.services {
            let srv_path = package.get_service_path(srv_name);
            let content = std::fs::read_to_string(&srv_path)
                .wrap_err_with(|| format!("Failed to read service file: {}", srv_path.display()))?;

            let parsed_srv = rosidl_parser::parse_service(&content)
                .wrap_err_with(|| format!("Failed to parse service: {}", srv_name))?;

            // Extract dependencies from request and response
            let req_deps = extract_dependencies(&parsed_srv.request);
            let resp_deps = extract_dependencies(&parsed_srv.response);
            all_dependencies.extend(req_deps);
            all_dependencies.extend(resp_deps);

            let generated = generate_nros_service_package(
                &package.name,
                srv_name,
                &parsed_srv,
                &all_dependencies,
                &package.version,
                edition,
                resolver,
            )
            .wrap_err_with(|| format!("Failed to generate nros service: {}", srv_name))?;

            // Write service file
            let srv_file = srv_dir.join(format!("{}.rs", to_snake_case(srv_name)));
            write_if_changed(&srv_file, &generated.service_rs)?;
            service_count += 1;
        }
    }

    // Create src/action directory if needed
    let mut action_count = 0;
    if !package.interfaces.actions.is_empty() {
        let action_dir = src_dir.join("action");
        std::fs::create_dir_all(&action_dir)?;

        // Phase 212.K.7.1.d: action envelope structs reference
        // `unique_identifier_msgs::msg::UUID` (every envelope with a
        // `goal_id`) + `builtin_interfaces::msg::Time` (SendGoal_Response
        // `stamp`). Mirror the dep injection in
        // `generate_nros_action_package` so the generated Cargo.toml
        // resolves these `<Pkg::msg::T as Message>::FIELDS` references.
        if package.name != "unique_identifier_msgs" {
            all_dependencies.insert("unique_identifier_msgs".to_string());
        }
        if package.name != "builtin_interfaces" {
            all_dependencies.insert("builtin_interfaces".to_string());
        }
        // Phase 244 E3 (RFC-0044) — the generated `impl RosAction::register_protocol_types`
        // names `action_msgs::srv::CancelGoal_{Request,Response}` + `msg::GoalStatusArray`,
        // so the action crate depends on `action_msgs` (a sibling generated crate;
        // path dep). `action_msgs` itself has no actions → no self-dep.
        if package.name != "action_msgs" {
            all_dependencies.insert("action_msgs".to_string());
        }

        // Generate actions
        for action_name in &package.interfaces.actions {
            let action_path = package.get_action_path(action_name);
            let content = std::fs::read_to_string(&action_path).wrap_err_with(|| {
                format!("Failed to read action file: {}", action_path.display())
            })?;

            let parsed_action = rosidl_parser::parse_action(&content)
                .wrap_err_with(|| format!("Failed to parse action: {}", action_name))?;

            // Extract dependencies from goal, result, and feedback
            let goal_deps = extract_dependencies(&parsed_action.spec.goal);
            let result_deps = extract_dependencies(&parsed_action.spec.result);
            let feedback_deps = extract_dependencies(&parsed_action.spec.feedback);
            all_dependencies.extend(goal_deps);
            all_dependencies.extend(result_deps);
            all_dependencies.extend(feedback_deps);

            let generated = generate_nros_action_package(
                &package.name,
                action_name,
                &parsed_action,
                &all_dependencies,
                &package.version,
                edition,
                resolver,
            )
            .wrap_err_with(|| format!("Failed to generate nros action: {}", action_name))?;

            // Write action file
            let action_file = action_dir.join(format!("{}.rs", to_snake_case(action_name)));
            write_if_changed(&action_file, &generated.action_rs)?;
            action_count += 1;
        }
    }

    // Remove self-dependency
    all_dependencies.remove(&package.name);

    // Generate msg/mod.rs
    generate_msg_mod_rs(&msg_dir, package)?;

    // Generate srv/mod.rs if there are services
    if !package.interfaces.services.is_empty() {
        let srv_dir = src_dir.join("srv");
        generate_srv_mod_rs(&srv_dir, package)?;
    }

    // Generate action/mod.rs if there are actions
    if !package.interfaces.actions.is_empty() {
        let action_dir = src_dir.join("action");
        generate_action_mod_rs(&action_dir, package)?;
    }

    // Generate lib.rs
    generate_lib_rs(&src_dir, package)?;

    // Generate Cargo.toml
    generate_cargo_toml(
        &package_output,
        &package.name,
        &package.version,
        &all_dependencies,
        !package.interfaces.actions.is_empty(),
    )?;

    Ok(GeneratedRustPackage {
        name: package.name.clone(),
        output_dir: package_output,
        message_count,
        service_count,
        action_count,
    })
}

/// Generate msg/mod.rs for nros
fn generate_msg_mod_rs(msg_dir: &Path, package: &Package) -> Result<()> {
    let mut content = String::new();
    content.push_str("//! Message types for this package\n\n");

    for msg_name in &package.interfaces.messages {
        let module_name = to_snake_case(msg_name);
        content.push_str(&format!("mod {};\n", module_name));
        content.push_str(&format!("pub use {}::{};\n\n", module_name, msg_name));
    }

    write_if_changed(msg_dir.join("mod.rs"), content)?;
    Ok(())
}

/// Generate srv/mod.rs for nros
fn generate_srv_mod_rs(srv_dir: &Path, package: &Package) -> Result<()> {
    let mut content = String::new();
    content.push_str("//! Service types for this package\n\n");

    for srv_name in &package.interfaces.services {
        let module_name = to_snake_case(srv_name);
        content.push_str(&format!("mod {};\n", module_name));
        // Export the service struct, request, and response types
        content.push_str(&format!(
            "pub use {}::{{{}, {}Request, {}Response}};\n\n",
            module_name, srv_name, srv_name, srv_name
        ));
    }

    write_if_changed(srv_dir.join("mod.rs"), content)?;
    Ok(())
}

/// Generate action/mod.rs for nros
fn generate_action_mod_rs(action_dir: &Path, package: &Package) -> Result<()> {
    let mut content = String::new();
    content.push_str("//! Action types for this package\n\n");

    for action_name in &package.interfaces.actions {
        let module_name = to_snake_case(action_name);
        content.push_str(&format!("mod {};\n", module_name));
        // Export the action struct and message types
        content.push_str(&format!(
            "pub use {}::{{{}, {}Goal, {}Result, {}Feedback}};\n\n",
            module_name, action_name, action_name, action_name, action_name
        ));
    }

    write_if_changed(action_dir.join("mod.rs"), content)?;
    Ok(())
}

/// Generate lib.rs for nros
fn generate_lib_rs(src_dir: &Path, package: &Package) -> Result<()> {
    let mut content = String::new();
    content.push_str("//! Generated nros bindings\n");
    content.push_str("//!\n");
    content.push_str("//! This crate is `no_std` compatible.\n\n");
    content.push_str("#![no_std]\n");
    content.push_str("#![allow(dead_code)]\n\n");

    if !package.interfaces.messages.is_empty() {
        content.push_str("pub mod msg;\n");
    }
    if !package.interfaces.services.is_empty() {
        content.push_str("pub mod srv;\n");
    }
    if !package.interfaces.actions.is_empty() {
        content.push_str("pub mod action;\n");
    }

    write_if_changed(src_dir.join("lib.rs"), content)?;
    Ok(())
}

/// Generate Cargo.toml for nros
fn generate_cargo_toml(
    output_dir: &Path,
    package_name: &str,
    package_version: &str,
    dependencies: &HashSet<String>,
    has_actions: bool,
) -> Result<()> {
    // Build std feature list including all dependencies
    let mut std_features = vec![
        "\"nros-core/std\"".to_string(),
        "\"nros-serdes/std\"".to_string(),
    ];
    for dep in dependencies {
        let crate_name = dep.replace('-', "_");
        std_features.push(format!("\"{}/std\"", crate_name));
    }
    let std_feature_list = std_features.join(", ");

    // Use crates.io version specifiers for nros crates.
    // For development, use .cargo/config.toml [patch.crates-io] to point to local paths.
    let mut cargo_toml = format!(
        r#"[package]
name = "{}"
version = "{}"
edition = "2021"

[features]
default = []
std = [{std_features}]

[dependencies]
# nros crates (patched to local via .cargo/config.toml during development)
nros-core = {{ version = "*", default-features = false }}
nros-serdes = {{ version = "*", default-features = false }}
heapless = "0.8"
"#,
        package_name,
        package_version,
        std_features = std_feature_list,
    );

    // issue #234 — action packages register their fixed `action_msgs` protocol
    // types (CancelGoal_{Request,Response}, GoalStatusArray) in
    // `RosAction::register_protocol_types` through the generic
    // `nros_rmw::register_type_descriptor` seam. That seam is a no-op unless a
    // descriptor-needing backend (Cyclone DDS) installs a registrar, so the dep
    // is unconditional and unfeatured — no named-backend dep and no cfg gate
    // (issue #60). The pre-#234 `rmw-cyclonedds`-feature-gated
    // `nros_rmw_cyclonedds::register::<M>()` path compiled out whenever the
    // consumer did not also turn on this crate's `rmw-cyclonedds` feature (the
    // standard example build never did), leaving the CancelGoal / GoalStatusArray
    // descriptors unregistered → `ActionCreationFailed`. `nros-rmw` is resolved
    // via the workspace `[patch.crates-io]` that `nros sync` writes (see
    // `nros_crate_path_lookup` — `nros-rmw` → `packages/core/nros-rmw`).
    if has_actions {
        cargo_toml.push_str("nros-rmw = { version = \"*\", default-features = false }\n");
    }

    // Add cross-package dependencies
    for dep in dependencies {
        let crate_name = dep.replace('-', "_");
        cargo_toml.push_str(&format!(
            "{} = {{ path = \"../{}\", default-features = false }}\n",
            crate_name, dep
        ));
    }

    write_if_changed(output_dir.join("Cargo.toml"), cargo_toml)?;
    Ok(())
}

/// Phase 233.1 (RFC-0039 Track B) — generate CDR-serializable `px4_msgs::msg::*`
/// from the PX4 `.msg` tree (`<px4>/msg/` + `<px4>/msg/versioned/`), with no
/// ament `package.xml`.
///
/// PX4 1.16+ moved the versioned core ROS 2 interface topics into
/// `msg/versioned/`; both directories are staged into one flat `msg/` (versioned
/// shadows a same-named base — it is the canonical definition) so the standard
/// ament-driven [`generate_package`] can emit the complete `px4_msgs` crate. The
/// generated types carry `TYPE_NAME = "px4_msgs::msg::dds_::<Name>_"`, which is
/// what the Micro XRCE-DDS Agent matches against PX4's `/fmu/*` endpoints.
pub fn generate_px4_msgs(
    px4_dir: &Path,
    output_dir: &Path,
    version: &str,
    edition: RosEdition,
    resolver: &CapacityResolver,
) -> Result<GeneratedRustPackage> {
    use crate::ament::InterfaceFiles;

    // Stage `msg/` + `msg/versioned/` into one flat `msg/` dir (versioned copied
    // last so it shadows a same-named base entry).
    let stage = output_dir.join(".px4_msg_stage");
    let stage_msg = stage.join("msg");
    std::fs::create_dir_all(&stage_msg)
        .wrap_err_with(|| format!("create staging dir {}", stage_msg.display()))?;

    let mut names: Vec<String> = Vec::new();
    for sub in ["msg", "msg/versioned"] {
        let dir = px4_dir.join(sub);
        if !dir.is_dir() {
            continue;
        }
        for entry in
            std::fs::read_dir(&dir).wrap_err_with(|| format!("readdir {}", dir.display()))?
        {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("msg") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
                continue;
            };
            std::fs::copy(&path, stage_msg.join(format!("{stem}.msg")))
                .wrap_err_with(|| format!("stage {}", path.display()))?;
            if !names.contains(&stem) {
                names.push(stem);
            }
        }
    }
    if names.is_empty() {
        let _ = std::fs::remove_dir_all(&stage);
        eyre::bail!(
            "{}: no `.msg` files under `msg/` or `msg/versioned/` (is this a PX4-Autopilot tree?)",
            px4_dir.display()
        );
    }
    names.sort();

    // Synthetic ament package — `share_dir/msg/<name>.msg` is exactly what the
    // staging layout provides, so `generate_package` resolves every msg.
    let package = Package {
        name: "px4_msgs".to_string(),
        version: version.to_string(),
        share_dir: stage.clone(),
        interfaces: InterfaceFiles {
            messages: names,
            services: Vec::new(),
            actions: Vec::new(),
            idl_messages: Vec::new(),
            idl_services: Vec::new(),
            idl_actions: Vec::new(),
        },
    };

    let result = generate_package(&package, output_dir, edition, resolver);
    let _ = std::fs::remove_dir_all(&stage);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ament::Package;
    use std::fs;

    /// Helper to create a test package with interface files
    fn create_test_package(temp_dir: &Path) -> Package {
        let share_dir = temp_dir.join("test_pkg");

        // Create msg files
        let msg_dir = share_dir.join("msg");
        fs::create_dir_all(&msg_dir).unwrap();
        write_if_changed(msg_dir.join("Point.msg"), "float64 x\nfloat64 y\n").unwrap();

        // Create srv files
        let srv_dir = share_dir.join("srv");
        fs::create_dir_all(&srv_dir).unwrap();
        write_if_changed(
            srv_dir.join("AddTwoInts.srv"),
            "int64 a\nint64 b\n---\nint64 sum\n",
        )
        .unwrap();

        Package::from_share_dir(share_dir).unwrap()
    }

    #[test]
    fn test_generate_nros_package() {
        let temp_dir = tempfile::tempdir().unwrap();
        let package = create_test_package(temp_dir.path());
        let output_dir = temp_dir.path().join("output");

        let result = generate_package(
            &package,
            &output_dir,
            RosEdition::Humble,
            &CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.message_count, 1);
        assert_eq!(generated.service_count, 1);
        assert_eq!(generated.action_count, 0);

        // Check that files were created
        let pkg_dir = output_dir.join("test_pkg");
        assert!(pkg_dir.join("Cargo.toml").exists());
        assert!(pkg_dir.join("src").join("lib.rs").exists());
        assert!(pkg_dir.join("src").join("msg").join("mod.rs").exists());
        assert!(pkg_dir.join("src").join("msg").join("point.rs").exists());
        assert!(pkg_dir.join("src").join("srv").join("mod.rs").exists());
        assert!(
            pkg_dir
                .join("src")
                .join("srv")
                .join("add_two_ints.rs")
                .exists()
        );

        // Check there's no build.rs (no C library linking)
        assert!(!pkg_dir.join("build.rs").exists());
    }

    #[test]
    fn test_cargo_toml_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let share_dir = temp_dir.path().join("nano_msgs");

        // Create message file
        let msg_dir = share_dir.join("msg");
        fs::create_dir_all(&msg_dir).unwrap();
        write_if_changed(msg_dir.join("Point.msg"), "float64 x\nfloat64 y\n").unwrap();

        // Create package.xml with specific version
        let package_xml = r#"<?xml version="1.0"?>
<package format="3">
  <name>nano_msgs</name>
  <version>1.0.0</version>
  <description>Test nros messages</description>
</package>
"#;
        write_if_changed(share_dir.join("package.xml"), package_xml).unwrap();

        let package = Package::from_share_dir(share_dir).unwrap();
        let output_dir = temp_dir.path().join("output");

        let result = generate_package(
            &package,
            &output_dir,
            RosEdition::Humble,
            &CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        // Check Cargo.toml content
        let cargo_toml =
            fs::read_to_string(output_dir.join("nano_msgs").join("Cargo.toml")).unwrap();
        assert!(cargo_toml.contains("name = \"nano_msgs\""));
        assert!(cargo_toml.contains("version = \"1.0.0\""));
        assert!(cargo_toml.contains("nros-core"));
        assert!(cargo_toml.contains("nros-serdes"));
        assert!(cargo_toml.contains("heapless"));
        // Should NOT contain rclrs dependencies
        assert!(!cargo_toml.contains("rosidl_runtime_rs"));
        // Should NOT have standalone workspace declaration (to avoid conflicts)
        assert!(!cargo_toml.contains("[workspace]"));
        // Phase 212.K.7.1 — generated msg crates are RMW-agnostic.
        // No `cyclonedds` Cargo feature, no `cyclonedds-sys` dep, no
        // `<other>/cyclonedds` feature ref, no `links = "*_cyclonedds_*"`.
        assert!(
            !cargo_toml.contains("cyclonedds"),
            "generated Cargo.toml leaked a cyclonedds reference (msg crates \
             must be RMW-agnostic — see Phase 212.K.7.1):\n{cargo_toml}"
        );
    }

    #[test]
    fn test_lib_rs_is_no_std() {
        let temp_dir = tempfile::tempdir().unwrap();
        let package = create_test_package(temp_dir.path());
        let output_dir = temp_dir.path().join("output");

        generate_package(
            &package,
            &output_dir,
            RosEdition::Humble,
            &CapacityResolver::empty(),
        )
        .unwrap();

        // Check lib.rs is no_std
        let lib_rs =
            fs::read_to_string(output_dir.join("test_pkg").join("src").join("lib.rs")).unwrap();
        assert!(lib_rs.contains("#![no_std]"));
        assert!(lib_rs.contains("pub mod msg"));
        assert!(lib_rs.contains("pub mod srv"));
    }

    #[test]
    fn test_messages_only_package() {
        let temp_dir = tempfile::tempdir().unwrap();
        let share_dir = temp_dir.path().join("msgs_only");

        // Create only message files (no services)
        let msg_dir = share_dir.join("msg");
        fs::create_dir_all(&msg_dir).unwrap();
        write_if_changed(msg_dir.join("Int32.msg"), "int32 data\n").unwrap();

        let package = Package::from_share_dir(share_dir).unwrap();
        let output_dir = temp_dir.path().join("output");

        let result = generate_package(
            &package,
            &output_dir,
            RosEdition::Humble,
            &CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.message_count, 1);
        assert_eq!(generated.service_count, 0);

        // Check lib.rs has only msg module
        let lib_rs =
            fs::read_to_string(output_dir.join("msgs_only").join("src").join("lib.rs")).unwrap();
        assert!(lib_rs.contains("pub mod msg"));
        assert!(!lib_rs.contains("pub mod srv"));

        // Check srv directory doesn't exist
        assert!(
            !output_dir
                .join("msgs_only")
                .join("src")
                .join("srv")
                .exists()
        );
    }
}
