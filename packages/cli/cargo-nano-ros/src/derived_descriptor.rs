//! RFC-0087 D4 / phase-420 W5 — the convention half of a provider descriptor.
//!
//! A provider descriptor carries only what no convention can produce. Six of
//! the eleven fields in `nros-rmw-zenoh/nros-rmw.toml` were convention written
//! longhand, and a field written twice is a field that can disagree with
//! itself. This module is the ONE implementation of those conventions:
//!
//! | field | derived from |
//! | --- | --- |
//! | the provider's names | the `<nano_ros_provides>` announcements, in order |
//! | cargo feature | `<kind>-<name>` |
//! | cmake value | the canonical name |
//! | C define token | `UPPER(name)` |
//! | cffi feature | `<cargo_feature>-cffi` |
//!
//! (`crate` is the sixth in the RFC's table. It is NOT here, and phase-420 W5
//! measured why: `nros-rmw-xrce` ships no `Cargo.toml` at all and its
//! `[rmw.provides.cargo].crate` names a SIBLING package,
//! `nros-rmw-xrce-cffi`; `nros-rmw-cyclonedds` names `cyclonedds-sys`, not its
//! own crate. "The package's `Cargo.toml`" is the right answer for two of four
//! backends, which makes it a convention with exceptions rather than a
//! convention. It stays authored, and `check-derived-descriptor-fields`
//! grandfathers the divergence rather than pretending it is derivable.)
//!
//! **`build.rs` and the library share this file**, the former through
//! `#[path = "src/derived_descriptor.rs"]`. That is the point: the generator
//! and anything that later wants to answer "what would this field be?" cannot
//! be two spellings of one rule. `scripts/check-derived-descriptor-fields.py`
//! is a third reader by necessity (it is Python and buildless), and it is a
//! CHECKER of the same rule rather than a producer — the sibling gates take
//! the same shape for the same reason.
//!
//! No dependencies, deliberately: a build script may not pull one in without
//! moving `Cargo.lock`, which this repo only permits through
//! `just lock-update`.

/// The cargo feature a provider of `kind` named `name` lowers to.
///
/// `rmw` + `zenoh` -> `rmw-zenoh`. Phase 248 C5b: this is the BOARD crate's
/// feature, not an `nros/rmw-X` one — the board self-links and registers the
/// concrete backend.
pub fn cargo_feature(kind: &str, name: &str) -> String {
    format!("{kind}-{name}")
}

/// The `-DNANO_ROS_RMW` (and family equivalent) CMake value: the canonical
/// name, unchanged. A separate field only ever recorded the identity function.
pub fn cmake_value(name: &str) -> String {
    name.to_string()
}

