//! Cargo configuration file patcher for ROS 2 bindings
//!
//! This module handles reading and writing .cargo/config.toml to add
//! [patch.crates-io] entries for generated ROS 2 bindings.

use eyre::{Result, WrapErr};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use toml::Value;

/// Cargo config patcher
pub struct ConfigPatcher {
    /// Path to .cargo/config.toml
    config_path: PathBuf,
    /// Parsed TOML document
    config: toml::Table,
}

impl ConfigPatcher {
    /// Create a new config patcher for a project
    pub fn new(project_root: &Path) -> Result<Self> {
        let cargo_dir = project_root.join(".cargo");
        let config_path = cargo_dir.join("config.toml");

        // Try to load existing config, or create empty if doesn't exist
        let config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .wrap_err_with(|| format!("Failed to read {}", config_path.display()))?;

            toml::from_str(&content).wrap_err("Failed to parse existing .cargo/config.toml")?
        } else {
            toml::Table::new()
        };

        Ok(ConfigPatcher {
            config_path,
            config,
        })
    }

    /// Add a patch entry for a ROS package
    pub fn add_patch(&mut self, package_name: &str, package_path: &Path) {
        // Ensure [patch] table exists
        if !self.config.contains_key("patch") {
            self.config
                .insert("patch".to_string(), Value::Table(toml::Table::new()));
        }

        let patch_table = self
            .config
            .get_mut("patch")
            .unwrap()
            .as_table_mut()
            .unwrap();

        // Ensure [patch.crates-io] table exists
        if !patch_table.contains_key("crates-io") {
            patch_table.insert("crates-io".to_string(), Value::Table(toml::Table::new()));
        }

        let crates_io_table = patch_table
            .get_mut("crates-io")
            .unwrap()
            .as_table_mut()
            .unwrap();

        // Add/update patch entry for this package
        let mut package_table = toml::Table::new();
        package_table.insert(
            "path".to_string(),
            Value::String(package_path.to_string_lossy().to_string()),
        );

        crates_io_table.insert(package_name.to_string(), Value::Table(package_table));
    }

    /// Add a git-based patch entry for a ROS package
    pub fn add_git_patch(&mut self, package_name: &str, git_url: &str) {
        // Ensure [patch] table exists
        if !self.config.contains_key("patch") {
            self.config
                .insert("patch".to_string(), Value::Table(toml::Table::new()));
        }

        let patch_table = self
            .config
            .get_mut("patch")
            .unwrap()
            .as_table_mut()
            .unwrap();

        // Ensure [patch.crates-io] table exists
        if !patch_table.contains_key("crates-io") {
            patch_table.insert("crates-io".to_string(), Value::Table(toml::Table::new()));
        }

        let crates_io_table = patch_table
            .get_mut("crates-io")
            .unwrap()
            .as_table_mut()
            .unwrap();

        // Add/update patch entry for this package
        let mut package_table = toml::Table::new();
        package_table.insert("git".to_string(), Value::String(git_url.to_string()));

        crates_io_table.insert(package_name.to_string(), Value::Table(package_table));
    }

    /// Add multiple patch entries
    pub fn add_patches(&mut self, patches: &HashMap<String, PathBuf>) {
        for (package_name, package_path) in patches {
            self.add_patch(package_name, package_path);
        }
    }

    /// Get current patch entry for a package (if any)
    pub fn get_patch(&self, package_name: &str) -> Option<PathBuf> {
        self.config
            .get("patch")?
            .as_table()?
            .get("crates-io")?
            .as_table()?
            .get(package_name)?
            .as_table()?
            .get("path")?
            .as_str()
            .map(PathBuf::from)
    }

    /// Remove a patch entry
    pub fn remove_patch(&mut self, package_name: &str) -> bool {
        if let Some(patch_table) = self.config.get_mut("patch")
            && let Some(crates_io) = patch_table.as_table_mut()
            && let Some(crates_io_table) = crates_io.get_mut("crates-io")
            && let Some(table) = crates_io_table.as_table_mut()
        {
            return table.remove(package_name).is_some();
        }
        false
    }

    /// Save the config back to disk
    pub fn save(&self) -> Result<()> {
        // Ensure .cargo directory exists
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .wrap_err_with(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        // Serialize to TOML string
        let content =
            toml::to_string_pretty(&self.config).wrap_err("Failed to serialize config")?;

        // Write to file
        fs::write(&self.config_path, content)
            .wrap_err_with(|| format!("Failed to write {}", self.config_path.display()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_patcher_empty_project() {
        let temp_dir = tempfile::tempdir().unwrap();
        let patcher = ConfigPatcher::new(temp_dir.path()).unwrap();

        assert!(patcher.config.is_empty());
    }

    #[test]
    fn test_add_single_patch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();

        patcher.add_patch("std_msgs", Path::new("target/ros2_bindings/std_msgs"));

        let patch_path = patcher.get_patch("std_msgs").unwrap();
        assert_eq!(patch_path, PathBuf::from("target/ros2_bindings/std_msgs"));
    }

    #[test]
    fn test_add_multiple_patches() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();

        let mut patches = HashMap::new();
        patches.insert(
            "std_msgs".to_string(),
            PathBuf::from("target/ros2_bindings/std_msgs"),
        );
        patches.insert(
            "geometry_msgs".to_string(),
            PathBuf::from("target/ros2_bindings/geometry_msgs"),
        );

        patcher.add_patches(&patches);

        assert!(patcher.get_patch("std_msgs").is_some());
        assert!(patcher.get_patch("geometry_msgs").is_some());
    }

    #[test]
    fn test_update_existing_patch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();

        patcher.add_patch("std_msgs", Path::new("path/v1"));
        patcher.add_patch("std_msgs", Path::new("path/v2"));

        let patch_path = patcher.get_patch("std_msgs").unwrap();
        assert_eq!(patch_path, PathBuf::from("path/v2"));
    }

    #[test]
    fn test_remove_patch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();

        patcher.add_patch("std_msgs", Path::new("target/ros2_bindings/std_msgs"));
        assert!(patcher.get_patch("std_msgs").is_some());

        let removed = patcher.remove_patch("std_msgs");
        assert!(removed);
        assert!(patcher.get_patch("std_msgs").is_none());
    }

    #[test]
    fn test_remove_nonexistent_patch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();

        let removed = patcher.remove_patch("nonexistent");
        assert!(!removed);
    }

    #[test]
    fn test_save_and_reload() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create and save config
        {
            let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();
            patcher.add_patch("std_msgs", Path::new("target/ros2_bindings/std_msgs"));
            patcher.save().unwrap();
        }

        // Reload and verify
        {
            let patcher = ConfigPatcher::new(temp_dir.path()).unwrap();
            let patch_path = patcher.get_patch("std_msgs").unwrap();
            assert_eq!(patch_path, PathBuf::from("target/ros2_bindings/std_msgs"));
        }
    }

    #[test]
    fn test_preserve_existing_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cargo_dir = temp_dir.path().join(".cargo");
        fs::create_dir_all(&cargo_dir).unwrap();

        // Create existing config with some settings
        let existing_config = r#"
[build]
target = "x86_64-unknown-linux-gnu"

[term]
color = "always"
"#;
        fs::write(cargo_dir.join("config.toml"), existing_config).unwrap();

        // Load, add patch, save
        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();
        patcher.add_patch("std_msgs", Path::new("target/ros2_bindings/std_msgs"));
        patcher.save().unwrap();

        // Reload and verify both old and new settings exist
        let content = fs::read_to_string(cargo_dir.join("config.toml")).unwrap();
        assert!(content.contains("[build]"));
        assert!(content.contains("[term]"));
        // TOML may format nested tables as [patch."crates-io"] or [patch.crates-io]
        assert!(content.contains("patch") && content.contains("crates-io"));
        assert!(content.contains("std_msgs"));
    }
}
