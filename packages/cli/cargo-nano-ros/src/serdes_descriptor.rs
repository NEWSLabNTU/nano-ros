// phase-421 W4 — the `nros-serdes.toml` descriptor, and the derivations that
// mean it carries almost nothing (RFC-0088 D6, RFC-0087 D4).
//
// **This module is `include!`d by `build.rs`** as well as compiled into the
// library, so it is deliberately std-only: `build.rs` may not use `toml` or
// `quick-xml`, which are ordinary dependencies rather than build-dependencies,
// and adding a build-dependency would move `Cargo.lock`.
//
// One parser, two callers, on purpose. The table in `OUT_DIR` is generated at
// build time (in-tree providers) while an out-of-repo provider's descriptor is
// read at selection time (`serdes_resolver::resolve_serdes_in`); two parsers
// for one file format is the drift class this repo keeps paying for
// (the sizes-header mirror, the FFI struct mirrors, the parity map).
//
// The `package.xml` reader below is the one exception, and it is the SMALL
// half: `build.rs` needs the `<nano_ros_provides kind="serdes"/>` names and
// cannot reach [`crate::package_xml::PackageXml`]'s quick-xml parser. It is
// cross-checked against the real parser by
// `serdes_resolver::tests::generated_names_match_the_real_package_xml_parser`,
// because two readers of one file that nobody compares is how the RMW parity
// map came to disagree with the vtable by 25 symbols.

/// What a provider's `nros-serdes.toml` says that no convention can produce.
///
/// Everything else about a serdes provider is DERIVED (RFC-0087 D4): the name
/// from the `package.xml` announcement, the crate from the sibling
/// `Cargo.toml`, and the cargo feature / cmake value / C token from the name.
/// A second spelling of a derivable fact drifts, so the descriptor does not
/// offer one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerdesDescriptor {
    /// `"schema"` | `"codegen"` — how the serializer is produced (RFC-0088 D7).
    /// Not derivable: a property of the implementation, not of the name.
    pub impl_strategy: String,
    /// Image-local `u8` discriminant (RFC-0088 D2). `None` means the provider
    /// states none and the build assigns one from the set of formats its image
    /// declares — **the NAME is the cross-image identity, never this number.**
    pub format_id: Option<u8>,
}

/// The strategy a provider gets when it ships no descriptor at all.
///
/// RFC-0088 D7: schema-driven is the default, because it costs a provider no
/// codegen work. `nros-serdes` itself declares `codegen` precisely because CDR
/// is the exception.
pub const DEFAULT_SERDES_IMPL: &str = "schema";

/// The serialization format a build gets when nothing declares one.
///
/// RFC-0088: a build that declares no format must behave exactly as it does
/// today, and today is CDR everywhere.
pub const DEFAULT_SERDES_NAME: &str = "cdr";

/// The strategies [`SerdesDescriptor::impl_strategy`] may name.
pub const SERDES_IMPL_STRATEGIES: &[&str] = &["schema", "codegen"];

/// The cargo feature a serdes name lowers to — `serdes-<name>`.
#[must_use]
pub fn serdes_cargo_feature(name: &str) -> String {
    format!("serdes-{name}")
}

/// The `-DNANO_ROS_SERDES` cmake value — the canonical name, verbatim.
///
/// A function rather than the caller writing `name.to_string()` so the
/// derivation has ONE site: the RMW descriptor authored `cmake_value` by hand
/// and it is `zenoh` for `zenoh` in every row, which is a field that only ever
/// restates its key.
#[must_use]
pub fn serdes_cmake_value(name: &str) -> String {
    name.to_string()
}

/// The C `#define` token — `UPPER(name)`.
///
/// `-` becomes `_` because a hyphen cannot appear in a preprocessor identifier
/// and a provider named `flat-buf` is legal (the announcement's `name=` is an
/// open vocabulary; see [`crate::package_xml::Provision`]).
#[must_use]
pub fn serdes_c_define_token(name: &str) -> String {
    name.to_uppercase().replace(['-', '.'], "_")
}

