//! Package.xml parser for extracting ROS 2 dependencies and provisions
//!
//! This module parses package.xml files to extract interface dependencies
//! (std_msgs, geometry_msgs, etc.) that need bindings generated, and the
//! phase-348 provision export that makes a package DISCOVERABLE as a provider.

use eyre::{Result, WrapErr, eyre};
use quick_xml::{Reader, events::Event};
use std::{collections::HashSet, path::Path};

/// A provision export — phase-348 W1 / RFC-0071 D5.
///
/// ```xml
/// <export>
///   <nano_ros_provides kind="rmw" name="zenoh"/>
/// </export>
/// ```
///
/// Deliberately a DIFFERENT tag from the consumption export
/// `<nano_ros deploy= board= rmw=/>` (`cmake/NanoRosPackageXml.cmake`), which
/// says "this is what I consume". The two would be confused on sight if
/// provision were spelled as another attribute of the same element, and they
/// mean opposite things: one selects a backend, the other IS one.
///
/// `kind` is an open vocabulary. The scan does not validate it, because the
/// kinds that exist are a property of what descriptors exist
/// (`nros-{rmw,board,platform}.toml`), not of this parser — a new provider
/// family must not require editing the XML reader.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct Provision {
    /// What family this package provides: `rmw`, `board`, `platform`, …
    pub kind: String,
    /// The name a consumer selects it by (`rmw = "zenoh"`).
    pub name: String,
}

/// Parsed package.xml metadata
#[derive(Debug, Clone)]
pub struct PackageXml {
    /// Package name
    pub name: String,
    /// Package version
    pub version: String,
    /// All dependencies (build, exec, depend)
    pub dependencies: HashSet<String>,
    /// `<export><nano_ros_provides …/></export>` entries (phase-348 W1).
    /// Empty for every package that is not a provider, which is almost all of
    /// them — this is the cheap parse the scan does per package.
    pub provides: Vec<Provision>,
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
        let mut provides = Vec::new();

        let mut current_tag = String::new();
        let mut in_export = false;

