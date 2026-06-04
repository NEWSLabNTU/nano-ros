//! Cargo.toml dependency parser for discovering ROS 2 interface packages
//!
//! This module parses a project's Cargo.toml to find ROS 2 interface package
//! dependencies and matches them against the ament index.

use cargo_metadata::MetadataCommand;
use eyre::{Result, WrapErr, eyre};
use std::{
    collections::{HashSet, VecDeque},
    path::Path,
};

/// Information about a ROS 2 dependency
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RosDependency {
    /// Package name (e.g., "std_msgs")
    pub name: String,
    /// Whether this is a direct dependency
    pub direct: bool,
}

/// Dependency parser for discovering ROS 2 packages
pub struct DependencyParser {
    /// List of known ROS 2 package names (from ament index)
    known_ros_packages: HashSet<String>,
}

impl DependencyParser {
    /// Create a new dependency parser with known ROS package names
    pub fn new(known_ros_packages: HashSet<String>) -> Self {
        DependencyParser { known_ros_packages }
    }

    /// Discover ROS 2 dependencies from a Cargo project
    pub fn discover_dependencies(&self, project_root: &Path) -> Result<Vec<RosDependency>> {
        // Try cargo metadata first (for full dependency graph)
        // If it fails (e.g., due to yanked dependencies), fall back to manual parsing
        let metadata_result = MetadataCommand::new().current_dir(project_root).exec();

        match metadata_result {
            Ok(metadata) => self.discover_from_metadata(&metadata),
            Err(_) => self.discover_from_cargo_toml(project_root),
        }
    }

    /// Discover dependencies using cargo metadata (full dependency graph)
    fn discover_from_metadata(
        &self,
        metadata: &cargo_metadata::Metadata,
    ) -> Result<Vec<RosDependency>> {
        // Find the root package (the workspace root or single package)
        let root_package = metadata
            .root_package()
            .ok_or_else(|| eyre!("No root package found (is this a valid Cargo project?)"))?;

        let mut dependencies = Vec::new();
        let mut visited = HashSet::new();

        // BFS to discover all dependencies
        let mut queue = VecDeque::new();
        queue.push_back((root_package, true)); // (package, is_direct)

        while let Some((package, is_direct)) = queue.pop_front() {
            if !visited.insert(package.id.clone()) {
                continue;
            }

            // Check if this package is a ROS 2 interface package
            if self.is_ros_package(&package.name) {
                dependencies.push(RosDependency {
                    name: package.name.clone(),
                    direct: is_direct,
                });
            }

            // Add dependencies to queue (they are not direct)
            for dep in &package.dependencies {
                // Find the actual package for this dependency
                if let Some(dep_package) = metadata.packages.iter().find(|p| p.name == dep.name) {
                    queue.push_back((dep_package, false));
                }
            }
        }

        Ok(dependencies)
    }

    /// Discover dependencies by parsing Cargo.toml directly (fallback when metadata fails)
    /// This only discovers direct dependencies, which is sufficient for initial binding generation
    fn discover_from_cargo_toml(&self, project_root: &Path) -> Result<Vec<RosDependency>> {
        use std::fs;

        let cargo_toml_path = project_root.join("Cargo.toml");
        let contents =
            fs::read_to_string(&cargo_toml_path).wrap_err("Failed to read Cargo.toml")?;

        let mut dependencies = Vec::new();

        // Simple parsing: look for lines in [dependencies] section
        let mut in_dependencies = false;
        for line in contents.lines() {
            let trimmed = line.trim();

            // Check for section headers
            if trimmed.starts_with('[') {
                in_dependencies = trimmed == "[dependencies]";
                continue;
            }

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Parse dependency line
            if in_dependencies && let Some(eq_pos) = trimmed.find('=') {
                let dep_name = trimmed[..eq_pos].trim().to_string();

                // Check if this is a known ROS package
                if self.is_ros_package(&dep_name) {
                    dependencies.push(RosDependency {
                        name: dep_name,
                        direct: true, // All discovered via Cargo.toml are direct
                    });
                }
            }
        }

        Ok(dependencies)
    }

    /// Check if a package name is a known ROS 2 package
    fn is_ros_package(&self, name: &str) -> bool {
        self.known_ros_packages.contains(name)
    }

    /// Get only direct dependencies
    pub fn get_direct_dependencies(&self, dependencies: &[RosDependency]) -> Vec<RosDependency> {
        dependencies
            .iter()
            .filter(|dep| dep.direct)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper to create a test Cargo.toml
    fn create_test_project(temp_dir: &Path, cargo_toml: &str) {
        fs::write(temp_dir.join("Cargo.toml"), cargo_toml).unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("src").join("main.rs"), "fn main() {}").unwrap();
    }

    #[test]
    fn test_new_parser() {
        let mut known = HashSet::new();
        known.insert("std_msgs".to_string());
        known.insert("geometry_msgs".to_string());

        let parser = DependencyParser::new(known);
        assert!(parser.is_ros_package("std_msgs"));
        assert!(parser.is_ros_package("geometry_msgs"));
        assert!(!parser.is_ros_package("serde"));
    }

    #[test]
    fn test_discover_no_dependencies() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cargo_toml = r#"
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
"#;
        create_test_project(temp_dir.path(), cargo_toml);

        let mut known = HashSet::new();
        known.insert("std_msgs".to_string());
        let parser = DependencyParser::new(known);

        let deps = parser.discover_dependencies(temp_dir.path()).unwrap();
        assert_eq!(deps.len(), 0);
    }

    #[test]
    fn test_discover_dependencies_valid_project() {
        // This test just verifies we can parse a valid project
        // We don't test with actual ROS dependencies since they don't exist in crates.io yet
        let temp_dir = tempfile::tempdir().unwrap();
        let cargo_toml = r#"
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
        create_test_project(temp_dir.path(), cargo_toml);

        let parser = DependencyParser::new(HashSet::new());
        let result = parser.discover_dependencies(temp_dir.path());

        // Should succeed even with no dependencies
        assert!(result.is_ok());
        let deps = result.unwrap();
        assert_eq!(deps.len(), 0);
    }

    #[test]
    fn test_get_direct_dependencies() {
        let deps = vec![
            RosDependency {
                name: "std_msgs".to_string(),
                direct: true,
            },
            RosDependency {
                name: "geometry_msgs".to_string(),
                direct: true,
            },
            RosDependency {
                name: "sensor_msgs".to_string(),
                direct: false,
            },
        ];

        let parser = DependencyParser::new(HashSet::new());
        let direct = parser.get_direct_dependencies(&deps);

        assert_eq!(direct.len(), 2);
        assert!(direct.iter().any(|d| d.name == "std_msgs"));
        assert!(direct.iter().any(|d| d.name == "geometry_msgs"));
        assert!(!direct.iter().any(|d| d.name == "sensor_msgs"));
    }

    #[test]
    fn test_empty_known_packages() {
        let parser = DependencyParser::new(HashSet::new());
        assert!(!parser.is_ros_package("std_msgs"));
        assert!(!parser.is_ros_package("anything"));
    }
}
