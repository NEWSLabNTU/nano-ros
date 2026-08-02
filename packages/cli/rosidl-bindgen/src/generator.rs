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
    rihs::{build_type_description, rihs01},
    utils::{extract_dependencies, to_snake_case},
};
use rosidl_parser::Message;
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

/// Resolve a `pkg/msg/Name` fully-qualified type name to its parsed [`Message`].
///
/// The RIHS01 type hash (REP-2011) is computed over the *closed* type
/// description DAG — every nested type must be loadable. `generate_package`
/// resolves same-package nested types itself (it owns the package's
/// `share_dir`); this callback covers cross-package references (typically
/// `std_msgs` / `builtin_interfaces` from the ament index).
pub type MsgResolver<'a> = dyn Fn(&str) -> Option<Message> + 'a;

/// A [`MsgResolver`] that resolves no cross-package types — for self-contained
/// packages (every nested type is same-package, handled internally) or Humble
/// (placeholder hash, resolver never consulted).
pub fn no_cross_pkg_resolver(_fqn: &str) -> Option<Message> {
    None
}

/// Compute the `TYPE_HASH` string emitted on a generated message.
///
/// Humble predates REP-2011 → the `TypeHashNotSupported` placeholder.
/// Iron+ compute the real `RIHS01_<hash>` over the canonical type
/// description. A nested type that cannot be resolved is a HARD error — we
/// never emit a plausible-but-wrong hash (a wrong hash silently breaks
/// discovery on the wire).
fn compute_msg_type_hash(
    edition: RosEdition,
    fqn: &str,
    message: &Message,
    resolve: &MsgResolver<'_>,
) -> Result<String> {
    if !edition.uses_type_hash() {
        return Ok(edition.type_hash().to_string());
    }
    let desc = build_type_description(fqn, message, |f| resolve(f)).map_err(|e| {
        eyre::eyre!("RIHS01 type-hash computation failed for {fqn} ({edition:?}): {e}")
    })?;
    Ok(rihs01(&desc))
}

/// Compute the three REP-2011 hashes a service emits (`_Request`, `_Response`,
/// and the SERVICE itself). Humble → the placeholder for all three. Iron+ build
/// the synthesized `_Event` DAG (§3a). A nested type that cannot be resolved is
/// a HARD error — never a wrong hash.
fn compute_service_type_hashes(
    edition: RosEdition,
    package: &str,
    srv_name: &str,
    service: &rosidl_parser::Service,
    resolve: &MsgResolver<'_>,
) -> Result<(String, String, String)> {
    if !edition.uses_type_hash() {
        let p = edition.type_hash().to_string();
        return Ok((p.clone(), p.clone(), p));
    }
    let r = |f: &str| resolve(f);
    let req = rosidl_codegen::rihs::service_member_type_description(
        package,
        srv_name,
        "_Request",
        &service.request,
        r,
    )
    .map_err(|e| eyre::eyre!("RIHS01 {package}/srv/{srv_name}_Request ({edition:?}): {e}"))?;
    let resp = rosidl_codegen::rihs::service_member_type_description(
        package,
        srv_name,
        "_Response",
        &service.response,
        r,
    )
    .map_err(|e| eyre::eyre!("RIHS01 {package}/srv/{srv_name}_Response ({edition:?}): {e}"))?;
    let svc = rosidl_codegen::rihs::build_service_type_description(
        package,
        srv_name,
        &service.request,
        &service.response,
        r,
    )
    .map_err(|e| eyre::eyre!("RIHS01 {package}/srv/{srv_name} ({edition:?}): {e}"))?;
    Ok((rihs01(&req), rihs01(&resp), rihs01(&svc)))
}

/// Compute the nine REP-2011 hashes an action emits. Humble → the placeholder
/// for all nine; Iron+ synthesize the full action DAG (§3b). Unresolvable nested
/// user type → HARD error.
fn compute_action_type_hashes(
    edition: RosEdition,
    package: &str,
    action_name: &str,
    action: &rosidl_parser::Action,
    resolve: &MsgResolver<'_>,
) -> Result<rosidl_codegen::rihs::ActionTypeHashes> {
    if !edition.uses_type_hash() {
        let p = edition.type_hash().to_string();
        return Ok(rosidl_codegen::rihs::ActionTypeHashes {
            goal: p.clone(),
            result: p.clone(),
            feedback: p.clone(),
            send_goal_request: p.clone(),
            send_goal_response: p.clone(),
            get_result_request: p.clone(),
            get_result_response: p.clone(),
            feedback_message: p.clone(),
            action: p.clone(),
            send_goal_service: p.clone(),
            get_result_service: p,
        });
    }
    rosidl_codegen::rihs::action_type_hashes(
        package,
        action_name,
        &action.spec.goal,
        &action.spec.result,
        &action.spec.feedback,
        |f| resolve(f),
    )
    .map_err(|e| eyre::eyre!("RIHS01 {package}/action/{action_name} ({edition:?}): {e}"))
}

