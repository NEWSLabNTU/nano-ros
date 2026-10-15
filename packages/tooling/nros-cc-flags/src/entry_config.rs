//! The connect configuration a CARGO-ROOTED C/C++ image bakes — issue 1512.
//!
//! `<rclc/detail/entry_config.h>` is the one producer of the `NROS_ENTRY_LOCATOR` /
//! `NROS_ENTRY_DOMAIN_ID` ladder (issue 0946). Its rungs are Kconfig values
//! and then the empty "absent" bottom rung; it lets an entry bake a literal by
//! defining either macro before the include (`#ifndef`). The cmake road does
//! that with `-DNROS_ENTRY_LOCATOR=…` (`NanoRosEntry.cmake`). A cargo-rooted
//! image — a bare-metal C or C++ leaf whose link root is Rust and whose C is
//! compiled by `cc` in `build.rs` — had no such carrier, so it dialled the
//! bottom rung and the backend substituted its own default: the guest's own
//! loopback, under QEMU.
//!
//! `nros sync` resolves the image's declared locator and domain into the
//! image's generated `build/<image>/nros-cargo.toml` as `[env]` rows (the
//! image is the unit; a leaf with two images gets two files). This reads them
//! and defines the macros. It derives nothing: an absent row defines nothing,
//! and the header's own ladder answers exactly as before.
//!
//! It lives here because this is the crate every such `build.rs` already
//! depends on for its `cc::Build` policy; a separate crate would add a
//! build-dependency to each leaf for one function.

/// The `[env]` row carrying the image's declared locator.
pub const LOCATOR_ENV: &str = "NROS_ENTRY_LOCATOR";
/// The `[env]` row carrying the image's declared domain id.
pub const DOMAIN_ID_ENV: &str = "NROS_ENTRY_DOMAIN_ID";

/// Define `NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID` on `build` from the
/// image's `[env]` rows, when present.
///
/// # Panics
///
/// When `NROS_ENTRY_DOMAIN_ID` is set but is not a decimal integer: the header
/// requires an integer constant expression, and a value that is not one would
/// otherwise surface as a C compile error far from its cause.
pub fn bake(build: &mut cc::Build) {
    println!("cargo:rerun-if-env-changed={LOCATOR_ENV}");
    println!("cargo:rerun-if-env-changed={DOMAIN_ID_ENV}");
    if let Some(loc) = std::env::var(LOCATOR_ENV).ok().filter(|v| !v.is_empty()) {
        build.define("NROS_ENTRY_LOCATOR", Some(c_string_literal(&loc).as_str()));
    }
    if let Some(id) = std::env::var(DOMAIN_ID_ENV).ok().filter(|v| !v.is_empty()) {
        assert!(
            id.parse::<u32>().is_ok(),
            "{DOMAIN_ID_ENV}={id:?} is not a decimal domain id"
        );
        build.define("NROS_ENTRY_DOMAIN_ID", Some(id.as_str()));
    }
}

/// `s` as a C string literal, escaping what a locator could carry.
fn c_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::c_string_literal;

    #[test]
    fn a_locator_becomes_one_c_string_literal() {
        assert_eq!(
            c_string_literal("tcp/10.0.2.2:10500"),
            "\"tcp/10.0.2.2:10500\""
        );
        assert_eq!(c_string_literal(r#"a"b\c"#), r#""a\"b\\c""#);
    }
}
