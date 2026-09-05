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
use eyre::{Result, WrapErr, eyre};
use rosidl_codegen::{
    CapacityResolver, RosEdition, generate_nros_action_package, generate_nros_message_package,
    generate_nros_service_package,
    utils::{extract_dependencies, to_snake_case},
};
use rosidl_parser::Message;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

/// Idempotent, ATOMIC write.
///
/// Idempotent: skip the rewrite when content matches, so the file's mtime
/// doesn't bump on every codegen run (cmake's mtime-driven rebuilds otherwise
/// force cargo to recompile every downstream FFI crate).
///
/// Atomic: issue 0920. This used to end in `std::fs::write`, which TRUNCATES
/// the target and then writes it. Interrupt codegen — or the build driving it —
/// between those two steps and the file is left at zero bytes. It is a
/// perfectly valid file, so nothing notices until something tries to compile
/// it, and then the error is a RELATIVE path from inside a jobserver fan-out
/// with no leaf name anywhere near it:
///
/// ```text
/// error[E0432]: unresolved import `goal_info::GoalInfo`
///  --> generated/action_msgs/src/msg/mod.rs:4:9
/// ```
///
/// It self-heals on the next codegen (empty != new, so it rewrites), which is
/// why it went unfiled for so long — but it survives long enough to red-line a
/// lane, and locating the one bad file among 200-odd generated trees is a
/// scripted sweep. It cost two lanes in a single session before it was fixed.
///
/// Writing a sibling temporary and renaming fixes it: `rename(2)` is atomic
/// within a filesystem, so an interrupted run leaves either the old file or the
/// new one, never a truncated one. The temp file is a sibling, not a `/tmp`
/// entry, so the rename cannot cross a filesystem boundary.
fn write_if_changed<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> std::io::Result<()> {
    let path = path.as_ref();
    let new = contents.as_ref();
    if std::fs::read(path).is_ok_and(|existing| existing == new) {
        return Ok(());
    }

    // Sibling temp: same directory, so the rename stays within one filesystem.
    // The pid keeps concurrent generators (the jobserver fans these out) from
    // clobbering each other's staging file.
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(format!(".nros-tmp.{}", std::process::id()));
    let tmp = PathBuf::from(tmp);

    // Scope the handle so it is closed before the rename; on some platforms
    // renaming an open file is legal but confusing, and we want the flush
    // error HERE rather than swallowed by the drop.
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&tmp)?;
        if let Err(e) = f.write_all(new).and_then(|()| f.flush()) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    }

    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
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
    // RFC-0068 Stage 1: resolve the message into the neutral IR and read its
    // hash, instead of calling the RIHS primitives directly (phase-335 W1.c).
    let resolved =
        rosidl_codegen::ResolvedMessage::resolve(fqn, message, |f| resolve(f)).map_err(|e| {
            eyre::eyre!("RIHS01 type-hash computation failed for {fqn} ({edition:?}): {e}")
        })?;
    Ok(resolved.type_hash)
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
    // RFC-0068 Stage 1: one ResolvedService carries the request/response/service
    // hashes; same REP-2011 computation as the three primitive calls it replaces
    // (phase-335 W1.c).
    let resolved =
        rosidl_codegen::ResolvedService::resolve(package, srv_name, service, |f| resolve(f))
            .map_err(|e| eyre::eyre!("RIHS01 {package}/srv/{srv_name} ({edition:?}): {e}"))?;
    Ok((
        resolved.request_hash,
        resolved.response_hash,
        resolved.type_hash,
    ))
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
    // RFC-0068 Stage 1: ResolvedAction carries the full §3b hash bundle
    // (phase-335 W1.c).
    let resolved =
        rosidl_codegen::ResolvedAction::resolve(package, action_name, action, |f| resolve(f))
            .map_err(|e| eyre::eyre!("RIHS01 {package}/action/{action_name} ({edition:?}): {e}"))?;
    Ok(resolved.hashes)
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

    // phase-403 W6 — the derived per-type bound leaves codegen as build
    // metadata instead of stopping inside the generated code. `self_resolve`
    // above is the CLOSED nested-type resolver the REP-2011 type hash already
    // demands, so this path can bound every type it can hash: same-package
    // types from this package's own share dir, everything else from the
    // caller's cross-package resolver.
    let mut inventory = rosidl_codegen::BoundInventory::new(&package.name);

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
        inventory.record_message(&fqn, &parsed_msg, resolver, &self_resolve);
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

    // phase-403 W6 — the Cargo half of the export. Three files, one model:
    //
    // * `nros_message_bounds.json` — the canonical artifact, readable by
    //   anything that can read a file (this is what a Kconfig generator or a
    //   `mem-report` style tool wants);
    // * `build.rs` — prints the same document as `cargo:` metadata;
    // * `links = "nros_msgs_<pkg>"` in the manifest — which is what makes cargo
    //   hand that metadata to a DEPENDENT's build script, as
    //   `DEP_NROS_MSGS_<PKG>_BOUNDS_JSON`.
    //
    // That is the channel `nros-c`'s build script already reads
    // `DEP_NROS_NODE_MAX_CBS` / `DEP_NROS_NODE_RX_BUF_SIZE` on. A message crate
    // links no native library; `links` is used here purely as the metadata
    // channel cargo makes it, which is also how `nros-node` uses it.
    //
    // Cost, stated rather than discovered: every generated message crate now
    // has a build script, so a cold build compiles and runs one more tiny crate
    // per interface package. There is no other cargo mechanism that delivers a
    // value INTO a dependent's build script without that dependent shelling out
    // to `cargo metadata`.
    write_bound_inventory(&package_output, &inventory)?;

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
///
/// # The codegen-version token (phase-429 W1)
///
/// Every generated crate carries the codegen version it was emitted at and
/// refuses to compile against a runtime that does not accept it. The number is
/// read from `nros_core::NROS_CODEGEN_VERSION` rather than re-spelled here, so
/// the emitter cannot drift from the runtime it is part of.
///
/// ## Why the assertion goes in `lib.rs`, at crate scope
///
/// "Was this crate emitted by a compatible codegen?" is a property of the
/// **crate**, not of any one call site, so the check must fire for a crate that
/// is merely compiled -- including under `cargo check`, which is all several
/// lanes run.
///
/// A crate-scope `const _: () = assert!(...)` gets exactly that: rustc
/// evaluates crate-scope const items during analysis, so `cargo check` reports
/// the failure. `nros_node::format_check`'s inline `const {}` block does not --
/// it is evaluated by the monomorphisation collector, which runs only during
/// codegen, so `cargo check` compiles a mismatch silently (RFC-0088 records
/// that measurement). That timing is right there, where the assertion is about
/// one generic instantiation and `test-unit` builds anyway; it is the wrong
/// timing here.
///
/// It also means the failure needs no message type, no monomorphisation and no
/// reachable call site: a generated crate nothing has started using yet still
/// refuses to build.
///
/// `nros_core` is always nameable from a generated crate -- `generate_cargo_toml`
/// emits it as an unconditional, unfeatured dependency -- so the assertion needs
/// no `cfg` gate.
fn generate_lib_rs(src_dir: &Path, package: &Package) -> Result<()> {
    let mut content = String::new();
    content.push_str("//! Generated nros bindings\n");
    content.push_str("//!\n");
    content.push_str("//! This crate is `no_std` compatible.\n\n");
    content.push_str("#![no_std]\n");
    content.push_str("#![allow(dead_code)]\n");
    // Generated code is not lint-groomed: consumers build it under
    // `clippy -D warnings` lanes (the zephyr fixture's run_rust_clippy), and
    // a stylistic lint in EMITTED code fails builds nobody can fix by hand
    // (the file is regenerated). Suppress clippy wholesale here — lint the
    // GENERATOR's templates, not its output.
    content.push_str("#![allow(clippy::all)]\n\n");

    content.push_str(&emit_codegen_version_token());

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

/// The codegen-version token and its compile-time assertion, as emitted into a
/// generated crate's `lib.rs`. See [`generate_lib_rs`] for why it sits at crate
/// scope.
///
/// The version is `nros_core::NROS_CODEGEN_VERSION` -- the runtime constant
/// itself, not a copy of it, so there is nothing here that can drift.
///
/// The assert message is a plain literal on purpose: a `const`-context
/// `assert!` cannot format, and a `{}` inside the string would be read by
/// `format_args!` as a capture, so the version is baked in as text instead.
fn emit_codegen_version_token() -> String {
    format!(
        r#"/// The nros codegen version this crate was emitted at (phase-429 W1).
///
/// The runtime declares the versions it accepts in `nros_core::codegen_version`;
/// the assertion below is what turns a disagreement into a build failure rather
/// than a wrong field offset several frames down at run time.
pub const NROS_EMITTED_CODEGEN_VERSION: u32 = {version};

// Crate scope, deliberately: rustc evaluates a crate-scope `const` item for a
// crate that is merely COMPILED, so `cargo check` reports this. An inline
// `const {{}}` block inside a generic (RFC-0088's `format_check`) is
// monomorphisation-timed, and `cargo check` never sees it.
const _: () = assert!(
    nros_core::codegen_version::accepts(NROS_EMITTED_CODEGEN_VERSION),
    "this crate was generated by nros codegen version {version}, which the \
     nros-core it is being compiled against does not accept -- see \
     nros_core::codegen_version. Regenerate the generated/ tree with this \
     checkout's codegen: `nros sync` in a consumer workspace, or \
     `just generate-bindings` in-tree."
);

"#,
        version = nros_core::NROS_CODEGEN_VERSION,
    )
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
    let registry = format!(r#"{crate_name} = {{ version = "*", default-features = false }}"#);
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

/// phase-403 W6 -- write the derived-bound inventory into a generated Rust
/// message crate, and the `build.rs` that puts it on the Cargo `links` channel.
///
/// The JSON file is the canonical artifact; `build.rs` republishes the same
/// document (compacted, because a `cargo:` value may not contain a newline) so
/// a dependent's build script can read it without knowing where the crate lives
/// on disk.
fn write_bound_inventory(
    package_output: &Path,
    inventory: &rosidl_codegen::BoundInventory,
) -> Result<()> {
    // phase-403 W7b (issue 0961) -- see the sibling in `cargo-nano-ros`. One
    // check per package, after every type is recorded, so one build names every
    // type that blew its stated budget.
    inventory.check_budgets().map_err(|e| eyre!("{e}"))?;
    write_if_changed(
        package_output.join(rosidl_codegen::INVENTORY_JSON_NAME),
        inventory.to_json(),
    )?;
    write_if_changed(package_output.join("build.rs"), inventory.to_build_rs())?;
    Ok(())
}

fn generate_cargo_toml(
    output_dir: &Path,
    package_name: &str,
    ament_version: &str,
    dependencies: &HashSet<String>,
    has_actions: bool,
) -> Result<()> {
    // phase-359 W10 — no `std` feature is emitted any more.
    //
    // A generated message crate has no `std::` path and no `feature = "std"`
    // arm of its own; the feature was built here purely to forward the flavour
    // to nros-core / nros-serdes (and, transitively, to every message crate it
    // depends on). That is how a leaf asking for `alloc` still ended up with a
    // hosted core — the same defect the RMW backends had, one tier down and
    // multiplied by every regenerated crate on every host.
    //
    // The two `cargo_nros.toml.jinja` copies carry the matching comment. They
    // are DORMANT — this function is the live emitter, which is exactly what
    // their own header warns about — so they are kept in step rather than
    // trusted.

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
    // phase-403 W6 — `links` is declared here purely as cargo's METADATA
    // channel: it is what makes the `cargo:` lines this crate's `build.rs`
    // prints reach a dependent's build script as `DEP_NROS_MSGS_<PKG>_*`. No
    // native library is linked. `nros-node` uses `links = "nros_node"` the same
    // way, and `nros-c` reads `DEP_NROS_NODE_RX_BUF_SIZE` off it.
    //
    // Cargo requires `links` to be unique across a dependency graph; a
    // generated crate is named after its ament package, which already is.
    let mut cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.0.0"
edition = "2021"
links = "{links_key}"

# Version of the interface package this was generated FROM. Informational: it
# varies by host (ROS install vs vendored interfaces) and must never reach the
# `version` field above, or it lands in consumers' lockfiles.
[package.metadata.nros]
ament_version = "{}"

[features]
default = []

[dependencies]
{nros_core_dep}
{nros_serdes_dep}
heapless = "0.8"
"#,
        package_name,
        ament_version,
        links_key = rosidl_codegen::BoundInventory::links_key(package_name),
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
    let (stage, package) = stage_px4_msgs(px4_dir, output_dir, version, &[])?;

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

/// Stage the PX4 `.msg` tree into one flat synthetic ament package.
///
/// `msg/` + `msg/versioned/` are copied into a single `msg/` dir (versioned last
/// so it shadows a same-named base entry — it is the canonical definition), which
/// is exactly the `share_dir/msg/<Name>.msg` layout the ament-driven generators
/// expect.
///
/// `only` selects a SUBSET by message name (issue 0362 approach B: a bridge
/// carries a handful of topics, not PX4's ~200). Accepts either the CamelCase
/// message name (`VehicleStatus`) or the snake_case uORB topic (`vehicle_status`).
/// Empty ⇒ every message. Nested types are always staged regardless of the filter,
/// because the RIHS01 hash is computed over the CLOSED type DAG.
///
/// Returns the staging dir (caller removes it) and the synthetic package.
fn stage_px4_msgs(
    px4_dir: &Path,
    output_dir: &Path,
    version: &str,
    only: &[String],
) -> Result<(PathBuf, Package)> {
    use crate::ament::InterfaceFiles;

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
            // Stage EVERY message even when filtering: a selected message may nest
            // another, and the type hash needs the closed DAG.
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

    // Apply the subset filter to the EMITTED list (staging keeps the full DAG).
    let messages = if only.is_empty() {
        names
    } else {
        let mut selected = Vec::new();
        for want in only {
            // Accept `VehicleStatus` or the uORB topic spelling `vehicle_status`.
            let matched = names
                .iter()
                .find(|n| n.as_str() == want.as_str() || to_snake_case(n) == to_snake_case(want));
            match matched {
                Some(n) => {
                    if !selected.contains(n) {
                        selected.push(n.clone());
                    }
                }
                None => {
                    let _ = std::fs::remove_dir_all(&stage);
                    eyre::bail!(
                        "px4 message `{want}` not found under {}/msg (or msg/versioned)",
                        px4_dir.display()
                    );
                }
            }
        }
        selected
    };

    let package = Package {
        name: "px4_msgs".to_string(),
        version: version.to_string(),
        share_dir: stage.clone(),
        interfaces: InterfaceFiles {
            messages,
            services: Vec::new(),
            actions: Vec::new(),
            idl_messages: Vec::new(),
            idl_services: Vec::new(),
            idl_actions: Vec::new(),
        },
    };
    Ok((stage, package))
}

/// Generated C++ `px4_msgs` headers (issue 0362).
#[derive(Debug)]
pub struct GeneratedPx4CppPackage {
    /// Directory the headers were written to.
    pub output_dir: PathBuf,
    /// Number of message headers emitted.
    pub message_count: usize,
}

/// Issue 0362 — emit CDR `px4_msgs::msg::*` as **C++ headers**, so an in-firmware
/// C++ PX4 module (the phase-325 W3 uORB→RMW bridge) can publish uORB data on a
/// ROS wire.
///
/// The direct uORB path needs none of this — `publisher_publish_raw` hands the PX4
/// struct straight to `orb_publish`. CDR is required only where nano-ros speaks the
/// ROS 2 wire protocol from inside PX4, and there the RIHS01 **type hash** is
/// load-bearing: `rmw_zenoh` keys discovery on it, so a guessed hash either never
/// matches or matches the wrong type. The hash therefore comes from the same
/// generator that emits the struct — [`compute_msg_type_hash`], exactly as the Rust
/// crate does — never from a second source.
///
/// Approach B: `only` names the handful of topics a bridge carries. The whole
/// `.msg` tree is still staged (nested types must resolve for the hash), but only
/// the selected messages are emitted.
pub fn generate_px4_msgs_cpp(
    px4_dir: &Path,
    output_dir: &Path,
    version: &str,
    edition: RosEdition,
    resolver: &CapacityResolver,
    only: &[String],
) -> Result<GeneratedPx4CppPackage> {
    let (stage, package) = stage_px4_msgs(px4_dir, output_dir, version, only)?;
    let result = emit_px4_cpp_headers(&package, output_dir, edition, resolver);
    let _ = std::fs::remove_dir_all(&stage);
    result
}

/// Message names this message nests from its OWN package — the siblings whose
/// headers the generated header will `#include` (issue 0362 transitive closure).
///
/// A same-package reference parses as `NamespacedType { package: None }`; a
/// cross-package one names its package and is out of scope here (px4_msgs is
/// self-contained). Array/sequence element types are unwrapped.
fn same_package_nested(message: &Message, package_name: &str) -> Vec<String> {
    fn walk(t: &rosidl_parser::FieldType, pkg: &str, out: &mut Vec<String>) {
        use rosidl_parser::FieldType as F;
        match t {
            F::NamespacedType { package, name, .. } => {
                if package.is_none() || package.as_deref() == Some(pkg) {
                    out.push(name.clone());
                }
            }
            F::Array { element_type, .. }
            | F::Sequence { element_type }
            | F::BoundedSequence { element_type, .. } => walk(element_type, pkg, out),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for field in &message.fields {
        walk(&field.field_type, package_name, &mut out);
    }
    out
}

fn emit_px4_cpp_headers(
    package: &Package,
    output_dir: &Path,
    edition: RosEdition,
    resolver: &CapacityResolver,
) -> Result<GeneratedPx4CppPackage> {
    // Headers land under `<out>/px4_msgs/msg/` so a module adds ONE include dir
    // (`<out>`) and writes `#include <px4_msgs/msg/vehicle_status.hpp>`.
    let msg_dir = output_dir.join(&package.name).join("msg");
    std::fs::create_dir_all(&msg_dir)
        .wrap_err_with(|| format!("create output dir {}", msg_dir.display()))?;

    // Same-package nested resolution — px4_msgs is self-contained.
    let self_resolve = |fqn: &str| -> Option<Message> {
        let mut parts = fqn.split('/');
        let pkg = parts.next()?;
        let name = parts.next_back()?;
        if pkg == package.name {
            let content = std::fs::read_to_string(package.get_message_path(name)).ok()?;
            rosidl_parser::parse_message(&content).ok()
        } else {
            None
        }
    };

    // Transitively close the emit set: a selected message's generated header
    // `#include`s its same-package nested siblings (`extract_intra_package_includes`),
    // so emitting only the named list would leave dangling includes. Walk the
    // nested types breadth-first and emit those too.
    let mut queue: Vec<String> = package.interfaces.messages.clone();
    let mut seen: HashSet<String> = queue.iter().cloned().collect();
    let mut message_count = 0;

    while let Some(msg_name) = queue.pop() {
        let msg_path = package.get_message_path(&msg_name);
        let content = std::fs::read_to_string(&msg_path)
            .wrap_err_with(|| format!("read message file {}", msg_path.display()))?;
        let parsed = rosidl_parser::parse_message(&content)
            .wrap_err_with(|| format!("parse message {msg_name}"))?;

        // Same-package nested types become sibling headers. NOT
        // `extract_dependencies` — that yields cross-package PACKAGE names (a
        // same-package field carries `package: None`), which is the Cargo-dep
        // question, not this one.
        for name in same_package_nested(&parsed, &package.name) {
            if seen.insert(name.clone()) {
                queue.push(name);
            }
        }

        let fqn = format!("{}/msg/{}", package.name, msg_name);
        let type_hash = compute_msg_type_hash(edition, &fqn, &parsed, &self_resolve)?;

        // phase-408 W1 — the same `self_resolve` the type hash is computed
        // through, so the header's derived size bound sees the nested types.
        let generated = rosidl_codegen::generate_cpp_message_package_with_lookup(
            &package.name,
            &msg_name,
            &parsed,
            &type_hash,
            resolver,
            &self_resolve,
        )
        .map_err(|e| eyre::eyre!("generate C++ message {msg_name}: {e}"))?;

        // Header + the split FFI Rust glue. The header's serialize/deserialize are
        // Rust symbols (`nros_cpp_{serialize,deserialize}_*`), so the `_types.rs` /
        // `_exports.rs` pair MUST travel with it — a consumer builds them into the
        // FFI staticlib it links, exactly as `nros_generate_interfaces(LANGUAGE CPP)`
        // does for a normal package.
        write_if_changed(msg_dir.join(&generated.header_name), &generated.header)
            .wrap_err_with(|| format!("write header for {msg_name}"))?;
        write_if_changed(
            msg_dir.join(&generated.ffi.types_rs_name),
            &generated.ffi.types_rs,
        )
        .wrap_err_with(|| format!("write ffi types for {msg_name}"))?;
        write_if_changed(
            msg_dir.join(&generated.ffi.exports_rs_name),
            &generated.ffi.exports_rs,
        )
        .wrap_err_with(|| format!("write ffi exports for {msg_name}"))?;

        // ROS-style alias header so a module writes
        // `#include <px4_msgs/msg/vehicle_status.hpp>` rather than the flat
        // prefixed form.
        let alias = msg_dir.join(format!("{}.hpp", to_snake_case(&msg_name)));
        if alias.file_name() != Some(std::ffi::OsStr::new(&generated.header_name)) {
            write_if_changed(
                &alias,
                format!("#pragma once\n#include \"{}\"\n", generated.header_name),
            )
            .wrap_err_with(|| format!("write alias header for {msg_name}"))?;
        }
        message_count += 1;
    }

    Ok(GeneratedPx4CppPackage {
        output_dir: output_dir.join(&package.name),
        message_count,
    })
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

        // phase-403 W6 — a generated crate DOES carry a `build.rs` now, and it
        // still links no C library. The assertion here used to be
        // `!build.rs.exists()` with the comment "no C library linking", which
        // conflated two things: the build script exists solely because `links`
        // is cargo's metadata channel, and `links` is how the derived per-type
        // size bounds reach a DEPENDENT's build script. Nothing native is
        // linked, which is what that comment was really guarding.
        let build_rs = std::fs::read_to_string(pkg_dir.join("build.rs")).unwrap();
        assert!(build_rs.contains("cargo:bounds_json="));
        assert!(
            !build_rs.contains("rustc-link-lib") && !build_rs.contains("rustc-link-search"),
            "a generated message crate must still link no native library"
        );

        // The inventory itself, and the `links` key that carries it.
        assert!(pkg_dir.join("nros_message_bounds.json").exists());
        let cargo_toml = std::fs::read_to_string(pkg_dir.join("Cargo.toml")).unwrap();
        assert!(cargo_toml.contains("links = \"nros_msgs_test_pkg\""));
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
        // phase-335 W5 — resolve-only: the cross-package dep was pulled into the
        // hash DAG through the resolver, but no crate is emitted for it. Only the
        // target package appears in the output tree.
        assert!(
            !out.join("builtin_interfaces").exists(),
            "the resolved dep builtin_interfaces must NOT be emitted — it is hash-only"
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

    /// phase-429 W1 — the emitted crate carries the codegen version it was
    /// emitted at, and that number is THE runtime constant.
    ///
    /// This is the runnable half of the compatibility token. The other half —
    /// that an unaccepted version is a compile error — cannot be asserted by a
    /// running test (no compile-fail harness in this workspace, and CLAUDE.md
    /// bans shelling out to `cargo` from a test); `nros_core::codegen_version`
    /// records what covers it instead. What this test can prove is that the
    /// assertion is really emitted, at CRATE scope rather than inside a
    /// generic, and that it reads the constant it claims to.
    #[test]
    fn lib_rs_carries_the_codegen_version_token() {
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

        let lib_rs =
            fs::read_to_string(output_dir.join("test_pkg").join("src").join("lib.rs")).unwrap();

        // The emitted token is the runtime constant, not a second literal that
        // needs keeping in step.
        let expected = format!(
            "pub const NROS_EMITTED_CODEGEN_VERSION: u32 = {};",
            nros_core::NROS_CODEGEN_VERSION
        );
        assert!(
            lib_rs.contains(&expected),
            "generated lib.rs must stamp the runtime's NROS_CODEGEN_VERSION \
             ({}); expected to find `{expected}` in:\n{lib_rs}",
            nros_core::NROS_CODEGEN_VERSION,
        );

        // The assertion, and that it consults the runtime rather than
        // re-deciding acceptance locally.
        assert!(
            lib_rs.contains("nros_core::codegen_version::accepts(NROS_EMITTED_CODEGEN_VERSION)"),
            "generated lib.rs must ask the runtime whether it accepts the \
             emitted version, got:\n{lib_rs}"
        );

        // Crate scope, not a `const {}` block inside a generic: that is what
        // makes `cargo check` fire it. Assert the shape, because the placement
        // IS the decision (see `generate_lib_rs`).
        assert!(
            lib_rs.contains("const _: () = assert!("),
            "the check must be a crate-scope const item, so a crate that is \
             merely compiled fails; got:\n{lib_rs}"
        );
        let token_at = lib_rs.find("const _: () = assert!(").unwrap();
        let first_mod = lib_rs.find("pub mod ").unwrap_or(lib_rs.len());
        assert!(
            token_at < first_mod,
            "the token must precede the crate's modules, so it is the first \
             thing a reader (and rustc) meets; got:\n{lib_rs}"
        );

        // The error has to tell the reader what to do. If this message is ever
        // reworded, keep it actionable rather than deleting the assertion.
        assert!(
            lib_rs.contains("nros sync"),
            "the compile error must name the fix (`nros sync`), got:\n{lib_rs}"
        );
    }

    /// A package with no services and no actions still gets the token: the
    /// version is a property of the CRATE, not of what it happens to contain.
    #[test]
    fn a_messages_only_crate_also_carries_the_token() {
        let temp_dir = tempfile::tempdir().unwrap();
        let share_dir = temp_dir.path().join("msgs_only_token");

        let msg_dir = share_dir.join("msg");
        fs::create_dir_all(&msg_dir).unwrap();
        write_if_changed(msg_dir.join("Int32.msg"), "int32 data\n").unwrap();

        let package = Package::from_share_dir(share_dir).unwrap();
        let output_dir = temp_dir.path().join("output");
        generate_package(
            &package,
            &output_dir,
            RosEdition::Humble,
            &CapacityResolver::empty(),
            &no_cross_pkg_resolver,
        )
        .unwrap();

        let lib_rs = fs::read_to_string(
            output_dir
                .join("msgs_only_token")
                .join("src")
                .join("lib.rs"),
        )
        .unwrap();
        assert!(lib_rs.contains("NROS_EMITTED_CODEGEN_VERSION"));
        assert!(lib_rs.contains("const _: () = assert!("));
        assert!(!lib_rs.contains("pub mod srv"));
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

    // ---- issue 0362: the C++ px4_msgs emitter -------------------------------

    /// A miniature PX4-shaped `.msg` tree: `msg/` + `msg/versioned/`, with one
    /// message that NESTS another so the transitive-closure rule is exercised.
    fn write_fake_px4_tree(root: &Path) {
        let msg = root.join("msg");
        let versioned = msg.join("versioned");
        fs::create_dir_all(&versioned).unwrap();
        fs::write(msg.join("Nested.msg"), "uint8 flag\n").unwrap();
        fs::write(
            msg.join("VehicleStatus.msg"),
            "uint64 timestamp\nuint8 arming_state\nNested nested\n",
        )
        .unwrap();
        fs::write(msg.join("DebugKeyValue.msg"), "float32 value\n").unwrap();
        // versioned/ shadows a same-named base entry
        fs::write(versioned.join("DebugKeyValue.msg"), "float32 value\n").unwrap();
    }

    /// The topic filter emits ONLY what was asked for — plus the nested types the
    /// selected headers `#include` (issue 0362 approach B).
    #[test]
    fn px4_cpp_emits_only_the_named_topics_plus_nested() {
        let tmp = tempfile::tempdir().unwrap();
        let px4 = tmp.path().join("PX4-Autopilot");
        write_fake_px4_tree(&px4);
        let out = tmp.path().join("out");

        let generated = generate_px4_msgs_cpp(
            &px4,
            &out,
            "1.17.0",
            RosEdition::Jazzy,
            &CapacityResolver::empty(),
            &["vehicle_status".to_string()], // uORB topic spelling resolves
        )
        .expect("emit C++ px4_msgs");

        let msg_dir = out.join("px4_msgs").join("msg");
        // VehicleStatus + the Nested it pulls in transitively.
        assert!(msg_dir.join("px4_msgs_msg_vehicle_status.hpp").is_file());
        assert!(
            msg_dir.join("px4_msgs_msg_nested.hpp").is_file(),
            "a nested type the selected header #includes must be emitted too"
        );
        // NOT selected, not nested by the selection.
        assert!(
            !msg_dir.join("px4_msgs_msg_debug_key_value.hpp").exists(),
            "the filter must not emit unrequested messages"
        );
        assert_eq!(generated.message_count, 2);
    }

    /// The header travels with its FFI glue (its serialize/deserialize are Rust
    /// symbols) and a ROS-style alias header.
    #[test]
    fn px4_cpp_emits_ffi_glue_and_alias_header() {
        let tmp = tempfile::tempdir().unwrap();
        let px4 = tmp.path().join("PX4-Autopilot");
        write_fake_px4_tree(&px4);
        let out = tmp.path().join("out");

        generate_px4_msgs_cpp(
            &px4,
            &out,
            "1.17.0",
            RosEdition::Jazzy,
            &CapacityResolver::empty(),
            &["DebugKeyValue".to_string()],
        )
        .unwrap();

        let msg_dir = out.join("px4_msgs").join("msg");
        assert!(
            msg_dir
                .join("px4_msgs_msg_debug_key_value_types.rs")
                .is_file()
        );
        assert!(
            msg_dir
                .join("px4_msgs_msg_debug_key_value_exports.rs")
                .is_file()
        );
        let alias = msg_dir.join("debug_key_value.hpp");
        assert!(alias.is_file(), "ROS-style alias header");
        assert!(
            fs::read_to_string(&alias)
                .unwrap()
                .contains("px4_msgs_msg_debug_key_value.hpp")
        );
    }

    /// The hash is the whole point: it must be a REAL RIHS01 (not the all-zero
    /// placeholder the CMake C++ path emits) and identical to what the Rust crate
    /// carries — a guessed hash either never matches on the wire or matches the
    /// wrong type.
    #[test]
    fn px4_cpp_hash_is_real_and_matches_the_rust_crate() {
        let tmp = tempfile::tempdir().unwrap();
        let px4 = tmp.path().join("PX4-Autopilot");
        write_fake_px4_tree(&px4);

        let cpp_out = tmp.path().join("cpp");
        generate_px4_msgs_cpp(
            &px4,
            &cpp_out,
            "1.17.0",
            RosEdition::Jazzy,
            &CapacityResolver::empty(),
            &["VehicleStatus".to_string()],
        )
        .unwrap();
        let hpp = fs::read_to_string(
            cpp_out
                .join("px4_msgs/msg")
                .join("px4_msgs_msg_vehicle_status.hpp"),
        )
        .unwrap();

        let rust_out = tmp.path().join("rust");
        generate_px4_msgs(
            &px4,
            &rust_out,
            "1.17.0",
            RosEdition::Jazzy,
            &CapacityResolver::empty(),
        )
        .unwrap();
        let rs = fs::read_to_string(rust_out.join("px4_msgs/src/msg").join("vehicle_status.rs"))
            .unwrap();

        let grab = |text: &str| -> String {
            let at = text.find("RIHS01_").expect("a real RIHS01 hash");
            text[at..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect()
        };
        let cpp_hash = grab(&hpp);
        let rust_hash = grab(&rs);
        assert!(
            !cpp_hash
                .trim_start_matches("RIHS01_")
                .chars()
                .all(|c| c == '0'),
            "emitted the all-zero placeholder hash: {cpp_hash}"
        );
        assert_eq!(
            cpp_hash, rust_hash,
            "the C++ header and the Rust crate must carry the SAME type hash"
        );
    }

    /// An unknown topic is a hard error, not a silently-empty emit.
    #[test]
    fn px4_cpp_unknown_topic_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let px4 = tmp.path().join("PX4-Autopilot");
        write_fake_px4_tree(&px4);
        let err = generate_px4_msgs_cpp(
            &px4,
            &tmp.path().join("out"),
            "1.17.0",
            RosEdition::Jazzy,
            &CapacityResolver::empty(),
            &["no_such_topic".to_string()],
        )
        .unwrap_err();
        assert!(err.to_string().contains("no_such_topic"), "{err}");
    }
}