/// Generate nros Rust bindings for a ROS 2 package
///
/// This generates pure Rust, no_std compatible bindings using heapless types.
/// Unlike the rclrs backend, this does NOT require ROS 2 C libraries.
///
/// `msg_resolve` loads cross-package nested `.msg` types for the REP-2011 type
/// hash (Iron+); same-package nested types are resolved internally from
/// `package.share_dir`. Humble ignores it (placeholder hash).
pub fn generate_package(
    package: &Package,
    output_dir: &Path,
    edition: RosEdition,
    resolver: &CapacityResolver,
    msg_resolve: &MsgResolver<'_>,
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

    // Compose the type-hash resolver: same-package nested types come from this
    // package's own `share_dir` (loaded + parsed on demand); everything else
    // delegates to the caller-supplied cross-package resolver.
    let self_resolve = |fqn: &str| -> Option<Message> {
        let mut parts = fqn.split('/');
        let pkg = parts.next()?;
        let name = parts.next_back()?;
        if pkg == package.name {
            let content = std::fs::read_to_string(package.get_message_path(name)).ok()?;
            rosidl_parser::parse_message(&content).ok()
        } else {
            msg_resolve(fqn)
        }
    };

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

        let fqn = format!("{}/msg/{}", package.name, msg_name);
        let type_hash = compute_msg_type_hash(edition, &fqn, &parsed_msg, &self_resolve)?;

        let generated = generate_nros_message_package(
            &package.name,
            msg_name,
            &parsed_msg,
            &all_dependencies,
            &package.version,
            &type_hash,
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

            let (request_type_hash, response_type_hash, service_hash) =
                compute_service_type_hashes(
                    edition,
                    &package.name,
                    srv_name,
                    &parsed_srv,
                    &self_resolve,
                )?;

            let generated = generate_nros_service_package(
                &package.name,
                srv_name,
                &parsed_srv,
                &all_dependencies,
                &package.version,
                &request_type_hash,
                &response_type_hash,
                &service_hash,
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

            let action_hashes = compute_action_type_hashes(
                edition,
                &package.name,
                action_name,
                &parsed_action,
                &self_resolve,
            )?;

            let generated = generate_nros_action_package(
                &package.name,
                action_name,
                &parsed_action,
                &all_dependencies,
                &package.version,
                &action_hashes,
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
/// How a generated crate should reference an nros runtime crate.
///
/// RFC-0067 Q1 — folding `nros-core` / `nros-serdes` into D1. They used to be
/// registry names (`version = "*"`) rescued by `[patch.crates-io]`, which has
/// the same two defects the message crates had:
///
///   * cargo loads `.cargo/config.toml` by walking up from the CURRENT
///     DIRECTORY, so `cargo --manifest-path <leaf>` from the repo root never
///     loaded the patch and resolution failed `no matching package named
///     nros-core` (measured on a config-patched leaf after phase-333 W1 fixed
///     the message half);
///   * nano-ros publishes nothing to crates.io, so an unpatched resolution is
///     a bare name in a registry namespace nano-ros does not own.
///
/// A path dep has neither. The asymmetry with message crates is that these live
/// in the CHECKOUT rather than beside the generated crate, so the emitted path
/// must be:
///
///   * RELATIVE when the generated tree is inside the checkout — a committed
///     `generated/` tree must not carry a host-specific absolute path (that is
///     the issue-0375 / 0391 class this same phase just removed);
///   * ABSOLUTE for a copy-out project outside the checkout, where no stable
///     relative path exists. That content is regenerated per host by the user's
///     own `nros sync`, exactly like the central `nros-patch.toml` it replaces,
///     so a host path is correct there.
///
/// With no `NROS_REPO_DIR` (codegen invoked outside a workspace), fall back to
/// the registry form so behaviour is unchanged rather than emitting a path that
/// cannot resolve.
fn nros_dep_line(crate_name: &str, package_output: &Path) -> String {
    let registry = format!(
        r#"{crate_name} = {{ version = "*", default-features = false }}"#
    );
    let Some(root) = std::env::var_os("NROS_REPO_DIR").map(PathBuf::from) else {
        return registry;
    };
    // Subpath per crate: most live under packages/core, the RMW backends do not.
    // Mirrors `nros_crate_path_lookup` on the sync side; kept small deliberately,
    // since only the crates a GENERATED manifest can name need an entry.
    let subpath = match crate_name {
        "nros-core" | "nros-serdes" | "nros-rmw" => format!("packages/core/{crate_name}"),
        "nros-rmw-cyclonedds" => "packages/rmw/cyclonedds/nros-rmw-cyclonedds".to_string(),
        _ => format!("packages/core/{crate_name}"),
    };
    let target = root.join(subpath);
    if !target.is_dir() {
        return registry;
    }
    let out_abs = package_output
        .canonicalize()
        .unwrap_or_else(|_| package_output.to_path_buf());
    let spec = if out_abs.starts_with(&root) {
        match pathdiff::diff_paths(&target, &out_abs) {
            Some(rel) => rel.display().to_string(),
            None => target.display().to_string(),
        }
    } else {
        target.display().to_string()
    };
    format!(r#"{crate_name} = {{ path = "{spec}", default-features = false }}"#)
}

fn generate_cargo_toml(
    output_dir: &Path,
    package_name: &str,
    ament_version: &str,
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
    // issue 0391 — the crate version is a CONSTANT, and the real (ament) version
    // is recorded as metadata instead.
    //
    // A generated msg crate is produced per host from whatever interface source
    // that host has: `/opt/ros/<distro>` where ROS 2 is installed, the vendored
    // `packages/cli/interfaces/` where it is not. Those disagree — humble ships
    // action_msgs 1.2.2, the vendored copy is 1.2.3 — so writing the source
    // version here put the GENERATOR'S ENVIRONMENT into every consumer's
    // Cargo.lock. Tracked leaf locks then flip whenever a contributor with a
    // different interface source rebuilds, which `--locked` (issues 0359/0378)
    // turns into a hard build failure for everyone else. The executor-fairness
    // lock had already been refreshed, reverted and refreshed again over it.
    //
    // Consumers depend on these crates by PATH through `[patch.crates-io]` and
    // declare `version = "*"`, so the constant costs nothing at resolution time.
    let mut cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.0.0"
edition = "2021"

# Version of the interface package this was generated FROM. Informational: it
# varies by host (ROS install vs vendored interfaces) and must never reach the
# `version` field above, or it lands in consumers' lockfiles.
[package.metadata.nros]
ament_version = "{}"

[features]
default = []
std = [{std_features}]

[dependencies]
{nros_core_dep}
{nros_serdes_dep}
heapless = "0.8"
"#,
        package_name,
        ament_version,
        std_features = std_feature_list,
        nros_core_dep = nros_dep_line("nros-core", output_dir),
        nros_serdes_dep = nros_dep_line("nros-serdes", output_dir),
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
        cargo_toml.push_str(&format!("{}\n", nros_dep_line("nros-rmw", output_dir)));
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

    // px4_msgs is self-contained (all nested types are same-package), so the
    // internal same-package resolver covers the full DAG.
    let result = generate_package(
        &package,
        output_dir,
        edition,
        resolver,
        &no_cross_pkg_resolver,
    );
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
            &no_cross_pkg_resolver,
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

    // REP-2011 TYPE_HASH wiring (phase-304 W1b c). Reference values captured
    // live from Jazzy → packages/testing/nros-tests/fixtures/ros-editions/jazzy/.
    const JAZZY_INT32_HASH: &str =
        "RIHS01_b6578ded3c58c626cfe8d1a6fb6e04f706f97e9f03d2727c9ff4e74b1cef0deb";
    const JAZZY_HEADER_HASH: &str =
        "RIHS01_f49fb3ae2cf070f793645ff749683ac6b06203e41c891e17701b1cb597ce6a01";

    fn gen_one_msg(edition: RosEdition, resolve: &MsgResolver<'_>) -> String {
        let temp = tempfile::tempdir().unwrap();
        let share = temp.path().join("std_msgs");
        let msg_dir = share.join("msg");
        fs::create_dir_all(&msg_dir).unwrap();
        write_if_changed(msg_dir.join("Int32.msg"), "int32 data\n").unwrap();
        let package = Package::from_share_dir(share).unwrap();
        let out = temp.path().join("out");
        generate_package(&package, &out, edition, &CapacityResolver::empty(), resolve).unwrap();
        fs::read_to_string(
            out.join("std_msgs")
                .join("src")
                .join("msg")
                .join("int32.rs"),
        )
        .unwrap()
    }

    #[test]
    fn humble_emits_placeholder_type_hash() {
        let rs = gen_one_msg(RosEdition::Humble, &no_cross_pkg_resolver);
        assert!(
            rs.contains("const TYPE_HASH: &'static str = \"TypeHashNotSupported\""),
            "Humble predates REP-2011 — expected the placeholder, got:\n{rs}"
        );
    }

    #[test]
    fn jazzy_emits_real_rihs01_hash_for_flat_message() {
        // Int32 is self-contained → the internal same-package resolver closes
        // the DAG; no cross-package resolver needed.
        let rs = gen_one_msg(RosEdition::Jazzy, &no_cross_pkg_resolver);
        assert!(
            rs.contains(&format!(
                "const TYPE_HASH: &'static str = \"{JAZZY_INT32_HASH}\""
            )),
            "Jazzy Int32 must carry the real captured RIHS01 hash, got:\n{rs}"
        );
    }

    #[test]
    fn jazzy_emits_real_rihs01_hash_for_nested_message() {
        // std_msgs/Header references builtin_interfaces/msg/Time — a
        // CROSS-package nested type the caller-supplied resolver must provide.
        let resolve = |fqn: &str| -> Option<Message> {
            match fqn {
                "builtin_interfaces/msg/Time" => {
                    rosidl_parser::parse_message("int32 sec\nuint32 nanosec\n").ok()
                }
                _ => None,
            }
        };
        let temp = tempfile::tempdir().unwrap();
        let share = temp.path().join("std_msgs");
        let msg_dir = share.join("msg");
        fs::create_dir_all(&msg_dir).unwrap();
        write_if_changed(
            msg_dir.join("Header.msg"),
            "builtin_interfaces/Time stamp\nstring frame_id\n",
        )
        .unwrap();
        let package = Package::from_share_dir(share).unwrap();
        let out = temp.path().join("out");
        generate_package(
            &package,
            &out,
            RosEdition::Jazzy,
            &CapacityResolver::empty(),
            &resolve,
        )
        .unwrap();
        let rs = fs::read_to_string(
            out.join("std_msgs")
                .join("src")
                .join("msg")
                .join("header.rs"),
        )
        .unwrap();
        assert!(
            rs.contains(&format!(
                "const TYPE_HASH: &'static str = \"{JAZZY_HEADER_HASH}\""
            )),
            "Jazzy Header (nested Time) must carry the real captured RIHS01 hash, got:\n{rs}"
        );
    }

    #[test]
    fn jazzy_service_emits_real_rihs01_hashes() {
        // std_srvs/srv/SetBool — Request=bool, Response=bool+string. The emitted
        // Request/Response TYPE_HASH + the SERVICE_HASH must be the three distinct
        // live-Jazzy values (the _Event DAG is synthesized internally).
        let temp = tempfile::tempdir().unwrap();
        let share = temp.path().join("std_srvs");
        let srv_dir = share.join("srv");
        fs::create_dir_all(&srv_dir).unwrap();
        write_if_changed(
            srv_dir.join("SetBool.srv"),
            "bool data\n---\nbool success\nstring message\n",
        )
        .unwrap();
        let package = Package::from_share_dir(share).unwrap();
        let out = temp.path().join("out");
        generate_package(
            &package,
            &out,
            RosEdition::Jazzy,
            &CapacityResolver::empty(),
            &no_cross_pkg_resolver,
        )
        .unwrap();
        let rs = fs::read_to_string(
            out.join("std_srvs")
                .join("src")
                .join("srv")
                .join("set_bool.rs"),
        )
        .unwrap();
        // Request TYPE_HASH, Response TYPE_HASH, SERVICE_HASH — all three present.
        for h in [
            "RIHS01_c62fbb99d94e1b25e8ef9e109f9581956bb1b3361a45a4e5810c36a90d29932e", // _Request
            "RIHS01_d0814e7f7b4880ab77e9c57426c7aa1562ab69f11eef8e2e968812f9cbd0b059", // _Response
            "RIHS01_abe9e4bb6b41b40e6789712c00ec8871923e089af3f667a79992a428cff2da0a", // service
        ] {
            assert!(
                rs.contains(h),
                "generated SetBool must carry {h}, got:\n{rs}"
            );
        }
    }

    #[test]
    fn jazzy_action_emits_nine_real_rihs01_hashes() {
        // Fibonacci is self-contained (goal/result/feedback are primitives) so
        // every nested type is an embedded built-in — no cross-pkg resolver.
        let src = "int32 order\n---\nint32[] sequence\n---\nint32[] partial_sequence\n";
        let temp = tempfile::tempdir().unwrap();
        let share = temp.path().join("example_interfaces");
        let action_dir = share.join("action");
        fs::create_dir_all(&action_dir).unwrap();
        write_if_changed(action_dir.join("Fibonacci.action"), src).unwrap();
        let package = Package::from_share_dir(share).unwrap();
        let out = temp.path().join("out");
        generate_package(
            &package,
            &out,
            RosEdition::Jazzy,
            &CapacityResolver::empty(),
            &no_cross_pkg_resolver,
        )
        .unwrap();
        let rs = fs::read_to_string(
            out.join("example_interfaces")
                .join("src")
                .join("action")
                .join("fibonacci.rs"),
        )
        .unwrap();

        // Expected hashes straight from the engine (ties codegen wiring to it).
        let parsed = rosidl_parser::parse_action(src).unwrap();
        let h = rosidl_codegen::rihs::action_type_hashes(
            "example_interfaces",
            "Fibonacci",
            &parsed.spec.goal,
            &parsed.spec.result,
            &parsed.spec.feedback,
            |_| None,
        )
        .unwrap();
        for (label, hash) in [
            ("goal", &h.goal),
            ("result", &h.result),
            ("feedback", &h.feedback),
            ("send_goal_request", &h.send_goal_request),
            ("send_goal_response", &h.send_goal_response),
            ("get_result_request", &h.get_result_request),
            ("get_result_response", &h.get_result_response),
            ("feedback_message", &h.feedback_message),
            ("action", &h.action),
        ] {
            assert!(
                rs.contains(hash.as_str()),
                "{label} hash {hash} missing:\n{rs}"
            );
        }
        assert!(
            !rs.contains("TypeHashNotSupported"),
            "Jazzy action must not emit the Humble placeholder:\n{rs}"
        );
    }

    #[test]
    fn jazzy_unresolvable_nested_type_fails_loud() {
        // Header needs builtin_interfaces/msg/Time; with no resolver the hash
        // cannot be computed — must ERROR, never emit a wrong/placeholder hash.
        let temp = tempfile::tempdir().unwrap();
        let share = temp.path().join("std_msgs");
        let msg_dir = share.join("msg");
        fs::create_dir_all(&msg_dir).unwrap();
        write_if_changed(
            msg_dir.join("Header.msg"),
            "builtin_interfaces/Time stamp\nstring frame_id\n",
        )
        .unwrap();
        let package = Package::from_share_dir(share).unwrap();
        let out = temp.path().join("out");
        let err = generate_package(
            &package,
            &out,
            RosEdition::Jazzy,
            &CapacityResolver::empty(),
            &no_cross_pkg_resolver,
        )
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("RIHS01") && msg.contains("builtin_interfaces/msg/Time"),
            "expected a loud unresolved-nested-type error, got: {msg}"
        );
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
            &no_cross_pkg_resolver,
        );
        assert!(result.is_ok());

        // Check Cargo.toml content
        let cargo_toml =
            fs::read_to_string(output_dir.join("nano_msgs").join("Cargo.toml")).unwrap();
        assert!(cargo_toml.contains("name = \"nano_msgs\""));
        // issue 0391 — the crate version is the CONSTANT, and the package.xml
        // version survives only as metadata. Asserting both directions matters:
        // a regression that puts the ament version back in `version` is exactly
        // what put the generator's environment into consumers' lockfiles.
        assert!(
            cargo_toml.contains("version = \"0.0.0\""),
            "generated crate must carry the constant version, got:\n{cargo_toml}"
        );
        assert!(
            cargo_toml.contains("ament_version = \"1.0.0\""),
            "package.xml version must survive as [package.metadata.nros] ament_version, got:\n{cargo_toml}"
        );
        assert!(
            !cargo_toml.contains("\nversion = \"1.0.0\""),
            "the ament version must NOT be the crate version, got:\n{cargo_toml}"
        );
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
            &no_cross_pkg_resolver,
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
            &no_cross_pkg_resolver,
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
