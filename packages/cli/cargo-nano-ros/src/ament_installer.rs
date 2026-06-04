//! Ament installation support for cargo-ros2
//!
//! This module handles installing Rust packages to the ament index structure,
//! similar to cargo-ament-build. It creates the necessary markers, installs
//! source files, binaries, and metadata.

use eyre::{Result, WrapErr};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Ament installer for creating ament-compatible installations
pub struct AmentInstaller {
    /// Install base directory (e.g., install/package_name)
    install_base: PathBuf,
    /// Package name
    package_name: String,
    /// Project root directory
    project_root: PathBuf,
    /// Target directory (from cargo metadata - handles workspace builds)
    target_dir: PathBuf,
    /// Verbose output
    verbose: bool,
    /// Build profile (debug or release)
    profile: String,
}

impl AmentInstaller {
    /// Create a new ament installer
    pub fn new(
        install_base: PathBuf,
        package_name: String,
        project_root: PathBuf,
        target_dir: PathBuf,
        verbose: bool,
        profile: String,
    ) -> Self {
        Self {
            install_base,
            package_name,
            project_root,
            target_dir,
            verbose,
            profile,
        }
    }

    /// Run the complete installation process
    pub fn install(&self, is_library: bool) -> Result<()> {
        if self.verbose {
            eprintln!(
                "Installing {} to {}",
                self.package_name,
                self.install_base.display()
            );
        }

        // Create directory structure
        self.create_directories()?;

        // Create ament index markers
        self.create_markers()?;

        // Create colcon marker
        self.create_colcon_marker()?;

        // Install source files
        self.install_source_files()?;

        // Install binaries (if not a pure library)
        if !is_library {
            self.install_binaries()?;
        }

        // Install metadata
        self.install_metadata()?;

        // Install additional files from [package.metadata.ros]
        self.install_metadata_ros_files()?;

        // Create colcon DSV files (package.dsv and local_setup.dsv)
        self.create_dsv_files()?;

        if self.verbose {
            eprintln!("✓ Installation complete!");
        }

        Ok(())
    }

    /// Create necessary directory structure
    fn create_directories(&self) -> Result<()> {
        let dirs = [
            self.lib_dir(),
            self.share_dir(),
            self.ament_index_dir(),
            self.rust_source_dir(),
        ];

        for dir in &dirs {
            fs::create_dir_all(dir)
                .wrap_err_with(|| format!("Failed to create directory: {}", dir.display()))?;
        }

        Ok(())
    }

    /// Create ament index markers
    fn create_markers(&self) -> Result<()> {
        // Create package marker
        let marker_file = self
            .ament_index_dir()
            .join("resource_index")
            .join("packages")
            .join(&self.package_name);

        fs::create_dir_all(marker_file.parent().unwrap())?;
        fs::write(&marker_file, "")?;

        if self.verbose {
            eprintln!("  Created marker: {}", marker_file.display());
        }

        // Create package type marker (Rust)
        let package_type_file = self
            .ament_index_dir()
            .join("resource_index")
            .join("package_type")
            .join(&self.package_name);

        fs::create_dir_all(package_type_file.parent().unwrap())?;
        fs::write(&package_type_file, "rust")?;

        if self.verbose {
            eprintln!(
                "  Created package type marker: {}",
                package_type_file.display()
            );
        }

        Ok(())
    }

    /// Create colcon marker file
    /// This marker file is required for colcon to discover the package
    fn create_colcon_marker(&self) -> Result<()> {
        let colcon_marker_dir = self
            .install_base
            .join("share")
            .join("colcon-core")
            .join("packages");

        fs::create_dir_all(&colcon_marker_dir)?;

        let colcon_marker_file = colcon_marker_dir.join(&self.package_name);

        // Parse package.xml to get dependencies
        let dependencies = self.extract_dependencies();
        let deps_string = dependencies.join(":");

        fs::write(&colcon_marker_file, deps_string)?;

        if self.verbose {
            eprintln!("  Created colcon marker: {}", colcon_marker_file.display());
        }

        Ok(())
    }

