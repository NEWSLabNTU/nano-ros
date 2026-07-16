//! cargo-nano-ros library
//!
//! This library provides functionality for generating nros message bindings
//! from package.xml dependencies.
//!
//! # Public API
//!
//! This library exposes a high-level API for:
//! - Generating Rust bindings from package.xml dependencies
//! - Generating bindings for individual ROS 2 interface packages
//! - Cleaning generated bindings
//!
//! # Example
//!
//! ```no_run
//! use cargo_nano_ros::{GenerateConfig, generate_from_package_xml};
//! use std::path::PathBuf;
//!
//! let config = GenerateConfig {
//!     manifest_path: PathBuf::from("package.xml"),
//!     output_dir: PathBuf::from("generated"),
//!     generate_config: true,
//!     nano_ros_path: None,
//!     nano_ros_git: false,
//!     force: false,
//!     verbose: false,
//!     ros_edition: "humble".to_string(),
//!     renames: std::collections::HashMap::new(),
//!     codegen_config: None,
//! };
//!
//! generate_from_package_xml(config).expect("Failed to generate bindings");
//! ```

pub mod ament_installer;
pub mod cache;
pub mod capability_resolver;
pub mod config_patcher;
// Phase 212.K.7.1 — `cyclonedds_emit` (msg-crate-side cyclonedds
// feature + descriptor build.rs) is retired. Generated msg crates are
// RMW-agnostic; cyclonedds type registration happens at runtime
// inside `nros-rmw-cyclonedds` via the `nros-serdes` schema API
// (K.7.3) + descriptor builder (K.7.4). The standalone
// `nros codegen cyclonedds-descriptors` verb (K.4) still emits
// host-side descriptor C and lives in `nros-cli-core`.
pub mod dependency_parser;
pub mod package_discovery;
pub mod package_xml;
pub mod rmw_resolver;
pub mod scaffold;
pub mod workflow;

use eyre::{Result, WrapErr, eyre};
use rosidl_bindgen::ament::{AmentIndex, Package};
use rosidl_codegen::{RosEdition, utils::to_snake_case};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// Idempotent file write — skip the write (and the mtime bump) if the file
/// already contains exactly the same bytes. Codegen reruns under cmake
/// otherwise pump fresh mtimes into hundreds of generated files every build,
/// causing cargo to recompile every downstream FFI crate even when nothing
/// actually changed.
fn write_if_changed<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> std::io::Result<()> {
    let path = path.as_ref();
    let new = contents.as_ref();
    if std::fs::read(path).is_ok_and(|existing| existing == new) {
        return Ok(());
    }
    std::fs::write(path, new)
}

/// Path to bundled interface files relative to the cargo-nano-ros crate root.
/// These are shipped with nros so codegen works without a ROS 2 environment.
const BUNDLED_INTERFACES_DIR: &str = "interfaces";

/// Parse `--rename old=new` argument.
pub fn parse_rename(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!("expected format: old_pkg=new_crate_name, got: {s}"));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Configuration for generating bindings from package.xml
#[derive(Debug, Clone)]
pub struct GenerateConfig {
    /// Path to package.xml
    pub manifest_path: PathBuf,
    /// Output directory for generated bindings
    pub output_dir: PathBuf,
    /// Generate .cargo/config.toml with [patch.crates-io] entries
    pub generate_config: bool,
    /// Path to nros crates directory (for config patches)
    pub nano_ros_path: Option<PathBuf>,
    /// Use nros git repository for config patches
    pub nano_ros_git: bool,
    /// Overwrite existing bindings
    pub force: bool,
    /// Enable verbose output
    pub verbose: bool,
    /// ROS 2 edition for type hash format ("humble" or "iron")
    pub ros_edition: String,
    /// Package name remap: `old_pkg_name → new_crate_name`.
    ///
    /// Renames the generated crate, its directory, cross-package `use` references,
    /// and Cargo.toml dependency names. Used by nano-ros to generate
    /// `nros-rcl-interfaces` instead of `rcl_interfaces`.
    pub renames: std::collections::HashMap<String, String>,
    /// Explicit `nros-codegen.toml` path (RFC-0033). When `None`, the config is
    /// discovered by walking up from the manifest directory. The explicit file,
    /// if any, wins over discovered ones.
    pub codegen_config: Option<PathBuf>,
}

/// Configuration for binding generation (single package)
#[derive(Debug, Clone)]
pub struct BindgenConfig {
    /// ROS package name (e.g., "std_msgs")
    pub package_name: String,
    /// Optional direct path to package share directory (bypasses ament index)
    pub package_path: Option<PathBuf>,
    /// Output directory for generated bindings
    pub output_dir: PathBuf,
    /// Enable verbose output
    pub verbose: bool,
}

/// Configuration for ament installation (used by colcon integration)
#[derive(Debug, Clone)]
pub struct InstallConfig {
    /// Project root directory (where Cargo.toml is located)
    pub project_root: PathBuf,
    /// Install base directory (install/<package>/)
    pub install_base: PathBuf,
    /// Workspace build directory (where ros2_cargo_config.toml is located)
    pub build_base: PathBuf,
    /// Build profile: "debug" or "release"
    pub profile: String,
    /// Enable verbose output
    pub verbose: bool,
}

/// Configuration for C code generation (from JSON args file, for CMake)
#[derive(Debug, Clone)]
pub struct GenerateCConfig {
    /// Path to JSON arguments file
    pub args_file: PathBuf,
    /// Enable verbose output
    pub verbose: bool,
}

/// Configuration for C code generation from package.xml (standalone mode)
#[derive(Debug, Clone)]
pub struct GenerateCStandaloneConfig {
    /// Path to package.xml
    pub manifest_path: PathBuf,
    /// Output directory for generated C bindings
    pub output_dir: PathBuf,
    /// Overwrite existing bindings
    pub force: bool,
    /// Enable verbose output
    pub verbose: bool,
    /// ROS 2 edition for type hash format ("humble" or "iron")
    pub ros_edition: String,
    /// Explicit `nros-codegen.toml` path (RFC-0033); discovered from the
    /// manifest dir when `None`.
    pub codegen_config: Option<PathBuf>,
}

/// Configuration for C++ code generation
#[derive(Debug, Clone)]
pub struct GenerateCppConfig {
    /// Path to JSON arguments file
    pub args_file: PathBuf,
    /// Enable verbose output
    pub verbose: bool,
}

/// Parse a ROS edition string into a `RosEdition` enum value.
fn parse_ros_edition(s: &str) -> Result<RosEdition> {
    match s {
        "humble" => Ok(RosEdition::Humble),
        "iron" => Ok(RosEdition::Iron),
        _ => Err(eyre!(
            "Unknown ROS edition '{}'. Expected 'humble' or 'iron'.",
            s
        )),
    }
}

