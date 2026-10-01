//! The entry packs' spelling filters — the ONLY per-language Rust an entry
//! pack may need (phase-474 W2 / RFC-0091 §6b).
//!
//! A spelling is a correctness property: a wrong one compiles somewhere else,
//! later, as an error with no path back to what produced it. That is the
//! argument phase-469 used to decline RFC-0068's `spelling.toml`, and it holds
//! here unchanged, so spellings stay in compiled Rust. What a template does
//! with a spelled value is LAYOUT, and layout is the pack's.
//!
//! A pack DECLARES the filters its templates call (`filters = [...]` in its
//! `pack.toml`); `every_filter_a_pack_calls_is_declared_and_registered`
//! below holds the two together in both directions. The registry is keyed by
//! NAME, and a name is the pack that calls it, not the syntax it emits
//! (RFC-0091 §6b's W2.5b correction): `cpp_board_class` is C++'s spelling of a
//! board family, and a Zig pack would add its own row, not grow C++'s.
//!
//! A new entry language whose values these filters already spell correctly
//! needs NO Rust at all: a `pack.toml` and its templates. One that needs a new
//! spelling adds ONE function and ONE row here — which is the whole of what
//! RFC-0091 §8 step 4 ("an entry emitter in Rust") used to mean.

use minijinja::{Environment, Error, ErrorKind};

/// Escape a raw string for a C or C++ string literal: backslash and double
/// quote. (Issue 1102 / RFC-0091 §8b defect 2 — escaping is a per-language
/// filter, never an IR field: C, Rust and Zig literals escape differently, so
/// there is no neutral "already escaped" value.)
fn c_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A ROS package name as an identifier — `nros_entry_lower::sanitize_pkg`,
/// the ONE answer to "which identifier does this package become" (phase-432
/// W2.4). Language-neutral: every C-family and Rust pack agrees, which is why
/// the lowering keeps the RAW name and the pack asks for the spelling.
fn pkg_ident(s: &str) -> String {
    nros_entry_lower::sanitize_pkg(s)
}

/// The C++ board class a family's entry calls (`::nros::board::LinuxBoard`).
///
/// phase-432 W2.2 / RFC-0091 §8b defect 1 — a RENDERING of the board family,
/// not a second table: the board keys collapse onto five families in
/// `nros_entry_lower::BOARD_KEYS`, and this is C++'s spelling of each. It used
/// to be `emit_cpp::board_cpp_path`, an emitter-private function; it is a
/// filter now because a spelling is exactly what a filter is.
fn cpp_board_class(family: &str) -> Result<String, Error> {
    use nros_entry_lower::BoardFamily;
    let f = nros_entry_lower::family_from_str(family).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("cpp_board_class: `{family}` is not a board family"),
        )
    })?;
    Ok(match f {
        BoardFamily::Native => "::nros::board::LinuxBoard",
        BoardFamily::Zephyr => "::nros::board::ZephyrBoard",
        BoardFamily::Nuttx => "::nros::board::NuttxBoard",
        BoardFamily::Freertos => "::nros::board::FreertosBoard",
        BoardFamily::Threadx => "::nros::board::ThreadxBoard",
    }
    .to_string())
}