    /// Extract runtime dependencies from package.xml
    fn extract_dependencies(&self) -> Vec<String> {
        let package_xml_path = self.project_root.join("package.xml");

        if !package_xml_path.exists() {
            return Vec::new();
        }

        let xml_content = match fs::read_to_string(&package_xml_path) {
            Ok(content) => content,
            Err(_) => return Vec::new(),
        };

        let mut dependencies = Vec::new();

        // Simple XML parsing for <depend> tags
        for line in xml_content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<depend>") && trimmed.ends_with("</depend>") {
                let dep = trimmed
                    .trim_start_matches("<depend>")
                    .trim_end_matches("</depend>")
                    .trim();
                dependencies.push(dep.to_string());
            }
        }

        dependencies
    }

    /// Create colcon DSV files
    /// These files tell colcon what environment scripts to source
    fn create_dsv_files(&self) -> Result<()> {
        let share_pkg_dir = self.share_dir();

        // Create package.dsv
        let package_dsv = share_pkg_dir.join("package.dsv");
        let package_dsv_content = format!(
            "source;share/{}/hook/ament_prefix_path.ps1\n\
             source;share/{}/hook/ament_prefix_path.dsv\n\
             source;share/{}/hook/ament_prefix_path.sh\n",
            self.package_name, self.package_name, self.package_name
        );
        fs::write(&package_dsv, package_dsv_content)?;

        if self.verbose {
            eprintln!("  Created package.dsv");
        }

        // Create local_setup.dsv (points to package.dsv for simplicity)
        let local_setup_dsv = share_pkg_dir.join("local_setup.dsv");
        fs::write(&local_setup_dsv, "")?; // Empty for now, colcon will handle it

        if self.verbose {
            eprintln!("  Created local_setup.dsv");
        }

        Ok(())
    }

    /// Install source files to share directory
    fn install_source_files(&self) -> Result<()> {
        let source_files = [("Cargo.toml", false), ("Cargo.lock", false), ("src", true)];

        let dest_dir = self.rust_source_dir();

        for (name, is_dir) in &source_files {
            let source = self.project_root.join(name);
            let dest = dest_dir.join(name);

            if !source.exists() {
                continue;
            }

            if *is_dir {
                self.copy_dir_recursive(&source, &dest)?;
            } else {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&source, &dest).wrap_err_with(|| {
                    format!("Failed to copy {} to {}", source.display(), dest.display())
                })?;
            }

            if self.verbose {
                eprintln!("  Installed: {}", name);
            }
        }

        Ok(())
    }

    /// Install binaries to lib directory
    fn install_binaries(&self) -> Result<()> {
        let target_dir = self.target_dir.join(&self.profile);
        let cargo_toml_path = self.project_root.join("Cargo.toml");

        // Parse Cargo.toml to find binary names
        let cargo_toml =
            fs::read_to_string(&cargo_toml_path).wrap_err("Failed to read Cargo.toml")?;

        let binaries = self.extract_binary_names(&cargo_toml);

        if binaries.is_empty() {
            if self.verbose {
                eprintln!("  No binaries to install (library package)");
            }
            return Ok(());
        }

        let dest_dir = self.lib_dir().join(&self.package_name);
        fs::create_dir_all(&dest_dir)?;

        for binary_name in binaries {
            let source = target_dir.join(&binary_name);
            let dest = dest_dir.join(&binary_name);

            if source.exists() {
                fs::copy(&source, &dest)
                    .wrap_err_with(|| format!("Failed to copy binary: {}", binary_name))?;

                // Make executable on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = fs::metadata(&dest)?.permissions();
                    perms.set_mode(0o755);
                    fs::set_permissions(&dest, perms)?;
                }

                if self.verbose {
                    eprintln!("  Installed binary: {}", binary_name);
                }
            } else if self.verbose {
                eprintln!(
                    "  Warning: Binary not found: {} (did you run with --release?)",
                    binary_name
                );
            }
        }

        Ok(())
    }

    /// Install metadata files
    fn install_metadata(&self) -> Result<()> {
        let package_xml_source = self.project_root.join("package.xml");
        let package_xml_dest = self.share_dir().join("package.xml");

        if package_xml_source.exists() {
            fs::copy(&package_xml_source, &package_xml_dest)
                .wrap_err("Failed to copy package.xml")?;

            if self.verbose {
                eprintln!("  Installed: package.xml");
            }
        } else if self.verbose {
            eprintln!("  Note: No package.xml found (optional)");
        }

        Ok(())
    }

    /// Install additional files from [package.metadata.ros] in Cargo.toml
    ///
    /// Supports both directories and individual files:
    /// - install_to_share: Array of paths to copy to install/<pkg>/share/<pkg>/
    /// - install_to_include: Array of paths to copy to install/<pkg>/include/<pkg>/
    /// - install_to_lib: Array of paths to copy to install/<pkg>/lib/<pkg>/
    ///
    /// Examples:
    /// ```toml
    /// [package.metadata.ros]
    /// install_to_share = ["launch", "config", "README.md"]  # Directories and files
    /// install_to_include = ["include"]
    /// install_to_lib = ["scripts"]
    /// ```
    ///
    /// Behavior:
    /// - Directories: Copied recursively, name preserved (e.g., "launch" → share/<pkg>/launch/)
    /// - Individual files: Filename only preserved (e.g., "config/params.yaml" → share/<pkg>/params.yaml)
    /// - Missing paths: Build fails with error
    fn install_metadata_ros_files(&self) -> Result<()> {
        use toml::Value;

        let cargo_toml_path = self.project_root.join("Cargo.toml");
        let cargo_toml_content = match fs::read_to_string(&cargo_toml_path) {
            Ok(content) => content,
            Err(_) => return Ok(()), // No Cargo.toml, nothing to do
        };

        let cargo_toml: Value = match cargo_toml_content.parse() {
            Ok(value) => value,
            Err(_) => return Ok(()), // Can't parse, skip
        };

        // Navigate to [package.metadata.ros]
        let metadata_ros = match cargo_toml
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("ros"))
        {
            Some(Value::Table(table)) => table,
            _ => return Ok(()), // No metadata.ros section
        };

        // Process each installation target
        for (subdir, dest_base) in [
            ("share", self.share_dir()),
            (
                "include",
                self.install_base.join("include").join(&self.package_name),
            ),
            ("lib", self.lib_dir().join(&self.package_name)),
        ] {
            let key = format!("install_to_{}", subdir);

            if let Some(Value::Array(paths)) = metadata_ros.get(&key) {
                // Create destination directory
                fs::create_dir_all(&dest_base)?;

                for path_value in paths {
                    if let Value::String(rel_path) = path_value {
                        let src = self.project_root.join(rel_path);

                        if !src.exists() {
                            return Err(eyre::eyre!(
                                "[package.metadata.ros.{}] path not found: {} (expected at {})",
                                key,
                                rel_path,
                                src.display()
                            ));
                        }

                        // Get the file/directory name to preserve in destination
                        let name = src.file_name().ok_or_else(|| {
                            eyre::eyre!("Invalid path in metadata.ros: {}", rel_path)
                        })?;
                        let dest = dest_base.join(name);

                        if src.is_dir() {
                            self.copy_dir_recursive(&src, &dest)?;
                        } else {
                            fs::copy(&src, &dest).wrap_err_with(|| {
                                format!("Failed to copy {} to {}", src.display(), dest.display())
                            })?;
                        }

                        if self.verbose {
                            eprintln!("  Installed [metadata.ros.{}]: {}", key, rel_path);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Extract binary names from Cargo.toml
    fn extract_binary_names(&self, cargo_toml: &str) -> Vec<String> {
        let mut binaries = Vec::new();

        // Simple parser for [[bin]] sections
        let mut in_bin_section = false;

        for line in cargo_toml.lines() {
            let trimmed = line.trim();

            if trimmed == "[[bin]]" {
                in_bin_section = true;
                continue;
            }

            if in_bin_section {
                if trimmed.starts_with('[') {
                    in_bin_section = false;
                    continue;
                }

                if trimmed.starts_with("name")
                    && let Some(name) = self.extract_toml_string_value(trimmed)
                {
                    binaries.push(name);
                }
            }
        }

        // Also check for default binary (package name)
        // Note: Cargo uses the package name as-is for binary names (with dashes)
        if binaries.is_empty() {
            binaries.push(self.package_name.clone());
        }

        binaries
    }

    /// Extract string value from TOML line (simple parser)
    fn extract_toml_string_value(&self, line: &str) -> Option<String> {
        let parts: Vec<&str> = line.split('=').collect();
        if parts.len() != 2 {
            return None;
        }

        let value = parts[1].trim();
        let value = value.trim_matches('"').trim_matches('\'');
        Some(value.to_string())
    }

    /// Copy directory recursively
    fn copy_dir_recursive(&self, src: &Path, dst: &Path) -> Result<()> {
        copy_dir_recursive_impl(src, dst)
    }

    /// Get lib directory path
    fn lib_dir(&self) -> PathBuf {
        self.install_base.join("lib")
    }

    /// Get share directory path
    fn share_dir(&self) -> PathBuf {
        self.install_base.join("share").join(&self.package_name)
    }

    /// Get ament index directory path
    fn ament_index_dir(&self) -> PathBuf {
        self.install_base.join("share").join("ament_index")
    }

    /// Get rust source directory path
    fn rust_source_dir(&self) -> PathBuf {
        self.share_dir().join("rust")
    }
}

/// Copy directory recursively (helper function)
fn copy_dir_recursive_impl(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive_impl(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Check if a package is a pure library (no binaries)
pub fn is_library_package(project_root: &Path) -> Result<bool> {
    let cargo_toml_path = project_root.join("Cargo.toml");
    let cargo_toml = fs::read_to_string(&cargo_toml_path).wrap_err("Failed to read Cargo.toml")?;

    // Check if there's a [[bin]] section or default binary
    let has_bin_section = cargo_toml.contains("[[bin]]");
    let has_default_main = project_root.join("src").join("main.rs").exists();

    Ok(!has_bin_section && !has_default_main)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_ament_installer_directories() {
        let temp_dir = TempDir::new().unwrap();
        let install_base = temp_dir.path().join("install").join("test_pkg");
        let project_root = temp_dir.path().join("project");
        let target_dir = temp_dir.path().join("target");

        let installer = AmentInstaller::new(
            install_base.clone(),
            "test_pkg".to_string(),
            project_root,
            target_dir,
            false,
            "debug".to_string(),
        );

        assert_eq!(installer.lib_dir(), install_base.join("lib"));
        assert_eq!(
            installer.share_dir(),
            install_base.join("share").join("test_pkg")
        );
        assert_eq!(
            installer.ament_index_dir(),
            install_base.join("share").join("ament_index")
        );
        assert_eq!(
            installer.rust_source_dir(),
            install_base.join("share").join("test_pkg").join("rust")
        );
    }

    #[test]
    fn test_is_library_package() {
        let temp_dir = TempDir::new().unwrap();

        // Create a library package
        fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[package]
name = "test-lib"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_lib"
"#,
        )
        .unwrap();

        fs::write(temp_dir.path().join("src").join("lib.rs"), "").unwrap();

        assert!(is_library_package(temp_dir.path()).unwrap());
    }

    #[test]
    fn test_is_not_library_package() {
        let temp_dir = TempDir::new().unwrap();

        // Create a binary package
        fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[package]
name = "test-bin"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();

        fs::write(temp_dir.path().join("src").join("main.rs"), "fn main() {}").unwrap();

        assert!(!is_library_package(temp_dir.path()).unwrap());
    }

    #[test]
    fn test_extract_binary_names() {
        let temp_dir = TempDir::new().unwrap();
        let installer = AmentInstaller::new(
            temp_dir.path().to_path_buf(),
            "my-pkg".to_string(),
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("target"),
            false,
            "debug".to_string(),
        );

        let cargo_toml = r#"
[package]
name = "my-pkg"

[[bin]]
name = "my-binary"
path = "src/main.rs"

[[bin]]
name = "other-binary"
path = "src/other.rs"
"#;

        let binaries = installer.extract_binary_names(cargo_toml);
        assert_eq!(binaries.len(), 2);
        assert!(binaries.contains(&"my-binary".to_string()));
        assert!(binaries.contains(&"other-binary".to_string()));
    }

    #[test]
    fn test_extract_toml_string_value() {
        let temp_dir = TempDir::new().unwrap();
        let installer = AmentInstaller::new(
            temp_dir.path().to_path_buf(),
            "test".to_string(),
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("target"),
            false,
            "debug".to_string(),
        );

        assert_eq!(
            installer.extract_toml_string_value("name = \"my-binary\""),
            Some("my-binary".to_string())
        );

        assert_eq!(
            installer.extract_toml_string_value("name='other'"),
            Some("other".to_string())
        );

        assert_eq!(installer.extract_toml_string_value("invalid"), None);
    }
}