/// The C `#define NROS_SYSTEM_RMW_<TOKEN>` token: `UPPER(name)`.
///
/// Anything that cannot appear in a C identifier becomes `_`, so a name like
/// `native_sim/native/64` lowers to something a preprocessor accepts rather
/// than to a token that fails at the use site. No in-tree rmw name needs the
/// substitution today (`cyclonedds` -> `CYCLONEDDS`); it exists so the rule is
/// total.
pub fn c_define_token(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// The `nros-c` / `nros-cpp` umbrella feature that bundles and force-links a
/// backend: the cargo feature plus `-cffi`.
pub fn cffi_feature(cargo_feature: &str) -> String {
    format!("{cargo_feature}-cffi")
}

/// The `<nano_ros_provides kind="..." name="..."/>` names a `package.xml`
/// announces, in file order.
///
/// **The announcement is the only spelling of a provider's names.** RFC-0087
/// D4: a descriptor that also lists them is a second spelling policed by a
/// gate, which is the shape CLAUDE.md warns about.
///
/// Comments are stripped first, and that is not optional (issue 0516): every
/// provider `package.xml` in this tree documents the tag in a comment above
/// the real one, and a scanner that cannot tell the two apart reads the
/// example as a claim. `check-provider-announcements.py` and
/// `nros_read_package_xml_body` in `NanoRosPackageXml.cmake` carry the
/// identical strip for the identical reason.
///
/// This is a scanner, not a parser: it does not validate the document, and
/// `package_xml::PackageXml` (quick-xml, the library's real reader) stays the
/// authority. `rmw_resolver`'s `descriptor_names_match_the_package_xml_reader`
/// asserts the two agree on every in-tree provider, so this cheap form cannot
/// drift from the expensive one.
pub fn announced_names(package_xml: &str, kind: &str) -> Vec<String> {
    let body = strip_xml_comments(package_xml);
    let needle = format!("kind=\"{kind}\"");
    let mut out = Vec::new();
    for tag in body.split("<nano_ros_provides").skip(1) {
        let Some(end) = tag.find('>') else { continue };
        let attrs = &tag[..end];
        if !attrs.contains(&needle) {
            continue;
        }
        if let Some(name) = attr_value(attrs, "name") {
            out.push(name);
        }
    }
    out
}

/// `<!-- ... -->` removed. An XML comment body cannot contain `--`, so the
/// scan is exact rather than heuristic.
fn strip_xml_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            // An unterminated comment swallows the remainder, which is what a
            // real XML reader does too.
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn attr_value(attrs: &str, key: &str) -> Option<String> {
    let at = attrs.find(&format!("{key}=\""))? + key.len() + 2;
    let rest = &attrs[at..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_scalar_conventions() {
        assert_eq!(cargo_feature("rmw", "zenoh"), "rmw-zenoh");
        assert_eq!(cargo_feature("serdes", "flatbuf"), "serdes-flatbuf");
        assert_eq!(cmake_value("cyclonedds"), "cyclonedds");
        assert_eq!(c_define_token("cyclonedds"), "CYCLONEDDS");
        assert_eq!(cffi_feature("rmw-xrce"), "rmw-xrce-cffi");
    }

    #[test]
    fn c_define_token_is_total() {
        // A name is not constrained to C identifier characters — the zephyr
        // board announces `native_sim/native/64`. The token must still be one.
        assert_eq!(
            c_define_token("native_sim/native/64"),
            "NATIVE_SIM_NATIVE_64"
        );
        assert_eq!(c_define_token("bare-metal"), "BARE_METAL");
    }

    #[test]
    fn announcements_are_read_in_file_order() {
        let xml = r#"<package format="3"><name>p</name>
  <export>
    <build_type>nros_cargo</build_type>
    <nano_ros_provides kind="rmw" name="zenoh"/>
    <nano_ros_provides kind="rmw" name="rmw-zenoh"/>
    <nano_ros_provides kind="board" name="not-an-rmw"/>
    <nano_ros_provides kind="rmw" name="rmw-zenoh-cffi"/>
  </export>
</package>"#;
        assert_eq!(
            announced_names(xml, "rmw"),
            ["zenoh", "rmw-zenoh", "rmw-zenoh-cffi"]
        );
        assert_eq!(announced_names(xml, "board"), ["not-an-rmw"]);
        assert!(announced_names(xml, "serdes").is_empty());
    }

    #[test]
    fn a_commented_out_announcement_is_not_a_claim() {
        // Issue 0516. Every provider package.xml in this tree documents the
        // tag in a comment; counting one is how a doc example becomes a name.
        let xml = r#"<export>
    <!-- <nano_ros_provides kind="rmw" name="example"/> -->
    <nano_ros_provides kind="rmw" name="real"/>
  </export>"#;
        assert_eq!(announced_names(xml, "rmw"), ["real"]);
    }

    #[test]
    fn an_unterminated_comment_does_not_leak_its_body() {
        let xml = r#"<export>
    <!-- <nano_ros_provides kind="rmw" name="example"/>
  </export>"#;
        assert!(announced_names(xml, "rmw").is_empty());
    }
}