/// Parse an `nros-serdes.toml`.
///
/// Line-oriented, matching `build.rs`'s existing `nros-rmw.toml` reader and
/// `NanoRosCapabilities.cmake`'s `file(STRINGS … REGEX …)` — the descriptor
/// shape is flat `key = value` under one table, and it is ours.
///
/// Errors rather than defaulting on a value it does not understand: a typo'd
/// `impl = "codgen"` that silently became `schema` would produce a working
/// build with the wrong serializer.
pub fn parse_serdes_descriptor(text: &str, origin: &str) -> Result<SerdesDescriptor, String> {
    let mut section = String::new();
    let mut impl_strategy: Option<String> = None;
    let mut format_id: Option<u8> = None;

    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match (section.as_str(), key) {
            ("serdes", "impl") => impl_strategy = Some(value.trim_matches('"').to_string()),
            ("serdes", "format_id") => {
                let parsed = value.trim_matches('"').parse::<u8>().map_err(|e| {
                    format!("{origin}: [serdes].format_id = {value} is not a u8 ({e})")
                })?;
                if parsed == 0 {
                    return Err(format!(
                        "{origin}: [serdes].format_id = 0 is reserved for \"no format\" — \
                         in-tree formats hold low values from 1 (RFC-0088 D2)"
                    ));
                }
                format_id = Some(parsed);
            }
            _ => {}
        }
    }

    let impl_strategy = impl_strategy.unwrap_or_else(|| DEFAULT_SERDES_IMPL.to_string());
    if !SERDES_IMPL_STRATEGIES.contains(&impl_strategy.as_str()) {
        return Err(format!(
            "{origin}: [serdes].impl = {impl_strategy:?} is not a strategy (known: {})",
            SERDES_IMPL_STRATEGIES.join(", ")
        ));
    }

    Ok(SerdesDescriptor {
        impl_strategy,
        format_id,
    })
}

/// The `[package] name` of a `Cargo.toml`, for the `crate` derivation.
///
/// Std-only for the same reason as the descriptor parser. Only the `[package]`
/// table is consulted, so a `name =` under `[dependencies.…]` cannot be
/// mistaken for the crate's own.
#[must_use]
pub fn cargo_package_name(text: &str) -> Option<String> {
    let mut in_package = false;
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_package = section == "package";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == "name"
        {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// The `name=`s of every `<nano_ros_provides kind="<kind>" …/>` in a
/// `package.xml`, in declaration order.
///
/// The `build.rs` half of the "two readers of one file" note at the top of this
/// module. Comments are stripped first, because a commented-out announcement is
/// not an announcement — `package_xml::tests::commented_out_provision_is_ignored`
/// asserts exactly that of the real parser, and a second reader that disagreed
/// would announce a name no scan can find.
#[must_use]
pub fn package_xml_provides(text: &str, kind: &str) -> Vec<String> {
    let stripped = strip_xml_comments(text);
    let mut out = Vec::new();
    let mut rest = stripped.as_str();
    while let Some(at) = rest.find("<nano_ros_provides") {
        rest = &rest[at + "<nano_ros_provides".len()..];
        let Some(end) = rest.find('>') else { break };
        let (tag, after) = rest.split_at(end);
        rest = after;
        if xml_attr(tag, "kind").as_deref() == Some(kind)
            && let Some(name) = xml_attr(tag, "name")
            && !name.is_empty()
        {
            out.push(name);
        }
    }
    out
}

fn strip_xml_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("<!--") {
        out.push_str(&rest[..at]);
        match rest[at..].find("-->") {
            Some(end) => rest = &rest[at + end + 3..],
            // Unterminated comment: everything after it is commented out.
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn xml_attr(tag: &str, attr: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(at) = rest.find(attr) {
        let before_ok = at == 0
            || rest[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let after = &rest[at + attr.len()..];
        let after_trimmed = after.trim_start();
        if before_ok && let Some(v) = after_trimmed.strip_prefix('=') {
            let v = v.trim_start();
            let quote = v.chars().next()?;
            if quote == '"' || quote == '\'' {
                let body = &v[1..];
                return body.find(quote).map(|e| body[..e].to_string());
            }
        }
        rest = &rest[at + attr.len()..];
    }
    None
}
