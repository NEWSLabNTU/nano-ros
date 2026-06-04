//! Package discovery for workspace and ament packages
//!
//! This module provides functions to discover Cargo packages in the workspace
//! and installed ament packages, similar to the Python colcon-cargo logic.

use eyre::Result;
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

/// Discover Cargo packages in the workspace directory
///
/// Recursively walks the workspace to find all Cargo.toml files,
/// extracting package names and paths. Skips build/ and install/ directories.
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `build_base` - Build directory to skip (e.g., "build/")
/// * `install_base` - Install directory to skip (e.g., "install/")
///
/// # Returns
/// HashMap of package name -> absolute path to package directory
pub fn discover_workspace_packages(
    workspace_root: &Path,
    build_base: Option<&Path>,
    install_base: Option<&Path>,
) -> Result<HashMap<String, PathBuf>> {
    let mut packages = HashMap::new();

    // Walk the workspace directory
    fn walk_dir(
        dir: &Path,
        build_base: Option<&Path>,
        install_base: Option<&Path>,
        packages: &mut HashMap<String, PathBuf>,
    ) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        // Check if we should skip this directory
        // Skip build/ directories
        if let Some(build_base) = build_base
            && (dir == build_base || dir.starts_with(build_base))
        {
            return Ok(());
        }

        // Skip install/ directories (identified by setup.sh)
        if let Some(install_base) = install_base
            && (dir == install_base || dir.starts_with(install_base))
        {
            return Ok(());
        }

        // Check for setup.sh (indicates install directory)
        if dir.join("setup.sh").exists() {
            return Ok(());
        }

        // Check for COLCON_IGNORE (build directories have this)
        if dir.join("COLCON_IGNORE").exists() {
            return Ok(());
        }

        // Check if this directory has a Cargo.toml
        let cargo_toml_path = dir.join("Cargo.toml");
        if cargo_toml_path.exists()
            && let Ok(content) = fs::read_to_string(&cargo_toml_path)
            && let Some(name) = extract_package_name(&content)
        {
            packages.insert(name, dir.to_path_buf());
        }

        // Recursively walk subdirectories
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type()
                    && file_type.is_dir()
                {
                    walk_dir(&entry.path(), build_base, install_base, packages)?;
                }
            }
        }

        Ok(())
    }

    walk_dir(workspace_root, build_base, install_base, &mut packages)?;
    Ok(packages)
}

/// Discover installed ament packages with Rust bindings
///
/// Scans AMENT_PREFIX_PATH to find packages with rust_packages resource index.
/// Returns a mapping of package names to their Rust binding directories.
///
/// # Returns
/// HashMap of package name -> path to <prefix>/share/<package>/rust/
pub fn discover_installed_ament_packages() -> Result<HashMap<String, PathBuf>> {
    let mut packages = HashMap::new();

    // Get AMENT_PREFIX_PATH from environment
    let ament_prefix_path = env::var("AMENT_PREFIX_PATH").unwrap_or_default();

    if ament_prefix_path.is_empty() {
        // Not an error - just means no ROS 2 is sourced
        return Ok(packages);
    }

    // Split by path separator
    let prefixes: Vec<&str> = ament_prefix_path
        .split(if cfg!(windows) { ';' } else { ':' })
        .collect();

    for prefix in prefixes {
        let prefix_path = Path::new(prefix);

        // Check for rust_packages resource index
        let rust_packages_dir = prefix_path
            .join("share")
            .join("ament_index")
            .join("resource_index")
            .join("rust_packages");

        if rust_packages_dir.exists() {
            // Read all package names from this directory
            if let Ok(entries) = fs::read_dir(&rust_packages_dir) {
                for entry in entries.flatten() {
                    let package_name = entry.file_name().to_string_lossy().to_string();

                    // The rust bindings are located at <prefix>/share/<package>/rust/
                    let rust_binding_path =
                        prefix_path.join("share").join(&package_name).join("rust");

                    if rust_binding_path.exists() {
                        packages.insert(package_name, rust_binding_path);
                    }
                }
            }
        }
    }

    Ok(packages)
}