        loop {
            match reader.read_event() {
                // `Empty` is the self-closing form. Before phase-348 this arm
                // did not exist at all — every `<tag/>` fell through the `_`
                // catch-all — so a provision written self-closing (the natural
                // spelling, and the one the docs show) would have been silently
                // invisible.
                Ok(Event::Start(e) | Event::Empty(e))
                    if e.name().as_ref() == b"nano_ros_provides" =>
                {
                    if !in_export {
                        return Err(eyre!(
                            "<nano_ros_provides> outside <export> — a provision \
                             is only read from the export block, so this one \
                             would never be discovered"
                        ));
                    }
                    let mut kind = None;
                    let mut pname = None;
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| eyre!("bad attribute: {e}"))?;
                        let value = attr
                            .unescape_value()
                            .map_err(|e| eyre!("bad attribute value: {e}"))?
                            .to_string();
                        match attr.key.as_ref() {
                            b"kind" => kind = Some(value),
                            b"name" => pname = Some(value),
                            other => {
                                return Err(eyre!(
                                    "<nano_ros_provides> has unknown attribute {:?} \
                                     — expected only kind= and name=",
                                    String::from_utf8_lossy(other)
                                ));
                            }
                        }
                    }
                    // Both are load-bearing and neither has a defensible
                    // default: a provision with no name cannot be selected, and
                    // one with no kind names no descriptor.
                    let (kind, pname) = match (kind, pname) {
                        (Some(k), Some(n)) if !k.is_empty() && !n.is_empty() => (k, n),
                        (k, n) => {
                            return Err(eyre!(
                                "<nano_ros_provides> needs non-empty kind= and name= \
                                 (got kind={:?}, name={:?})",
                                k.unwrap_or_default(),
                                n.unwrap_or_default()
                            ));
                        }
                    };
                    provides.push(Provision { kind, name: pname });
                }
                Ok(Event::Start(e)) => {
                    current_tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if e.name().as_ref() == b"export" {
                        in_export = true;
                    }
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
                Ok(Event::End(e)) => {
                    if e.name().as_ref() == b"export" {
                        in_export = false;
                    }
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
            provides,
        })
    }

    /// Get all dependencies
    pub fn all_dependencies(&self) -> &HashSet<String> {
        &self.dependencies
    }

    /// Provisions of one kind, in declaration order.
    pub fn provides_of_kind(&self, kind: &str) -> impl Iterator<Item = &Provision> {
        self.provides.iter().filter(move |p| p.kind == kind)
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

    /// phase-348 W1 — a package.xml carrying provisions, in both XML spellings.
    fn provider_xml(body: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<package format="3">
  <name>nros_rmw_zenoh</name>
  <version>0.0.0</version>
  <export>
{body}
  </export>
</package>"#
        )
    }

    #[test]
    fn provision_export_is_parsed_self_closing() {
        let pkg = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros_provides kind="rmw" name="zenoh"/>"#,
        ))
        .unwrap();
        assert_eq!(
            pkg.provides,
            vec![Provision {
                kind: "rmw".into(),
                name: "zenoh".into()
            }]
        );
    }

    /// The paired form is legal XML and means the same thing. Worth pinning
    /// because the two forms take DIFFERENT quick-xml events, and only the
    /// self-closing one appears in the docs — so a user writing the paired form
    /// would otherwise hit a silent non-discovery.
    #[test]
    fn provision_export_is_parsed_paired() {
        let pkg = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros_provides kind="board" name="mps2-an385"></nano_ros_provides>"#,
        ))
        .unwrap();
        assert_eq!(pkg.provides.len(), 1);
        assert_eq!(pkg.provides[0].kind, "board");
    }

    #[test]
    fn one_package_may_provide_several_things() {
        let pkg = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros_provides kind="rmw" name="zenoh"/>
    <nano_ros_provides kind="rmw" name="zenoh-pico"/>"#,
        ))
        .unwrap();
        assert_eq!(pkg.provides.len(), 2);
        assert_eq!(
            pkg.provides_of_kind("rmw").count(),
            2,
            "both are rmw provisions"
        );
        assert_eq!(pkg.provides_of_kind("board").count(), 0);
    }

    /// The consumption export and the provision export must not be confused for
    /// one another — they mean opposite things and can appear in one file (a
    /// backend's own test fixture consumes an rmw while providing one).
    #[test]
    fn consumption_export_is_not_a_provision() {
        let pkg = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros deploy="native" board="native" rmw="zenoh"/>"#,
        ))
        .unwrap();
        assert!(
            pkg.provides.is_empty(),
            "<nano_ros rmw=…> says what this package CONSUMES; reading it as a \
             provision would make every consumer advertise itself as a backend"
        );
    }

    /// The acceptance criterion's negative half: an ordinary package is not a
    /// provider, and parsing it is unchanged.
    #[test]
    fn package_without_provision_provides_nothing() {
        let pkg = PackageXml::parse_str(
            r#"<?xml version="1.0"?>
<package format="3">
  <name>ordinary</name>
  <depend>std_msgs</depend>
  <export><build_type>ament_cargo</build_type></export>
</package>"#,
        )
        .unwrap();
        assert!(pkg.provides.is_empty());
        assert!(pkg.dependencies.contains("std_msgs"));
    }

    /// A provision inside an XML COMMENT is not a provision.
    ///
    /// Free here — `quick_xml` reports comments as their own event, which the
    /// match arms never look at — but pinned because the cmake and python
    /// readers of this same file are regexes over raw text, and both DID have
    /// this bug (phase-348 W1). If this parser ever grows a text-scanning
    /// fast path, this is the test that catches it.
    #[test]
    fn commented_out_provision_is_not_a_provision() {
        let pkg = PackageXml::parse_str(&provider_xml(
            r#"    <!-- <nano_ros_provides kind="rmw" name="ghost"/> -->
    <nano_ros_provides kind="rmw" name="real"/>"#,
        ))
        .unwrap();
        assert_eq!(pkg.provides.len(), 1);
        assert_eq!(pkg.provides[0].name, "real");
    }

    #[test]
    fn provision_outside_export_is_an_error() {
        let err = PackageXml::parse_str(
            r#"<?xml version="1.0"?>
<package format="3">
  <name>misplaced</name>
  <nano_ros_provides kind="rmw" name="zenoh"/>
</package>"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("outside <export>"), "got: {err}");
    }

    #[test]
    fn provision_missing_name_is_an_error() {
        let err = PackageXml::parse_str(&provider_xml(r#"    <nano_ros_provides kind="rmw"/>"#))
            .unwrap_err()
            .to_string();
        assert!(err.contains("non-empty kind= and name="), "got: {err}");
    }

    #[test]
    fn provision_with_unknown_attribute_is_an_error() {
        let err = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros_provides kind="rmw" name="zenoh" versoin="2"/>"#,
        ))
        .unwrap_err()
        .to_string();
        assert!(err.contains("unknown attribute"), "got: {err}");
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