/// Generate bindings from package.xml dependencies.
///
/// This is the main entry point for standalone usage. It:
/// 1. Parses package.xml to find dependencies
/// 2. Resolves transitive dependencies via ament index
/// 3. Filters to interface packages (those with msg/srv/action)
/// 4. Generates nros bindings for each
/// 5. Optionally generates .cargo/config.toml
pub fn generate_from_package_xml(config: GenerateConfig) -> Result<()> {
    use package_xml::PackageXml;

    let edition = parse_ros_edition(&config.ros_edition)?;

    // Parse package.xml
    let pkg_xml = PackageXml::parse(&config.manifest_path)?;

    // Per-field capacity config (RFC-0033): discover `nros-codegen.toml` by
    // walking up from the manifest directory; an explicit --codegen-config wins.
    let manifest_dir = config
        .manifest_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let resolver = rosidl_codegen::CapacityResolver::resolve_for(
        config.codegen_config.as_deref(),
        manifest_dir,
        None,
    )?;

    if config.verbose {
        println!("Package: {} v{}", pkg_xml.name, pkg_xml.version);
        println!(
            "Dependencies from package.xml: {:?}",
            pkg_xml.all_dependencies()
        );
    }

    // Load ament index (with bundled interface fallback)
    let index = load_index_with_fallback(config.verbose)?;

    // Resolve all dependencies (including transitive)
    let all_deps =
        resolve_transitive_dependencies(&index, pkg_xml.all_dependencies(), config.verbose)?;

    if config.verbose {
        println!("Resolved {} total dependencies", all_deps.len());
    }

    // Filter to interface packages only
    let interface_packages = filter_interface_packages(&index, &all_deps, config.verbose)?;

    if interface_packages.is_empty() {
        println!("No interface packages found in dependencies");
        return Ok(());
    }

    println!(
        "Generating bindings for {} interface packages...",
        interface_packages.len()
    );

    // Create output directory
    std::fs::create_dir_all(&config.output_dir)?;

    // Generate bindings for each interface package
    let mut generated_packages = Vec::new();
    for (pkg_name, package) in &interface_packages {
        let pkg_output = config.output_dir.join(pkg_name);

        if config.verbose {
            if pkg_output.exists() && !config.force {
                println!("  Refreshing {} (reuses unchanged files)", pkg_name);
            } else {
                println!("  Generating {}...", pkg_name);
            }
        }

        let result = rosidl_bindgen::generator::generate_package(
            package,
            &config.output_dir,
            edition,
            &resolver,
        )?;

        println!(
            "  ✓ {} ({} messages, {} services, {} actions)",
            pkg_name, result.message_count, result.service_count, result.action_count
        );

        // Phase 212.K.7.1 — msg crates are RMW-agnostic. The K.4
        // per-msg-crate cyclonedds descriptor emit (+ Cargo `cyclonedds`
        // feature + build.rs) was removed; cyclonedds type registration
        // now happens at runtime inside `nros-rmw-cyclonedds` via the
        // `nros-serdes` schema API (K.7.3) + descriptor builder
        // (K.7.4). The standalone `nros codegen cyclonedds-descriptors`
        // verb still emits host-side descriptor C for app-side use.

        generated_packages.push(pkg_name.clone());
    }

    // Apply package renames (e.g., rcl_interfaces → nros-rcl-interfaces)
    if !config.renames.is_empty() {
        apply_package_renames(&config.output_dir, &config.renames, config.verbose)?;
    }

    // Generate .cargo/config.toml if requested
    if config.generate_config {
        generate_cargo_config(
            &config.output_dir,
            &generated_packages,
            config.nano_ros_path.as_deref(),
            config.nano_ros_git,
            config.verbose,
        )?;
    }

    println!(
        "✓ Generated bindings for {} packages in {}",
        generated_packages.len(),
        config.output_dir.display()
    );

    Ok(())
}

/// Apply package renames to generated output.
///
/// For each `old_name → new_name` mapping:
/// 1. Rename the output directory (`old_name/` → `new_name/`)
/// 2. Update `[package] name` in Cargo.toml
/// 3. Update cross-package dependency names in Cargo.toml
/// 4. Update `use old_name::` references in Rust source files
/// 5. Update `std` feature propagation in Cargo.toml
fn apply_package_renames(
    output_dir: &Path,
    renames: &std::collections::HashMap<String, String>,
    verbose: bool,
) -> Result<()> {
    use std::fs;

    // Build reverse lookup: old_crate_ident → new_crate_ident (for `use` statements)
    let ident_renames: std::collections::HashMap<String, String> = renames
        .iter()
        .map(|(old, new)| (old.replace('-', "_"), new.replace('-', "_")))
        .collect();

    // Phase 1: Rename directories
    for (old_name, new_name) in renames {
        let old_dir = output_dir.join(old_name);
        let new_dir = output_dir.join(new_name);
        if old_dir.exists() && old_dir != new_dir {
            if new_dir.exists() {
                fs::remove_dir_all(&new_dir)?;
            }
            fs::rename(&old_dir, &new_dir)?;
            if verbose {
                println!("  Renamed {} → {}", old_name, new_name);
            }
        }
    }

    // Phase 2: Fix Cargo.toml files and Rust source in all renamed packages
    for new_name in renames.values() {
        let pkg_dir = output_dir.join(new_name);
        if !pkg_dir.exists() {
            continue;
        }

        // Fix Cargo.toml
        let cargo_path = pkg_dir.join("Cargo.toml");
        if cargo_path.exists() {
            let mut content = fs::read_to_string(&cargo_path)?;

            // Replace package name
            for (old_name, new_name) in renames {
                // [package] name
                content = content.replace(
                    &format!("name = \"{}\"", old_name),
                    &format!("name = \"{}\"", new_name),
                );
                // [dependencies] and [features] references
                content = content.replace(
                    &format!("{} = {{ path", old_name),
                    &format!("{} = {{ path", new_name),
                );
                content = content.replace(
                    &format!("\"../{}\",", old_name),
                    &format!("\"../{}\",", new_name),
                );
                content = content.replace(
                    &format!("\"../{}\"", old_name),
                    &format!("\"../{}\"", new_name),
                );
                // std feature propagation
                content =
                    content.replace(&format!("{}/std", old_name), &format!("{}/std", new_name));
            }

            write_if_changed(&cargo_path, content)?;
        }

        // Fix Rust source files: replace `old_ident::` with `new_ident::`
        let src_dir = pkg_dir.join("src");
        if src_dir.exists() {
            fix_rust_idents_recursive(&src_dir, &ident_renames)?;
        }
    }

    Ok(())
}

/// Recursively fix Rust identifier references in .rs files.
///
/// Only replaces occurrences outside of string literals, so that
/// `RosMessage::TYPE_NAME` / `RosService::SERVICE_NAME` constants — which
/// must use the original ROS package name for DDS wire compatibility — are
/// left untouched when the Rust crate is renamed (e.g. `rcl_interfaces` →
/// `nros_rcl_interfaces`).
fn fix_rust_idents_recursive(
    dir: &Path,
    ident_renames: &std::collections::HashMap<String, String>,
) -> Result<()> {
    use std::fs;

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            fix_rust_idents_recursive(&path, ident_renames)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let mut content = fs::read_to_string(&path)?;
            let mut changed = false;

            for (old_ident, new_ident) in ident_renames {
                let old_ref = format!("{}::", old_ident);
                let new_ref = format!("{}::", new_ident);
                if content.contains(&old_ref) {
                    content = replace_outside_strings(&content, &old_ref, &new_ref);
                    changed = true;
                }
            }

            if changed {
                write_if_changed(&path, content)?;
            }
        }
    }
    Ok(())
}