/// Discover interface packages from workspace install directory
///
/// Scans the install directory for packages with .msg/.srv/.action files.
/// Returns a mapping of package name -> share directory path.
///
/// # Colcon Dependency Ordering Guarantee
///
/// This function discovers from the `install/` directory, which works correctly
/// because colcon **guarantees** topological dependency ordering:
///
/// 1. Dependencies are ALWAYS built and installed BEFORE dependents
/// 2. When building package B that depends on package A, package A is already in install/A/
/// 3. The install/ directory is created before any package builds
///
/// Example workflow:
/// ```text
/// colcon build
///   └─> Builds robot_interfaces (no dependencies) → install/robot_interfaces/
///   └─> Builds robot_controller (depends on robot_interfaces)
///       └─> cargo ros2 ament-build
///           └─> Discovers robot_interfaces from install/robot_interfaces/ ✓
///           └─> Generates bindings ✓
///           └─> Build succeeds ✓
/// ```
///
/// This is colcon's core design: packages discover dependencies from `install/`, not `src/`.
/// See: <https://colcon.readthedocs.io/en/released/developer/environment.html>
///
/// # Edge Cases
///
/// The only scenario where this returns empty is when:
/// - User bypasses colcon and runs `cargo ros2 build` directly in a package subdirectory
/// - This is expected behavior - users should use colcon for workspace builds
///
/// # Arguments
/// * `install_base` - Install directory path (e.g., "install/")
///
/// # Returns
/// HashMap of package name -> absolute path to package share directory
pub fn discover_interface_packages_from_workspace(
    install_base: &Path,
) -> Result<HashMap<String, PathBuf>> {
    let mut packages = HashMap::new();

    // If install/ doesn't exist, we're either:
    // 1. Building the first package in the workspace (no dependencies yet), or
    // 2. User is building standalone (not via colcon)
    // In both cases, returning empty is correct.
    if !install_base.exists() {
        return Ok(packages);
    }

    // Iterate through packages in install directory
    if let Ok(entries) = fs::read_dir(install_base) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                continue;
            }

            let package_name = entry.file_name().to_string_lossy().to_string();
            let share_dir = entry.path().join("share").join(&package_name);

            if !share_dir.exists() {
                continue;
            }

            // Check if package has interface files
            let has_interfaces = share_dir.join("msg").exists()
                || share_dir.join("srv").exists()
                || share_dir.join("action").exists();

            if has_interfaces {
                packages.insert(package_name, share_dir);
            }
        }
    }

    Ok(packages)
}

/// Extract package name from Cargo.toml content
///
/// Simple line-by-line parser to find "name = ..." in [package] section
fn extract_package_name(cargo_toml: &str) -> Option<String> {
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name")
            && let Some(eq_pos) = trimmed.find('=')
        {
            let value = &trimmed[eq_pos + 1..].trim();
            let value = value.trim_matches('"').trim_matches('\'');
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_extract_package_name() {
        let cargo_toml = r#"
[package]
name = "my_package"
version = "0.1.0"
"#;
        assert_eq!(
            extract_package_name(cargo_toml),
            Some("my_package".to_string())
        );
    }

    #[test]
    fn test_discover_workspace_packages() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Create a test package
        let pkg_dir = workspace.join("test_pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(
            pkg_dir.join("Cargo.toml"),
            r#"[package]
name = "test_pkg"
version = "0.1.0"
"#,
        )
        .unwrap();

        // Create build directory (should be skipped)
        let build_dir = workspace.join("build");
        fs::create_dir_all(&build_dir).unwrap();
        fs::write(build_dir.join("COLCON_IGNORE"), "").unwrap();

        let pkg_in_build = build_dir.join("test_pkg");
        fs::create_dir_all(&pkg_in_build).unwrap();
        fs::write(
            pkg_in_build.join("Cargo.toml"),
            r#"[package]
name = "should_be_skipped"
version = "0.1.0"
"#,
        )
        .unwrap();

        // Discover packages
        let packages = discover_workspace_packages(workspace, Some(&build_dir), None).unwrap();

        assert_eq!(packages.len(), 1);
        assert!(packages.contains_key("test_pkg"));
        assert!(!packages.contains_key("should_be_skipped"));
    }

    #[test]
    fn test_discover_installed_ament_packages_empty() {
        // When AMENT_PREFIX_PATH is not set, should return empty
        unsafe { env::remove_var("AMENT_PREFIX_PATH") };
        let packages = discover_installed_ament_packages().unwrap();
        assert!(packages.is_empty());
    }
}