/// Adds one filter to an environment.
type Register = fn(&mut Environment<'static>);

/// Every entry filter, by the name a pack declares and a template calls.
pub(crate) const FILTERS: &[(&str, Register)] = &[
    ("c_str", |env| env.add_filter("c_str", c_str)),
    ("pkg_ident", |env| env.add_filter("pkg_ident", pkg_ident)),
    ("cpp_board_class", |env| {
        env.add_filter("cpp_board_class", cpp_board_class)
    }),
];

/// Register every filter on `env`.
pub(crate) fn register_all(env: &mut Environment<'static>) {
    for (_, register) in FILTERS {
        register(env);
    }
}

/// Register exactly the filters `names` declares — what a pack loaded from
/// outside the bundle gets, so its declaration is what it renders with.
#[cfg(test)]
pub(crate) fn register_declared(
    env: &mut Environment<'static>,
    names: &[String],
) -> Result<(), String> {
    for name in names {
        let (_, register) = FILTERS
            .iter()
            .find(|(n, _)| n == name)
            .ok_or_else(|| format!("filter `{name}` is declared and no Rust registers it"))?;
        register(env);
    }
    Ok(())
}

/// Every `| name` a template calls, inside its `{{ }}` and `{% %}` tags.
///
/// Text outside a tag is generated SOURCE — a Rust closure's `|runtime|` is
/// not a filter call — so it is skipped rather than pattern-matched.
#[cfg(test)]
pub(crate) fn filters_called(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find(['{']) {
        let after = &rest[open + 1..];
        let close = match after.chars().next() {
            Some('{') => "}}",
            Some('%') => "%}",
            // A comment is prose about the template, and may quote a tag.
            Some('#') => {
                let Some(end) = after.find("#}") else { break };
                rest = &after[end + 2..];
                continue;
            }
            _ => {
                rest = after;
                continue;
            }
        };
        let Some(end) = after.find(close) else { break };
        let tag = &after[1..end];
        let mut parts = tag.split('|').skip(1);
        for p in parts.by_ref() {
            let name: String = p
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
        rest = &after[end + close.len()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_str_escapes_backslash_and_quote() {
        assert_eq!(c_str(r#"a\b"c"#), r#"a\\b\"c"#);
    }

    #[test]
    fn cpp_board_class_spells_every_family_and_refuses_the_rest() {
        for f in nros_entry_lower::BoardFamily::ALL {
            let class = cpp_board_class(f.as_str()).expect("every family has a class");
            assert!(class.starts_with("::nros::board::"), "{class}");
        }
        assert!(cpp_board_class("zigos").is_err());
    }

    #[test]
    fn filters_called_reads_tags_and_skips_source_text() {
        let t = "{# {{ q | quoted }} #}{{ a | c_str }} |runtime| {%- if x|length %}{{ b|pkg_ident }}{% endif %}";
        assert_eq!(filters_called(t), vec!["c_str", "length", "pkg_ident"]);
    }

    /// Both directions, over every bundled pack: a filter a pack's templates
    /// call is either declared by that pack AND registered here, or a
    /// minijinja builtin; and every filter registered here is called by some
    /// pack. The first half is the one with teeth — minijinja resolves a
    /// filter at RENDER time, so a template calling a filter nobody registers
    /// fails on some user's build rather than here.
    #[test]
    fn every_filter_a_pack_calls_is_declared_and_registered() {
        let manifests = super::super::pack::manifests().expect("manifests parse");
        let registered: Vec<&str> = FILTERS.iter().map(|(n, _)| *n).collect();
        let mut called_anywhere: Vec<String> = Vec::new();
        for (pack, _key, src) in super::super::render::ENTRY_TEMPLATES {
            let m = &manifests[pack];
            for f in filters_called(src) {
                if registered.contains(&f.as_str()) {
                    assert!(
                        m.filters.contains(&f),
                        "pack `{pack}` calls `{f}` without declaring it in `filters`"
                    );
                } else {
                    assert!(
                        is_builtin_filter(&f),
                        "pack `{pack}` calls `{f}`, which is neither registered nor a builtin"
                    );
                }
                called_anywhere.push(f);
            }
            for f in &m.filters {
                assert!(
                    registered.contains(&f.as_str()),
                    "pack `{pack}` declares `{f}`, which no Rust registers"
                );
            }
        }
        for name in registered {
            assert!(
                called_anywhere.iter().any(|f| f == name),
                "`{name}` is registered and no pack calls it"
            );
        }
    }

    /// Whether `name` is a minijinja builtin: a fresh environment, which has
    /// only the builtins, must not report it UNKNOWN.
    fn is_builtin_filter(name: &str) -> bool {
        let env = Environment::new();
        let src = format!("{{{{ x | {name} }}}}");
        match env.render_str(&src, minijinja::context! { x => "" }) {
            Ok(_) => true,
            Err(e) => e.kind() != ErrorKind::UnknownFilter,
        }
    }
}
