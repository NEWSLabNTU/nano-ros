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
    /// `<export><nano_ros_uses …/></export>` entries (RFC-0087 D3), in
    /// declaration order. The `<nano_ros board= rmw=/>` sugar that used to
    /// desugar into this list is retired and refused (phase-445 W3b): a
    /// package's board and RMW are its `system.toml`.
    pub uses: Vec<Selection>,
    /// `<export><build_type>…</build_type></export>`, VERBATIM — the raw
    /// spelling, never canonicalised here.
    ///
    /// RFC-0094 D3 makes this load-bearing: it selects the DRIVER that builds
    /// the package, where file presence selects whether it is built here at
    /// all. Two questions, and conflating them is a defect in both directions
    /// (issue 1207).
    ///
    /// The spelling stays raw because the vocabulary is not this crate's:
    /// `nros_cli_core::build_type` owns the table that maps six live spellings
    /// and three retired ones onto two build paths, and it is cross-checked
    /// against `cmake/NanoRosPackageXml.cmake` by a gate. A parser that
    /// resolved the value here would be a fourth reader of that table.
    ///
    /// `None` for the 5 tracked packages that declare no build type at all —
    /// which is not an error: a consumer falls back to file presence, which is
    /// the pre-RFC-0094 answer.
    pub build_type: Option<String>,
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
        let mut build_type: Option<String> = None;

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
                // phase-445 W3b (RFC-0098 D3/D5) — the `<nano_ros deploy= board=
                // rmw=/>` sugar is RETIRED. A package states its board and RMW in
                // the `system.toml` beside its manifest, and the cmake reader
                // (`nano_ros_read_package_export`) refuses the element; this is
                // its twin, so the two readers cannot disagree about a file.
                // Refused rather than ignored: ignoring it would drop a board
                // selection the author believes is in force.
                Ok(Event::Start(e) | Event::Empty(e)) if e.name().as_ref() == b"nano_ros" => {
                    return Err(eyre!(
                        "the <nano_ros deploy= board= rmw=/> element is retired \
                         (RFC-0098 D3/D5, phase-445) — state the deployment in a \
                         system.toml beside the package's manifest (`[system] \
                         rmw/domain_id`, `[image.<id>] board = \"<board>\"`) and \
                         delete the element; a provider selection with no \
                         deployment meaning is `<nano_ros_uses kind= name=/>`"
                    ));
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
                        // FIRST declaration wins, matching every other reader
                        // of this element: `check-build-type-spelling.py` and
                        // `NanoRosPackageXml.cmake` both take `[0]` of what
                        // they find. A second `<build_type>` is a malformed
                        // package.xml, and disagreeing about WHICH one is
                        // authoritative is worse than either answer.
                        //
                        // NOT gated on `in_export`, deliberately: colcon reads
                        // it from `<export>` and every tracked file writes it
                        // there, but the two regex readers this must agree with
                        // scan the whole file. A reader that were stricter here
                        // would resolve a package differently from the cmake
                        // reader that acts on it — the exact drift RFC-0087 D2
                        // built the shared table to prevent.
                        "build_type" if build_type.is_none() => {
                            build_type = Some(text);
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
            uses,
            build_type,
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

    /// Selections of one kind, in declaration order.
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
            r#"    <nano_ros_uses kind="rmw" name="zenoh"/>"#,
        ))
        .unwrap();
        assert!(
            pkg.provides.is_empty(),
            "<nano_ros_uses kind=rmw> says what this package CONSUMES; reading it \
             as a provision would make every consumer advertise itself as a backend"
        );
        assert_eq!(pkg.uses_of_kind("rmw").count(), 1);
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

    /// phase-445 W3b — the `<nano_ros deploy= board= rmw=/>` sugar is retired
    /// and REFUSED, in every shape the tree used to carry, with an error naming
    /// the file to write. The cmake twin (`nano_ros_read_package_export`) refuses
    /// it too, so no reader can quietly honour a board the other rejects.
    #[test]
    fn the_retired_tuple_is_refused() {
        for tuple in [
            r#"    <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/>"#,
            r#"    <nano_ros deploy="native"/>"#,
            r#"    <nano_ros deploy="native" serdes="flatbuf"/>"#,
        ] {
            let err = PackageXml::parse_str(&provider_xml(tuple)).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("retired") && msg.contains("system.toml"),
                "{msg}"
            );
        }
        // The general form a selection moved to still parses, and `deploy`
        // is no family of it.
        let general = PackageXml::parse_str(&provider_xml(
            r#"    <nano_ros_uses kind="board" name="mps2-an385-freertos"/>
    <nano_ros_uses kind="rmw" name="zenoh"/>"#,
        ))
        .unwrap();
        assert_eq!(general.uses_of_kind("board").count(), 1);
        assert_eq!(general.uses_of_kind("rmw").count(), 1);
        assert_eq!(general.uses_of_kind("deploy").count(), 0);
        // A COMMENTED-OUT tuple is not a tuple (issue 0516) — not refused.
        assert!(
            PackageXml::parse_str(&provider_xml(r#"    <!-- <nano_ros deploy="native"/> -->"#))
                .is_ok()
        );
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

    // ── RFC-0094 D3 / phase-439 W3 — <build_type> is read, not dropped ────

    /// The headline of W3: the element three sites had been re-deriving from
    /// file presence is now carried by the parser.
    #[test]
    fn the_build_type_is_read_verbatim() {
        let pkg =
            PackageXml::parse_str(&provider_xml(r#"    <build_type>nros_cmake</build_type>"#))
                .unwrap();
        assert_eq!(pkg.build_type.as_deref(), Some("nros_cmake"));
    }

    /// A package declaring none is not an error — 5 tracked packages do, and
    /// RFC-0094 D3 falls back to file presence for them rather than inventing
    /// a declaration.
    #[test]
    fn no_build_type_is_none_not_an_error() {
        let pkg = PackageXml::parse_str(
            r#"<?xml version="1.0"?>
<package format="3">
  <name>undeclared</name>
</package>"#,
        )
        .unwrap();
        assert_eq!(pkg.build_type, None);
    }

    /// The value is RAW. Canonicalisation belongs to `nros_cli_core::build_type`,
    /// whose table is cross-checked against the cmake reader — resolving here
    /// would make this a fourth, unchecked reader of that vocabulary.
    #[test]
    fn a_legacy_or_foreign_spelling_is_carried_not_resolved() {
        for raw in ["ament_cmake", "ament_python", "nros_entry"] {
            let pkg = PackageXml::parse_str(&provider_xml(&format!(
                "    <build_type>{raw}</build_type>"
            )))
            .unwrap();
            assert_eq!(
                pkg.build_type.as_deref(),
                Some(raw),
                "{raw} must survive the parser unchanged"
            );
        }
    }

    /// `<build_type>\n    ament_cargo\n  </build_type>` is the shape a
    /// hand-indented file takes, and `trim_text` is what makes the two spellings
    /// one value. Pinned because the routing decision keys on an exact match
    /// against the table.
    #[test]
    fn surrounding_whitespace_does_not_change_the_declaration() {
        let pkg = PackageXml::parse_str(&provider_xml(
            "    <build_type>\n      nros_cargo\n    </build_type>",
        ))
        .unwrap();
        assert_eq!(pkg.build_type.as_deref(), Some("nros_cargo"));
    }

    /// A declaration inside a COMMENT is not a declaration — the same rule the
    /// provision arms carry, and the one both regex readers of this element had
    /// to be taught (issue 0516).
    #[test]
    fn a_commented_out_build_type_is_not_a_declaration() {
        let pkg = PackageXml::parse_str(&provider_xml(
            r#"    <!-- <build_type>nros_cargo</build_type> -->
    <build_type>nros_cmake</build_type>"#,
        ))
        .unwrap();
        assert_eq!(pkg.build_type.as_deref(), Some("nros_cmake"));
    }

    /// FIRST wins, matching `check-build-type-spelling.py` and
    /// `NanoRosPackageXml.cmake`, which both take `[0]`. Two readers
    /// disagreeing about which of two declarations is authoritative is worse
    /// than either answer.
    #[test]
    fn the_first_declaration_wins() {
        let pkg = PackageXml::parse_str(&provider_xml(
            r#"    <build_type>nros_cmake</build_type>
    <build_type>nros_cargo</build_type>"#,
        ))
        .unwrap();
        assert_eq!(pkg.build_type.as_deref(), Some("nros_cmake"));
    }
}
