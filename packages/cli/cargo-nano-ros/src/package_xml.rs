//! Package.xml parser for extracting ROS 2 dependencies and provisions
//!
//! This module parses package.xml files to extract interface dependencies
//! (std_msgs, geometry_msgs, etc.) that need bindings generated, and the
//! phase-348 provision export that makes a package DISCOVERABLE as a provider.

use eyre::{Result, WrapErr, eyre};
use quick_xml::{Reader, events::Event};
use std::{collections::HashSet, path::Path};

/// A `(kind, name)` announcement — phase-348 W1 / RFC-0071 D5, generalised by
/// RFC-0087 D3.
///
/// One shape, two directions, two tags:
///
/// ```xml
/// <export>
///   <nano_ros_provides kind="rmw"    name="zenoh"/>   <!-- "I am"           -->
///   <nano_ros_uses     kind="serdes" name="flatbuf"/> <!-- "build me against" -->
/// </export>
/// ```
///
/// The directions stay two tags deliberately. They mean opposite things, and
/// spelling one as an attribute of the other is how two independent readers
/// came to confuse them: this module's own test message ("`<nano_ros rmw=…>`
/// says what this package CONSUMES") and `cmake/NanoRosPackageXml.cmake`'s
/// comment about having "reported the file as consuming `rmw=zenoh`".
///
/// `kind` is an open vocabulary. The scan does not validate it, because the
/// kinds that exist are a property of what descriptors exist
/// (`nros-{rmw,board,platform,serdes}.toml`), not of this parser — a new
/// provider family must not require editing the XML reader. That is the whole
/// point of `<nano_ros_uses>`: selecting a serializer costs no new attribute in
/// this parser or in the cmake one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct Provision {
    /// What family this announcement names: `rmw`, `board`, `platform`, …
    pub kind: String,
    /// The name a consumer selects it by (`rmw = "zenoh"`).
    pub name: String,
}

/// A consumption announcement. Same shape as [`Provision`], opposite direction
/// — see that type's docs for why they are not one tag.
pub type Selection = Provision;

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
    /// `<export><nano_ros_uses …/></export>` entries, plus the `board=` / `rmw=`
    /// attributes of the `<nano_ros …/>` sugar desugared into the same list
    /// (RFC-0087 D3). Declaration order, sugar last.
    pub uses: Vec<Selection>,
    /// The `deploy=` attribute of `<nano_ros …/>`, verbatim.
    ///
    /// NOT desugared into [`Self::uses`], because `deploy` is **not a provider
    /// kind**: it names a `[deploy.*]` block in `system.toml`, which
    /// `NanoRosPackageXml.cmake` maps to the `NANO_ROS_PLATFORM` axis. Folding
    /// it in would invent a family that has no descriptor and no provider.
    pub deploy: Option<String>,
}

