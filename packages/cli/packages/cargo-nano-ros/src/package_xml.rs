//! Package.xml parser for extracting ROS 2 dependencies
//!
//! This module parses package.xml files to extract interface dependencies
//! (std_msgs, geometry_msgs, etc.) that need bindings generated.

use eyre::{Result, WrapErr, eyre};
use quick_xml::{Reader, events::Event};
use std::{collections::HashSet, path::Path};

/// Parsed package.xml metadata
#[derive(Debug, Clone)]
pub struct PackageXml {
    /// Package name
    pub name: String,
    /// Package version
    pub version: String,
    /// All dependencies (build, exec, depend)
    pub dependencies: HashSet<String>,
}

impl PackageXml {
    /// Parse a package.xml file
    pub fn parse(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read {}", path.display()))?;

        Self::parse_str(&content)
    }

    /// Parse package.xml from string content
    pub fn parse_str(content: &str) -> Result<Self> {
        let mut reader = Reader::from_str(content);
        reader.config_mut().trim_text(true);

        let mut name = None;
        let mut version = None;
        let mut dependencies = HashSet::new();

        let mut current_tag = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => {
                    current_tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                }
                Ok(Event::Text(e)) => {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match current_tag.as_str() {
                        "name" if name.is_none() => {
                            name = Some(text);
                        }
                        "version" if version.is_none() => {
                            version = Some(text);
                        }
                        "depend" | "build_depend" | "exec_depend" | "build_export_depend" => {
                            dependencies.insert(text);
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(_)) => {
                    current_tag.clear();
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(eyre!("XML parse error: {}", e)),
                _ => {}
            }
        }

        Ok(PackageXml {
            name: name.ok_or_else(|| eyre!("Missing <name> in package.xml"))?,
            version: version.unwrap_or_else(|| "0.0.0".to_string()),
            dependencies,
        })
    }

    /// Get all dependencies
    pub fn all_dependencies(&self) -> &HashSet<String> {
        &self.dependencies
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_package_xml() {
        let xml = r#"<?xml version="1.0"?>
<package format="3">
  <name>my_package</name>
  <version>1.0.0</version>
  <description>Test package</description>
  <maintainer email="test@test.com">Test</maintainer>
  <license>Apache-2.0</license>

  <depend>std_msgs</depend>
  <depend>geometry_msgs</depend>
  <build_depend>rosidl_default_generators</build_depend>
  <exec_depend>rosidl_default_runtime</exec_depend>

  <export>
    <build_type>ament_cargo</build_type>
  </export>
</package>"#;

        let pkg = PackageXml::parse_str(xml).unwrap();
        assert_eq!(pkg.name, "my_package");
        assert_eq!(pkg.version, "1.0.0");
        assert!(pkg.dependencies.contains("std_msgs"));
        assert!(pkg.dependencies.contains("geometry_msgs"));
        assert!(pkg.dependencies.contains("rosidl_default_generators"));
        assert!(pkg.dependencies.contains("rosidl_default_runtime"));
    }

    #[test]
    fn test_parse_minimal_package_xml() {
        let xml = r#"<?xml version="1.0"?>
<package format="3">
  <name>minimal</name>
</package>"#;

        let pkg = PackageXml::parse_str(xml).unwrap();
        assert_eq!(pkg.name, "minimal");
        assert_eq!(pkg.version, "0.0.0");
        assert!(pkg.dependencies.is_empty());
    }
}