/// Replace all occurrences of `old` with `new` in `src`, skipping content
/// inside double-quoted string literals.
///
/// This ensures that DDS `TYPE_NAME` / `SERVICE_NAME` string constants are not
/// affected by crate renames while Rust `use` paths and identifiers are.
fn replace_outside_strings(src: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let old_bytes = old.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    let mut in_char = false;

    while i < bytes.len() {
        // Handle escape sequences inside strings/chars
        if (in_string || in_char) && bytes[i] == b'\\' && i + 1 < bytes.len() {
            out.push(bytes[i] as char);
            out.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }
        // Toggle string literal tracking
        if !in_char && bytes[i] == b'"' {
            in_string = !in_string;
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        // Toggle char literal tracking
        if !in_string && bytes[i] == b'\'' {
            in_char = !in_char;
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        // Only substitute outside of string/char literals
        if !in_string && !in_char && bytes[i..].starts_with(old_bytes) {
            out.push_str(new);
            i += old_bytes.len();
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Get the path to bundled interface files.
///
/// Searches for the `interfaces/` directory relative to the cargo-nano-ros
/// crate manifest, then falls back to checking relative to the binary location.
fn bundled_interfaces_dir() -> Option<PathBuf> {
    // Try relative to CARGO_MANIFEST_DIR (works during cargo build).
    // cargo-nano-ros lives at packages/cli/cargo-nano-ros; the bundled share
    // dirs at packages/cli/interfaces/. Gate on std_msgs so a sibling
    // `interfaces/` dir of some other layout can't shadow it.
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let dir = PathBuf::from(manifest_dir)
            .parent()
            .map(|p| p.join(BUNDLED_INTERFACES_DIR));
        if let Some(ref d) = dir
            && d.join("std_msgs").is_dir()
        {
            return dir;
        }
    }

    // Try relative to the running binary
    if let Ok(exe) = std::env::current_exe() {
        // Walk up from binary to find the interfaces directory. The nros
        // binary lives at packages/cli/target/{release,debug}/nros; the
        // bundled share dirs live at packages/cli/interfaces/ (post
        // codegen-submodule retirement). Require the std_msgs share dir so
        // a stray `interfaces/` dir elsewhere on the walk can't match.
        let mut path = exe.as_path();
        for _ in 0..6 {
            if let Some(parent) = path.parent() {
                let candidate = parent.join(BUNDLED_INTERFACES_DIR);
                if candidate.join("std_msgs").is_dir() {
                    return Some(candidate);
                }
                // Retired layout: packages/codegen/interfaces/
                let candidate = parent.join("packages/codegen").join(BUNDLED_INTERFACES_DIR);
                if candidate.exists() {
                    return Some(candidate);
                }
                // Legacy layout: colcon-nano-ros/interfaces/
                let candidate = parent.join("colcon-nano-ros").join(BUNDLED_INTERFACES_DIR);
                if candidate.exists() {
                    return Some(candidate);
                }
                path = parent;
            }
        }
    }

    None
}

/// Load the ament index, merging bundled interfaces as fallback.
///
/// If a ROS 2 environment is sourced, the ament index takes precedence.
/// Bundled interfaces (std_msgs, builtin_interfaces) fill in any gaps.
/// Public: `nros sync` (nros-cli-core) resolves external msg deps through
/// this too, so a host without ROS 2 still generates std_msgs (#204).
pub fn load_index_with_fallback(verbose: bool) -> Result<AmentIndex> {
    // Phase 293 / issue #212 — workspace-local interface packages. Mirror the
    // cmake smart-stub layering (`_NrosFindRosMsgPackage.cmake`): entries of
    // `NROS_INTERFACE_SEARCH_PATH` (colon/semicolon-separated colcon-`src/`
    // roots) take precedence over the ament env index, which takes precedence
    // over the bundled interfaces. Without this layer, `resolve-deps` on a
    // node whose package.xml depends on a sibling workspace msg pkg resolved
    // nothing, so the C/C++ verbs never generated typed bindings for it.
    let mut index = AmentIndex::from_path_string("")?;
    if let Ok(search_path) = std::env::var("NROS_INTERFACE_SEARCH_PATH") {
        for root in search_path
            .split([':', ';'])
            .filter(|s| !s.is_empty())
            .map(std::path::Path::new)
        {
            match AmentIndex::from_directory(root) {
                Ok(ws_index) => {
                    if verbose && ws_index.package_count() > 0 {
                        println!(
                            "Loaded {} workspace interface package(s) from {}",
                            ws_index.package_count(),
                            root.display()
                        );
                    }
                    index.merge(ws_index);
                }
                Err(e) => {
                    if verbose {
                        eprintln!(
                            "Warning: failed to scan NROS_INTERFACE_SEARCH_PATH entry {}: {}",
                            root.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    // Ament env index next (workspace packages shadow it).
    match AmentIndex::from_env() {
        Ok(idx) => {
            if verbose {
                println!("Loaded ament index ({} packages)", idx.package_count());
            }
            index.merge(idx);
        }
        Err(_) => {
            if verbose {
                println!("No ROS 2 environment detected, using bundled interfaces only");
            }
        }
    }

    // Merge bundled interfaces (ament packages take precedence)
    if let Some(bundled_dir) = bundled_interfaces_dir() {
        match AmentIndex::from_directory(&bundled_dir) {
            Ok(bundled_index) => {
                if verbose {
                    println!(
                        "Loaded {} bundled interface packages from {}",
                        bundled_index.package_count(),
                        bundled_dir.display()
                    );
                }
                index.merge(bundled_index);
            }
            Err(e) => {
                if verbose {
                    eprintln!("Warning: failed to load bundled interfaces: {}", e);
                }
            }
        }
    } else if verbose {
        eprintln!("Warning: bundled interfaces directory not found");
    }

    Ok(index)
}

/// Resolve transitive dependencies
fn resolve_transitive_dependencies(
    index: &AmentIndex,
    initial: &HashSet<String>,
    verbose: bool,
) -> Result<HashSet<String>> {
    let mut all_deps = HashSet::new();
    let mut queue: Vec<String> = initial.iter().cloned().collect();
    let mut visited = HashSet::new();

    while let Some(pkg_name) = queue.pop() {
        if visited.contains(&pkg_name) {
            continue;
        }
        visited.insert(pkg_name.clone());
        all_deps.insert(pkg_name.clone());

        // Try to find package in ament index
        if let Some(package) = index.find_package(&pkg_name) {
            // Get dependencies from package.xml in share directory
            let pkg_xml_path = package.share_dir.join("package.xml");
            if pkg_xml_path.exists()
                && let Ok(pkg_xml) = package_xml::PackageXml::parse(&pkg_xml_path)
            {
                for dep in pkg_xml.all_dependencies() {
                    if !visited.contains(dep) {
                        queue.push(dep.clone());
                    }
                }
            }
        } else if verbose {
            eprintln!("  Warning: {} not found in ament index", pkg_name);
        }
    }

    Ok(all_deps)
}

/// Filter to only interface packages (those with msg/srv/action directories)
fn filter_interface_packages(
    index: &AmentIndex,
    packages: &HashSet<String>,
    verbose: bool,
) -> Result<Vec<(String, Package)>> {
    let mut interface_packages = Vec::new();

    for pkg_name in packages {
        if let Some(package) = index.find_package(pkg_name) {
            let has_msg = package.share_dir.join("msg").exists();
            let has_srv = package.share_dir.join("srv").exists();
            let has_action = package.share_dir.join("action").exists();

            if has_msg || has_srv || has_action {
                interface_packages.push((pkg_name.clone(), package.clone()));
                if verbose {
                    println!(
                        "  Found interface package: {} (msg={}, srv={}, action={})",
                        pkg_name, has_msg, has_srv, has_action
                    );
                }
            }
        }
    }

    // Sort for deterministic output
    interface_packages.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(interface_packages)
}

/// Generate .cargo/config.toml with patch entries using ConfigPatcher (TOML-aware, idempotent)
fn generate_cargo_config(
    output_dir: &Path,
    packages: &[String],
    nano_ros_path: Option<&Path>,
    nano_ros_git: bool,
    verbose: bool,
) -> Result<()> {
    let project_root = Path::new(".");
    let mut patcher = config_patcher::ConfigPatcher::new(project_root)?;

    // Add nros crate patches
    if let Some(crates_path) = nano_ros_path {
        // Path-based patches (for local development)
        patcher.add_patch("nros-core", &crates_path.join("nros-core"));
        patcher.add_patch("nros-serdes", &crates_path.join("nros-serdes"));
    } else if nano_ros_git {
        // Git-based patches (for external users)
        let git_url = "https://github.com/jerry73204/nano-ros";
        patcher.add_git_patch("nros-core", git_url);
        patcher.add_git_patch("nros-serdes", git_url);
    }

    // Add message package patches
    for pkg in packages {
        patcher.add_patch(pkg, &output_dir.join(pkg));
    }

    patcher.save()?;

    if verbose {
        let nano_count = if nano_ros_path.is_some() || nano_ros_git {
            2
        } else {
            0
        };
        println!(
            "Generated .cargo/config.toml with {} patch entries",
            packages.len() + nano_count
        );
    }

    Ok(())
}

/// Generate Rust bindings for a single ROS 2 interface package
pub fn generate_bindings(config: BindgenConfig) -> Result<()> {
    use rosidl_bindgen::generator;

    if config.verbose {
        eprintln!("Generating bindings for {}...", config.package_name);
    }

    // Get package either from path or ament index (with bundled fallback)
    let package = if let Some(share_path) = config.package_path {
        Package::from_share_dir(share_path)?
    } else {
        let index = load_index_with_fallback(config.verbose)?;
        index
            .find_package(&config.package_name)
            .ok_or_else(|| {
                eyre!(
                    "Package '{}' not found in ament index or bundled interfaces",
                    config.package_name
                )
            })?
            .clone()
    };

    // Generate bindings using rosidl-bindgen library (default to Humble).
    // Single-package path (colcon convenience): discover nros-codegen.toml from
    // the output directory upward (RFC-0033). The workspace path
    // (`generate_from_package_xml`) discovers from the manifest directory.
    let resolver =
        rosidl_codegen::CapacityResolver::discover(&config.output_dir, None).unwrap_or_default();
    let result =
        generator::generate_package(&package, &config.output_dir, RosEdition::Humble, &resolver)?;

    if config.verbose {
        eprintln!(
            "✓ Generated {} messages, {} services, {} actions for {}",
            result.message_count, result.service_count, result.action_count, config.package_name
        );
    }

    Ok(())
}

/// Arguments structure for C code generation (parsed from JSON)
#[derive(Debug, serde::Deserialize)]
struct GenerateCArgs {
    package_name: String,
    output_dir: PathBuf,
    interface_files: Vec<PathBuf>,
    #[serde(default)]
    dependencies: Vec<String>,
    /// ROS 2 edition for type hash format (defaults to "humble")
    #[serde(default = "default_ros_edition")]
    ros_edition: String,
    /// Explicit `nros-codegen.toml` path (CMake `CODEGEN_CONFIG`, RFC-0033).
    /// When absent, discovered by walking up from the output directory.
    #[serde(default)]
    codegen_config: Option<PathBuf>,
}

fn default_ros_edition() -> String {
    "humble".to_string()
}

/// Generate C bindings from an arguments file
///
/// This is called by the CMake `nano_ros_generate_interfaces()` function.
/// It reads a JSON arguments file and generates C code for each interface.
pub fn generate_c_from_args_file(config: GenerateCConfig) -> Result<()> {
    // Read and parse arguments file
    let args_content = std::fs::read_to_string(&config.args_file)
        .wrap_err_with(|| format!("Failed to read args file: {}", config.args_file.display()))?;

    let args: GenerateCArgs = serde_json::from_str(&args_content)
        .wrap_err_with(|| format!("Failed to parse args file: {}", config.args_file.display()))?;

    let edition = parse_ros_edition(&args.ros_edition)?;
    let type_hash = edition.type_hash();

    // Per-field capacity config (RFC-0033): explicit CMake CODEGEN_CONFIG, else
    // discover by walking up from the output dir (nested in the consumer tree).
    let resolver = rosidl_codegen::CapacityResolver::resolve_for(
        args.codegen_config.as_deref(),
        &args.output_dir,
        None,
    )?;

    if config.verbose {
        println!("Generating C bindings for package: {}", args.package_name);
        println!("Output directory: {}", args.output_dir.display());
        println!("Interface files: {:?}", args.interface_files);
        println!("Dependencies: {:?}", args.dependencies);
        println!("ROS edition: {:?}", edition);
    }

    // Create output directories
    let msg_dir = args.output_dir.join("msg");
    let srv_dir = args.output_dir.join("srv");
    let action_dir = args.output_dir.join("action");
    std::fs::create_dir_all(&msg_dir)?;
    std::fs::create_dir_all(&srv_dir)?;
    std::fs::create_dir_all(&action_dir)?;

    // Track generated files for umbrella header
    let mut msg_headers = Vec::new();
    let mut srv_headers = Vec::new();
    let mut action_headers = Vec::new();

    // Process each interface file
    for file_path in &args.interface_files {
        let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let file_name = file_path
            .file_stem()
            .and_then(|n| n.to_str())
            .ok_or_else(|| eyre!("Invalid interface file name: {}", file_path.display()))?;

        // Read file content
        let content = std::fs::read_to_string(file_path)
            .wrap_err_with(|| format!("Failed to read interface file: {}", file_path.display()))?;

        match extension {
            "msg" => {
                let parsed = rosidl_parser::parse_message(&content)
                    .wrap_err_with(|| format!("Failed to parse message: {}", file_name))?;

                let generated = rosidl_codegen::generate_c_message_package(
                    &args.package_name,
                    file_name,
                    &parsed,
                    type_hash,
                    &resolver,
                )
                .wrap_err_with(|| {
                    format!("Failed to generate C code for message: {}", file_name)
                })?;

                // Write header and source
                let header_path = msg_dir.join(&generated.header_name);
                let source_path = msg_dir.join(&generated.source_name);
                write_if_changed(&header_path, &generated.header)?;
                write_if_changed(&source_path, &generated.source)?;

                msg_headers.push(generated.header_name);

                if config.verbose {
                    println!("  Generated message: {}", file_name);
                }
            }
            "srv" => {
                let parsed = rosidl_parser::parse_service(&content)
                    .wrap_err_with(|| format!("Failed to parse service: {}", file_name))?;

                let generated = rosidl_codegen::generate_c_service_package(
                    &args.package_name,
                    file_name,
                    &parsed,
                    type_hash,
                    &resolver,
                )
                .wrap_err_with(|| {
                    format!("Failed to generate C code for service: {}", file_name)
                })?;

                // Write header and source
                let header_path = srv_dir.join(&generated.header_name);
                let source_path = srv_dir.join(&generated.source_name);
                write_if_changed(&header_path, &generated.header)?;
                write_if_changed(&source_path, &generated.source)?;

                srv_headers.push(generated.header_name);

                if config.verbose {
                    println!("  Generated service: {}", file_name);
                }
            }
            "action" => {
                let parsed = rosidl_parser::parse_action(&content)
                    .wrap_err_with(|| format!("Failed to parse action: {}", file_name))?;

                let generated = rosidl_codegen::generate_c_action_package(
                    &args.package_name,
                    file_name,
                    &parsed,
                    type_hash,
                    &resolver,
                )
                .wrap_err_with(|| format!("Failed to generate C code for action: {}", file_name))?;

                // Write header and source
                let header_path = action_dir.join(&generated.header_name);
                let source_path = action_dir.join(&generated.source_name);
                write_if_changed(&header_path, &generated.header)?;
                write_if_changed(&source_path, &generated.source)?;

                action_headers.push(generated.header_name);

                if config.verbose {
                    println!("  Generated action: {}", file_name);
                }
            }
            _ => {
                return Err(eyre!(
                    "Unknown interface file type: {} (expected .msg, .srv, or .action)",
                    file_path.display()
                ));
            }
        }
    }

    // Generate umbrella header
    let umbrella_header = generate_umbrella_header(
        &args.package_name,
        &msg_headers,
        &srv_headers,
        &action_headers,
        &args.dependencies,
    );
    let umbrella_path = args.output_dir.join(format!("{}.h", args.package_name));
    write_if_changed(&umbrella_path, umbrella_header)?;

    if config.verbose {
        println!("  Generated umbrella header: {}.h", args.package_name);
    }

    println!(
        "✓ Generated {} messages, {} services, {} actions for {}",
        msg_headers.len(),
        srv_headers.len(),
        action_headers.len(),
        args.package_name
    );

    Ok(())
}

/// Generate C bindings from package.xml dependencies (standalone mode).
///
/// This provides the same UX as `generate_from_package_xml()` (Rust) but
/// generates C code instead. It:
/// 1. Parses package.xml to find interface dependencies
/// 2. Resolves transitive dependencies via ament index / bundled interfaces
/// 3. Collects .msg/.srv/.action files for each interface package
/// 4. Generates C code (headers + sources) in the output directory
pub fn generate_c_from_package_xml(config: GenerateCStandaloneConfig) -> Result<()> {
    use package_xml::PackageXml;

    let edition = parse_ros_edition(&config.ros_edition)?;
    let type_hash = edition.type_hash();

    // Parse package.xml
    let pkg_xml = PackageXml::parse(&config.manifest_path)?;

    // Per-field capacity config (RFC-0033), discovered from the manifest dir.
    let manifest_dir = config
        .manifest_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let resolver = rosidl_codegen::CapacityResolver::resolve_for(
        config.codegen_config.as_deref(),
        manifest_dir,
        None,
    )?;

    if config.verbose {
        println!("Package: {} v{}", pkg_xml.name, pkg_xml.version);
        println!(
            "Dependencies from package.xml: {:?}",
            pkg_xml.all_dependencies()
        );
    }

    // Load ament index (with bundled interface fallback)
    let index = load_index_with_fallback(config.verbose)?;

    // Resolve all dependencies (including transitive)
    let all_deps =
        resolve_transitive_dependencies(&index, pkg_xml.all_dependencies(), config.verbose)?;

    // Filter to interface packages only
    let interface_packages = filter_interface_packages(&index, &all_deps, config.verbose)?;

    if interface_packages.is_empty() {
        println!("No interface packages found in dependencies");
        return Ok(());
    }

    println!(
        "Generating C bindings for {} interface packages...",
        interface_packages.len()
    );

    // Create output directory
    std::fs::create_dir_all(&config.output_dir)?;

    for (pkg_name, package) in &interface_packages {
        let pkg_output = config.output_dir.join(pkg_name);

        // Skip if exists and not forcing
        if pkg_output.exists() && !config.force {
            if config.verbose {
                println!("  Skipping {} (already exists)", pkg_name);
            }
            continue;
        }

        // Collect interface files from the package's share directory
        let mut interface_files = Vec::new();
        for subdir in &["msg", "srv", "action"] {
            let dir = package.share_dir.join(subdir);
            if dir.exists() {
                let mut entries: Vec<_> = std::fs::read_dir(&dir)?
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e == "msg" || e == "srv" || e == "action")
                            .unwrap_or(false)
                    })
                    .collect();
                entries.sort();
                interface_files.extend(entries);
            }
        }

        if interface_files.is_empty() {
            continue;
        }

        if config.verbose {
            println!("  Generating C bindings for {}...", pkg_name);
        }

        // Create output directories
        let msg_dir = pkg_output.join("msg");
        let srv_dir = pkg_output.join("srv");
        let action_dir = pkg_output.join("action");
        std::fs::create_dir_all(&msg_dir)?;
        std::fs::create_dir_all(&srv_dir)?;
        std::fs::create_dir_all(&action_dir)?;

        // Track generated files for umbrella header
        let mut msg_headers = Vec::new();
        let mut srv_headers = Vec::new();
        let mut action_headers = Vec::new();

        // Collect dependency names for umbrella header
        let pkg_xml_path = package.share_dir.join("package.xml");
        let pkg_deps: Vec<String> = if pkg_xml_path.exists() {
            if let Ok(dep_xml) = package_xml::PackageXml::parse(&pkg_xml_path) {
                dep_xml
                    .all_dependencies()
                    .iter()
                    .filter(|d| interface_packages.iter().any(|(n, _)| n == *d))
                    .cloned()
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        for file_path in &interface_files {
            let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let file_name = file_path
                .file_stem()
                .and_then(|n| n.to_str())
                .ok_or_else(|| eyre!("Invalid interface file name: {}", file_path.display()))?;
            let content = std::fs::read_to_string(file_path).wrap_err_with(|| {
                format!("Failed to read interface file: {}", file_path.display())
            })?;

            match extension {
                "msg" => {
                    let parsed = rosidl_parser::parse_message(&content)
                        .wrap_err_with(|| format!("Failed to parse message: {}", file_name))?;
                    let generated = rosidl_codegen::generate_c_message_package(
                        pkg_name, file_name, &parsed, type_hash, &resolver,
                    )?;
                    write_if_changed(msg_dir.join(&generated.header_name), &generated.header)?;
                    write_if_changed(msg_dir.join(&generated.source_name), &generated.source)?;
                    msg_headers.push(generated.header_name);
                }
                "srv" => {
                    let parsed = rosidl_parser::parse_service(&content)
                        .wrap_err_with(|| format!("Failed to parse service: {}", file_name))?;
                    let generated = rosidl_codegen::generate_c_service_package(
                        pkg_name, file_name, &parsed, type_hash, &resolver,
                    )?;
                    write_if_changed(srv_dir.join(&generated.header_name), &generated.header)?;
                    write_if_changed(srv_dir.join(&generated.source_name), &generated.source)?;
                    srv_headers.push(generated.header_name);
                }
                "action" => {
                    let parsed = rosidl_parser::parse_action(&content)
                        .wrap_err_with(|| format!("Failed to parse action: {}", file_name))?;
                    let generated = rosidl_codegen::generate_c_action_package(
                        pkg_name, file_name, &parsed, type_hash, &resolver,
                    )?;
                    write_if_changed(action_dir.join(&generated.header_name), &generated.header)?;
                    write_if_changed(action_dir.join(&generated.source_name), &generated.source)?;
                    action_headers.push(generated.header_name);
                }
                _ => {}
            }
        }

        // Generate umbrella header
        let umbrella = generate_umbrella_header(
            pkg_name,
            &msg_headers,
            &srv_headers,
            &action_headers,
            &pkg_deps,
        );
        write_if_changed(pkg_output.join(format!("{}.h", pkg_name)), umbrella)?;

        println!(
            "  ✓ {} ({} messages, {} services, {} actions)",
            pkg_name,
            msg_headers.len(),
            srv_headers.len(),
            action_headers.len()
        );
    }

    println!("✓ Generated C bindings in {}", config.output_dir.display());

    Ok(())
}

/// Configuration for resolving dependencies from package.xml
#[derive(Debug, Clone)]
pub struct ResolveDepsConfig {
    /// Path to package.xml
    pub package_xml: PathBuf,
    /// Path to output .cmake file
    pub output_cmake: PathBuf,
    /// Enable verbose output
    pub verbose: bool,
}

/// A resolved interface package with its files and direct dependencies
#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    /// Package name
    pub name: String,
    /// Absolute paths to interface files (.msg/.srv/.action)
    pub files: Vec<PathBuf>,
    /// Direct interface-package dependencies
    pub deps: Vec<String>,
}

/// Resolve interface dependencies from package.xml and output a CMake script.
///
/// Parses `package.xml`, resolves transitive deps via ament index (with bundled
/// fallback), filters to interface packages, topologically sorts them, and
/// writes a `.cmake` script that sets `_NROS_RESOLVED_PACKAGES` plus per-package
/// `_FILES` and `_DEPS` variables.
pub fn resolve_deps_from_package_xml(config: ResolveDepsConfig) -> Result<()> {
    use package_xml::PackageXml;

    let pkg_xml = PackageXml::parse(&config.package_xml)?;

    if config.verbose {
        eprintln!(
            "resolve-deps: {} deps from package.xml: {:?}",
            pkg_xml.name,
            pkg_xml.all_dependencies()
        );
    }

    let index = load_index_with_fallback(config.verbose)?;

    let all_deps =
        resolve_transitive_dependencies(&index, pkg_xml.all_dependencies(), config.verbose)?;

    let interface_packages = filter_interface_packages(&index, &all_deps, config.verbose)?;

    if interface_packages.is_empty() {
        // Write an empty resolved list
        write_if_changed(
            &config.output_cmake,
            "# Auto-generated by nros-codegen resolve-deps\nset(_NROS_RESOLVED_PACKAGES \"\")\n",
        )?;
        return Ok(());
    }

    // Build per-package info: files and direct deps (filtered to interface packages)
    let iface_names: HashSet<&str> = interface_packages.iter().map(|(n, _)| n.as_str()).collect();

    let mut pkg_map: HashMap<&str, ResolvedPackage> = HashMap::new();
    for (pkg_name, package) in &interface_packages {
        // Collect interface files
        let mut files = Vec::new();
        for subdir in &["msg", "srv", "action"] {
            let dir = package.share_dir.join(subdir);
            if dir.exists() {
                let mut entries: Vec<_> = std::fs::read_dir(&dir)?
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e == "msg" || e == "srv" || e == "action")
                    })
                    .collect();
                entries.sort();
                files.extend(entries);
            }
        }

        // Get direct deps that are also interface packages
        let pkg_xml_path = package.share_dir.join("package.xml");
        let direct_deps: Vec<String> = if pkg_xml_path.exists() {
            if let Ok(dep_xml) = PackageXml::parse(&pkg_xml_path) {
                dep_xml
                    .all_dependencies()
                    .iter()
                    .filter(|d| iface_names.contains(d.as_str()))
                    .cloned()
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        pkg_map.insert(
            pkg_name.as_str(),
            ResolvedPackage {
                name: pkg_name.clone(),
                files,
                deps: direct_deps,
            },
        );
    }

    // Topological sort (Kahn's algorithm) — dependencies first
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    for (name, pkg) in &pkg_map {
        in_degree.entry(name).or_insert(0);
        for dep in &pkg.deps {
            if let Some(dep_str) = iface_names.get(dep.as_str()) {
                *in_degree.entry(name).or_insert(0) += 1;
                dependents.entry(dep_str).or_default().push(name);
            }
        }
    }

    let mut queue: Vec<&str> = Vec::new();
    for (name, deg) in &in_degree {
        if *deg == 0 {
            queue.push(name);
        }
    }
    queue.sort(); // deterministic
    let mut topo_order = Vec::new();

    while let Some(name) = queue.pop() {
        topo_order.push(name);
        if let Some(deps) = dependents.get(name) {
            for dep in deps {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        // Insert in sorted position for determinism
                        let pos = queue.partition_point(|x| *x > *dep);
                        queue.insert(pos, dep);
                    }
                }
            }
        }
    }

    // Generate CMake script
    let mut cmake = String::new();
    cmake.push_str("# Auto-generated by nros-codegen resolve-deps\n");

    let pkg_list: Vec<&str> = topo_order.to_vec();
    cmake.push_str(&format!(
        "set(_NROS_RESOLVED_PACKAGES \"{}\")\n",
        pkg_list.join(";")
    ));

    for &name in &topo_order {
        let pkg = &pkg_map[name];
        let files: Vec<String> = pkg.files.iter().map(|p| p.display().to_string()).collect();
        cmake.push_str(&format!(
            "set(_NROS_RESOLVED_{}_FILES \"{}\")\n",
            name,
            files.join(";")
        ));
        cmake.push_str(&format!(
            "set(_NROS_RESOLVED_{}_DEPS \"{}\")\n",
            name,
            pkg.deps.join(";")
        ));
    }

    if let Some(parent) = config.output_cmake.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_if_changed(&config.output_cmake, cmake)?;

    if config.verbose {
        eprintln!(
            "resolve-deps: wrote {} packages to {}",
            topo_order.len(),
            config.output_cmake.display()
        );
    }

    Ok(())
}

/// Generate C++ bindings from an arguments file
///
/// This is called by the CMake `nano_ros_generate_interfaces(LANGUAGE CPP)` function.
/// It reads a JSON arguments file and generates C++ headers + Rust FFI glue.
pub fn generate_cpp_from_args_file(config: GenerateCppConfig) -> Result<()> {
    // Read and parse arguments file (same format as C)
    let args_content = std::fs::read_to_string(&config.args_file)
        .wrap_err_with(|| format!("Failed to read args file: {}", config.args_file.display()))?;

    let args: GenerateCArgs = serde_json::from_str(&args_content)
        .wrap_err_with(|| format!("Failed to parse args file: {}", config.args_file.display()))?;

    let edition = parse_ros_edition(&args.ros_edition)?;
    let type_hash = edition.type_hash();

    // Per-field capacity config (RFC-0033): explicit CMake CODEGEN_CONFIG, else
    // discover by walking up from the output dir.
    let resolver = rosidl_codegen::CapacityResolver::resolve_for(
        args.codegen_config.as_deref(),
        &args.output_dir,
        None,
    )?;

    if config.verbose {
        println!("Generating C++ bindings for package: {}", args.package_name);
        println!("Output directory: {}", args.output_dir.display());
        println!("Interface files: {:?}", args.interface_files);
        println!("ROS edition: {:?}", edition);
    }

    // Create output directories
    let msg_dir = args.output_dir.join("msg");
    let srv_dir = args.output_dir.join("srv");
    let action_dir = args.output_dir.join("action");
    std::fs::create_dir_all(&msg_dir)?;
    std::fs::create_dir_all(&srv_dir)?;
    std::fs::create_dir_all(&action_dir)?;

    // Track generated files
    let mut msg_headers = Vec::new();
    let mut srv_headers = Vec::new();
    let mut action_headers = Vec::new();
    let mut ffi_rs_files = Vec::new();

    // Process each interface file
    for file_path in &args.interface_files {
        let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let file_name = file_path
            .file_stem()
            .and_then(|n| n.to_str())
            .ok_or_else(|| eyre!("Invalid interface file name: {}", file_path.display()))?;

        // Read file content
        let content = std::fs::read_to_string(file_path)
            .wrap_err_with(|| format!("Failed to read interface file: {}", file_path.display()))?;

        match extension {
            "msg" => {
                let parsed = rosidl_parser::parse_message(&content)
                    .wrap_err_with(|| format!("Failed to parse message: {}", file_name))?;

                let generated = rosidl_codegen::generate_cpp_message_package(
                    &args.package_name,
                    file_name,
                    &parsed,
                    type_hash,
                    &resolver,
                )
                .wrap_err_with(|| {
                    format!("Failed to generate C++ code for message: {}", file_name)
                })?;

                // Write header and FFI Rust glue
                write_if_changed(msg_dir.join(&generated.header_name), &generated.header)?;
                write_if_changed(msg_dir.join(&generated.ffi_rs_name), &generated.ffi_rs)?;

                // Phase 123.B.8 — emit ROS-style alias header
                // `msg/<snake_name>.hpp` that #includes the canonical
                // prefixed `<package>_msg_<snake_name>.hpp`. Lets
                // users write `#include <std_msgs/msg/int32.hpp>`
                // instead of the prefixed flat form.
                write_b8_alias_header(
                    &msg_dir.join(format!("{}.hpp", to_snake_case(file_name))),
                    &generated.header_name,
                )?;

                msg_headers.push(generated.header_name);
                ffi_rs_files.push(format!("msg/{}", generated.ffi_rs_name));

                if config.verbose {
                    println!("  Generated message: {}", file_name);
                }
            }
            "srv" => {
                let parsed = rosidl_parser::parse_service(&content)
                    .wrap_err_with(|| format!("Failed to parse service: {}", file_name))?;

                let generated = rosidl_codegen::generate_cpp_service_package(
                    &args.package_name,
                    file_name,
                    &parsed,
                    type_hash,
                    &resolver,
                )
                .wrap_err_with(|| {
                    format!("Failed to generate C++ code for service: {}", file_name)
                })?;

                // Write header and FFI Rust glue
                write_if_changed(srv_dir.join(&generated.header_name), &generated.header)?;
                write_if_changed(
                    srv_dir.join(&generated.request_ffi_rs_name),
                    &generated.request_ffi_rs,
                )?;
                write_if_changed(
                    srv_dir.join(&generated.response_ffi_rs_name),
                    &generated.response_ffi_rs,
                )?;

                // Phase 123.B.8 — ROS-style alias header.
                write_b8_alias_header(
                    &srv_dir.join(format!("{}.hpp", to_snake_case(file_name))),
                    &generated.header_name,
                )?;

                srv_headers.push(generated.header_name);
                ffi_rs_files.push(format!("srv/{}", generated.request_ffi_rs_name));
                ffi_rs_files.push(format!("srv/{}", generated.response_ffi_rs_name));

                if config.verbose {
                    println!("  Generated service: {}", file_name);
                }
            }
            "action" => {
                let parsed = rosidl_parser::parse_action(&content)
                    .wrap_err_with(|| format!("Failed to parse action: {}", file_name))?;

                let generated = rosidl_codegen::generate_cpp_action_package(
                    &args.package_name,
                    file_name,
                    &parsed,
                    type_hash,
                    &resolver,
                )
                .wrap_err_with(|| {
                    format!("Failed to generate C++ code for action: {}", file_name)
                })?;

                // Write header and FFI Rust glue
                write_if_changed(action_dir.join(&generated.header_name), &generated.header)?;
                write_if_changed(
                    action_dir.join(&generated.goal_ffi_rs_name),
                    &generated.goal_ffi_rs,
                )?;
                write_if_changed(
                    action_dir.join(&generated.result_ffi_rs_name),
                    &generated.result_ffi_rs,
                )?;
                write_if_changed(
                    action_dir.join(&generated.feedback_ffi_rs_name),
                    &generated.feedback_ffi_rs,
                )?;

                // Phase 123.B.8 — ROS-style alias header.
                write_b8_alias_header(
                    &action_dir.join(format!("{}.hpp", to_snake_case(file_name))),
                    &generated.header_name,
                )?;

                action_headers.push(generated.header_name);
                ffi_rs_files.push(format!("action/{}", generated.goal_ffi_rs_name));
                ffi_rs_files.push(format!("action/{}", generated.result_ffi_rs_name));
                ffi_rs_files.push(format!("action/{}", generated.feedback_ffi_rs_name));

                if config.verbose {
                    println!("  Generated action: {}", file_name);
                }
            }
            _ => {
                return Err(eyre!(
                    "Unknown interface file type: {} (expected .msg, .srv, or .action)",
                    file_path.display()
                ));
            }
        }
    }

    // Generate C++ umbrella header
    let umbrella_hpp = generate_cpp_umbrella_header(
        &args.package_name,
        &msg_headers,
        &srv_headers,
        &action_headers,
        &args.dependencies,
    );
    let umbrella_path = args.output_dir.join(format!("{}.hpp", args.package_name));
    write_if_changed(&umbrella_path, umbrella_hpp)?;

    // Generate Rust FFI mod.rs
    let mod_rs = generate_ffi_mod_rs(&ffi_rs_files);
    let mod_rs_path = args.output_dir.join("mod.rs");
    write_if_changed(&mod_rs_path, mod_rs)?;

    if config.verbose {
        println!("  Generated umbrella header: {}.hpp", args.package_name);
        println!("  Generated FFI mod.rs ({} modules)", ffi_rs_files.len());
    }

    println!(
        "✓ Generated {} messages, {} services, {} actions for {}",
        msg_headers.len(),
        srv_headers.len(),
        action_headers.len(),
        args.package_name
    );

    Ok(())
}

/// Phase 123.B.8 — emit a thin ROS-2-conventional alias header
/// that forwards to the canonical prefixed header in the same
/// directory. Example: `msg/int32.hpp` containing
/// `#include "std_msgs_msg_int32.hpp"`.
///
/// Lets users write `#include <std_msgs/msg/int32.hpp>` —
/// matches rclcpp's include style — without the codegen having
/// to abandon the unique-prefix naming scheme (which avoids
/// collisions when two packages declare a `Header.msg`).
fn write_b8_alias_header(alias_path: &Path, canonical_filename: &str) -> Result<()> {
    let guard = alias_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("nros_alias")
        .to_uppercase()
        .replace('-', "_");
    let content = format!(
        "#ifndef NROS_ALIAS_{guard}_HPP_\n\
         #define NROS_ALIAS_{guard}_HPP_\n\
         // Auto-generated ROS-style alias header (Phase 123.B.8).\n\
         // Forwards to the canonical prefixed header.\n\
         #include \"{canonical_filename}\"\n\
         #endif // NROS_ALIAS_{guard}_HPP_\n"
    );
    write_if_changed(alias_path, content)?;
    Ok(())
}

/// Generate C++ umbrella header
fn generate_cpp_umbrella_header(
    package_name: &str,
    msg_headers: &[String],
    srv_headers: &[String],
    action_headers: &[String],
    dependencies: &[String],
) -> String {
    let guard_name = format!("{}_HPP", package_name.to_uppercase().replace('-', "_"));

    let mut content = String::new();
    content.push_str(&format!("#ifndef {}\n", guard_name));
    content.push_str(&format!("#define {}\n\n", guard_name));

    content.push_str("#include \"nros/fixed_string.hpp\"\n");
    content.push_str("#include \"nros/fixed_sequence.hpp\"\n\n");

    if !dependencies.is_empty() {
        content.push_str("// Dependencies\n");
        for dep in dependencies {
            content.push_str(&format!("#include <{}/{}.hpp>\n", dep, dep));
        }
        content.push('\n');
    }

    if !msg_headers.is_empty() {
        content.push_str("// Messages\n");
        for header in msg_headers {
            content.push_str(&format!("#include \"msg/{}\"\n", header));
        }
        content.push('\n');
    }

    if !srv_headers.is_empty() {
        content.push_str("// Services\n");
        for header in srv_headers {
            content.push_str(&format!("#include \"srv/{}\"\n", header));
        }
        content.push('\n');
    }

    if !action_headers.is_empty() {
        content.push_str("// Actions\n");
        for header in action_headers {
            content.push_str(&format!("#include \"action/{}\"\n", header));
        }
        content.push('\n');
    }

    content.push_str(&format!("#endif  // {}\n", guard_name));
    content
}

/// Generate Rust FFI mod.rs that includes all FFI modules
fn generate_ffi_mod_rs(ffi_files: &[String]) -> String {
    let mut content = String::new();
    content.push_str("// Auto-generated — do not edit\n");
    content.push_str("// Includes all C++ FFI glue modules\n\n");

    for file in ffi_files {
        // Convert "msg/std_msgs_msg_string_ffi.rs" → include path
        content.push_str(&format!("#[path = \"{}\"]\n", file));
        // Module name: strip path and .rs extension
        let mod_name = file
            .rsplit('/')
            .next()
            .unwrap_or(file)
            .trim_end_matches(".rs");
        content.push_str(&format!("mod {};\n\n", mod_name));
    }

    content
}

/// Generate umbrella header that includes all generated headers
fn generate_umbrella_header(
    package_name: &str,
    msg_headers: &[String],
    srv_headers: &[String],
    action_headers: &[String],
    dependencies: &[String],
) -> String {
    let guard_name = format!("{}_H", package_name.to_uppercase().replace('-', "_"));

    let mut content = String::new();

    // Header guard
    content.push_str(&format!("#ifndef {}\n", guard_name));
    content.push_str(&format!("#define {}\n\n", guard_name));

    // Include nros core types (modular header)
    content.push_str("#include <nros/types.h>\n\n");

    // Include dependency headers. Each dependency's umbrella header lives at
    // `<pkg>/<pkg>.h` (sibling directory to this one), so emit the full
    // relative path rather than `<pkg.h>` alone — the latter would only
    // work if the build system put every package directory directly on
    // the include path, which it doesn't.
    if !dependencies.is_empty() {
        content.push_str("// Dependencies\n");
        for dep in dependencies {
            content.push_str(&format!("#include <{}/{}.h>\n", dep, dep));
        }
        content.push('\n');
    }

    // Include message headers
    if !msg_headers.is_empty() {
        content.push_str("// Messages\n");
        for header in msg_headers {
            content.push_str(&format!("#include \"msg/{}\"\n", header));
        }
        content.push('\n');
    }

    // Include service headers
    if !srv_headers.is_empty() {
        content.push_str("// Services\n");
        for header in srv_headers {
            content.push_str(&format!("#include \"srv/{}\"\n", header));
        }
        content.push('\n');
    }

    // Include action headers
    if !action_headers.is_empty() {
        content.push_str("// Actions\n");
        for header in action_headers {
            content.push_str(&format!("#include \"action/{}\"\n", header));
        }
        content.push('\n');
    }

    // End header guard
    content.push_str(&format!("#endif  // {}\n", guard_name));

    content
}

/// Clean generated bindings
pub fn clean_generated(output_dir: &Path, clean_config: bool, verbose: bool) -> Result<()> {
    // Remove output directory
    if output_dir.exists() {
        std::fs::remove_dir_all(output_dir)
            .wrap_err_with(|| format!("Failed to remove {}", output_dir.display()))?;
        if verbose {
            println!("Removed {}", output_dir.display());
        }
    }

    // Remove .cargo/config.toml if requested
    if clean_config {
        let config_path = Path::new(".cargo").join("config.toml");
        if config_path.exists() {
            std::fs::remove_file(&config_path)
                .wrap_err_with(|| format!("Failed to remove {}", config_path.display()))?;
            if verbose {
                println!("Removed {}", config_path.display());
            }
        }
    }

    Ok(())
}

/// Clean bindings in a project directory (for colcon integration)
///
/// This removes the target/ros2_bindings directory and related cache.
pub fn clean_bindings(project_root: &Path, verbose: bool) -> Result<()> {
    let bindings_dir = project_root.join("target").join("ros2_bindings");
    if bindings_dir.exists() {
        std::fs::remove_dir_all(&bindings_dir)
            .wrap_err_with(|| format!("Failed to remove {}", bindings_dir.display()))?;
        if verbose {
            println!("Removed {}", bindings_dir.display());
        }
    }
    Ok(())
}

/// Install package binaries and libraries to ament layout (for colcon integration)
pub fn install_to_ament(config: InstallConfig) -> Result<()> {
    use crate::ament_installer::{AmentInstaller, is_library_package};
    use cargo_metadata::MetadataCommand;
    use std::env;

    if config.verbose {
        eprintln!("Installing package to ament layout...");
    }

    // Save current directory and change to project root
    let original_dir = env::current_dir()?;
    env::set_current_dir(&config.project_root)?;

    // Read package metadata
    let mut metadata_cmd = MetadataCommand::new();

    // Use build_base provided by colcon to locate config file
    let config_file = config.build_base.join("ros2_cargo_config.toml");

    if config.verbose {
        eprintln!("Using config file: {}", config_file.display());
    }

    if config_file.exists() {
        metadata_cmd.other_options(vec![
            "--config".to_string(),
            config_file.display().to_string(),
        ]);
    }

    let metadata = metadata_cmd
        .exec()
        .wrap_err("Failed to read Cargo metadata")?;

    let root_package = metadata
        .root_package()
        .ok_or_else(|| eyre!("No root package found in Cargo.toml"))?;

    let package_name = root_package.name.clone();
    let target_dir = metadata.target_directory.clone().into_std_path_buf();
    let is_lib_only = is_library_package(&config.project_root)?;

    let installer = AmentInstaller::new(
        config.install_base.clone(),
        package_name,
        config.project_root.clone(),
        target_dir,
        config.verbose,
        config.profile.clone(),
    );

    let result = installer.install(is_lib_only);

    env::set_current_dir(original_dir)?;

    result
}