/// Read a `(kind, name)` announcement from either announcement tag.
///
/// ONE rule set for `<nano_ros_provides>` and `<nano_ros_uses>` (RFC-0087 D3):
/// inside `<export>`, both attributes present and non-empty, no others allowed.
/// The tag name is carried only so the error text names the element the author
/// actually wrote.
fn read_announcement(
    e: &quick_xml::events::BytesStart<'_>,
    tag: &str,
    in_export: bool,
) -> Result<Provision> {
    if !in_export {
        return Err(eyre!(
            "<{tag}> outside <export> — an announcement is only read from the \
             export block, so this one would never be discovered"
        ));
    }
    let mut kind = None;
    let mut name = None;
    for attr in e.attributes() {
        let attr = attr.map_err(|e| eyre!("bad attribute: {e}"))?;
        let value = attr
            .unescape_value()
            .map_err(|e| eyre!("bad attribute value: {e}"))?
            .to_string();
        match attr.key.as_ref() {
            b"kind" => kind = Some(value),
            b"name" => name = Some(value),
            other => {
                return Err(eyre!(
                    "<{tag}> has unknown attribute {:?} — expected only kind= and name=",
                    String::from_utf8_lossy(other)
                ));
            }
        }
    }
    // Both are load-bearing and neither has a defensible default: an
    // announcement with no name cannot be selected, and one with no kind names
    // no descriptor.
    match (kind, name) {
        (Some(k), Some(n)) if !k.is_empty() && !n.is_empty() => Ok(Provision { kind: k, name: n }),
        (k, n) => Err(eyre!(
            "<{tag}> needs non-empty kind= and name= (got kind={:?}, name={:?})",
            k.unwrap_or_default(),
            n.unwrap_or_default()
        )),
    }
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
        let mut uses: Vec<Selection> = Vec::new();
        let mut sugar: Vec<Selection> = Vec::new();
        let mut deploy = None;

        let mut current_tag = String::new();
        let mut in_export = false;

        loop {
            match reader.read_event() {
                // `Empty` is the self-closing form. Before phase-348 this arm
                // did not exist at all — every `<tag/>` fell through the `_`
                // catch-all — so a provision written self-closing (the natural
                // spelling, and the one the docs show) would have been silently
                // invisible.
                // `Empty` is the self-closing form. Before phase-348 this arm
                // did not exist at all — every `<tag/>` fell through the `_`
                // catch-all — so a provision written self-closing (the natural
                // spelling, and the one the docs show) would have been silently
                // invisible.
                //
                // RFC-0087 D3 — both announcement tags are read here, by ONE
                // rule set. Two readers implementing the rule separately is
                // exactly how provision and consumption came to be confused;
                // one match arm cannot disagree with itself.
                Ok(Event::Start(e) | Event::Empty(e))
                    if matches!(e.name().as_ref(), b"nano_ros_provides" | b"nano_ros_uses") =>
                {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let announcement = read_announcement(&e, &tag, in_export)?;
                    if tag == "nano_ros_provides" {
                        provides.push(announcement);
                    } else {
                        uses.push(announcement);
                    }
                }
                // The `<nano_ros deploy= board= rmw=/>` sugar (91 packages).
                // `board=` and `rmw=` ARE provider selections and desugar into
                // `uses`; `deploy=` is not a kind and stays an attribute.
                Ok(Event::Start(e) | Event::Empty(e)) if e.name().as_ref() == b"nano_ros" => {
                    if !in_export {
                        return Err(eyre!(
                            "<nano_ros> outside <export> — the consumption tuple is \
                             only read from the export block, so this one would \
                             never be seen"
                        ));
                    }
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| eyre!("bad attribute: {e}"))?;
                        let value = attr
                            .unescape_value()
                            .map_err(|e| eyre!("bad attribute value: {e}"))?
                            .to_string();
                        if value.is_empty() {
                            continue;
                        }
                        match attr.key.as_ref() {
                            b"deploy" => deploy = Some(value),
                            b"board" => sugar.push(Selection {
                                kind: "board".to_string(),
                                name: value,
                            }),
                            b"rmw" => sugar.push(Selection {
                                kind: "rmw".to_string(),
                                name: value,
                            }),
                            other => {
                                return Err(eyre!(
                                    "<nano_ros> has unknown attribute {:?} — the sugar \
                                     carries deploy=, board= and rmw=; anything else is \
                                     a <nano_ros_uses kind= name=/>",
                                    String::from_utf8_lossy(other)
                                ));
                            }
                        }
                    }
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
            uses: {
                uses.extend(sugar);
                uses
            },
            deploy,
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

    /// Selections of one kind, in declaration order — the general form and the
    /// `<nano_ros …/>` sugar together, which is what makes them equivalent to a
    /// consumer.
    pub fn uses_of_kind(&self, kind: &str) -> impl Iterator<Item = &Selection> {
        self.uses.iter().filter(move |u| u.kind == kind)
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
    // ── RFC-0087 D3 / phase-420 W1 — the general consumption form ─────────

    /// The acceptance criterion: a family with no bespoke attribute is
    /// selectable, and this parser learned nothing to make that true.
    #[test]
    fn a_family_with_no_attribute_is_selectable() {
        let pkg = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros_uses kind="serdes" name="flatbuf"/>"#,
        ))
        .unwrap();
        assert_eq!(
            pkg.uses_of_kind("serdes").collect::<Vec<_>>(),
            vec![&Selection {
                kind: "serdes".to_string(),
                name: "flatbuf".to_string(),
            }]
        );
        // And it is NOT a provision: this package consumes flatbuf, it is not
        // flatbuf. That confusion has cost two readers already.
        assert!(pkg.provides.is_empty());
    }

    /// Sugar and general form must be indistinguishable to a consumer, or the
    /// 91 packages using the tuple would mean something subtly different from
    /// the packages using the general form.
    #[test]
    fn the_sugar_and_the_general_form_resolve_identically() {
        let sugar = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/>"#,
        ))
        .unwrap();
        let general = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros deploy="freertos"/>
    <nano_ros_uses kind="board" name="mps2-an385-freertos"/>
    <nano_ros_uses kind="rmw" name="zenoh"/>"#,
        ))
        .unwrap();

        for kind in ["board", "rmw"] {
            assert_eq!(
                sugar.uses_of_kind(kind).collect::<Vec<_>>(),
                general.uses_of_kind(kind).collect::<Vec<_>>(),
                "{kind} differs between the sugar and the general form"
            );
        }
        assert_eq!(sugar.deploy.as_deref(), Some("freertos"));
        assert_eq!(general.deploy.as_deref(), Some("freertos"));
    }

    /// `deploy` names a `[deploy.*]` block in system.toml, not a provider, so
    /// it must not appear as a selection of kind `deploy` — a family with no
    /// descriptor and no provider behind it.
    #[test]
    fn deploy_is_not_a_provider_kind() {
        let pkg =
            PackageXml::parse_str(&provider_xml(r#"    <nano_ros deploy="native"/>"#)).unwrap();
        assert_eq!(pkg.deploy.as_deref(), Some("native"));
        assert!(
            pkg.uses.is_empty(),
            "deploy must not desugar into a selection"
        );
        assert_eq!(pkg.uses_of_kind("deploy").count(), 0);
    }

    /// The same rule set as `<nano_ros_provides>`, because it is literally the
    /// same code path — asserted here so a future split shows up as a failure.
    #[test]
    fn a_selection_obeys_the_provision_rules() {
        // outside <export>
        let err = PackageXml::parse_str(
            r#"<?xml version="1.0"?>
<package format="3">
  <name>p</name>
  <version>0.0.0</version>
  <nano_ros_uses kind="serdes" name="flatbuf"/>
</package>"#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("nano_ros_uses"),
            "the error must name the tag the author wrote: {err}"
        );

        // unknown attribute
        let err = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros_uses kind="serdes" name="flatbuf" versoin="2"/>"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("versoin"), "{err}");

        // empty name
        let err = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros_uses kind="serdes" name=""/>"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("non-empty"), "{err}");
    }

    /// issue 0516 — a documented example is not a declaration, and the strip
    /// covers the new tag because it covers the file, not a tag list.
    #[test]
    fn a_commented_out_selection_is_not_a_selection() {
        let pkg = PackageXml::parse_str(&provider_xml(
            r#"    <!-- <nano_ros_uses kind="serdes" name="ghost"/> -->
    <nano_ros_uses kind="serdes" name="real"/>"#,
        ))
        .unwrap();
        assert_eq!(
            pkg.uses_of_kind("serdes")
                .map(|u| u.name.as_str())
                .collect::<Vec<_>>(),
            vec!["real"]
        );
    }

    /// An unknown attribute on the sugar is a typo, not a new axis — the
    /// general form is where a new family goes.
    #[test]
    fn the_sugar_rejects_an_unknown_attribute() {
        let err = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros deploy="native" serdes="flatbuf"/>"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("serdes"), "{err}");
        assert!(err.to_string().contains("nano_ros_uses"), "{err}");
    }
}
