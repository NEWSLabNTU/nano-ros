//! Issue 1809 (the 0.6.0 boundary, phase-486 W8) — `system.toml` tables
//! nano-ros RETIRED, refused by name.
//!
//! A retired table is never dropped in silence: every nano-ros reader of a
//! `system.toml` — the strict `SystemToml` schema in `nros-cli-core`, the
//! overlay derivation, the single-package leaf reader and `nros::main!` — asks
//! this one table and refuses with the same message, naming what replaced it.
//! One spelling, so the refusal cannot drift between readers (CLAUDE.md "fix
//! the class").
//!
//! rlm's parser stopped reading `[deploy.*]` at `v0.1.57` and ignores tables it
//! does not declare; that is rlm's format and its decision. nano-ros refuses
//! before the file reaches it.

use std::path::Path;

/// One retired top-level `system.toml` table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetiredTable {
    /// The top-level key, as written (`deploy`, `param_services`).
    pub key: &'static str,
    /// What to write instead — the whole remedy, in the user's vocabulary.
    pub replacement: &'static str,
}

/// Every retired table. A reader refuses any of them.
pub const RETIRED_SYSTEM_TABLES: &[RetiredTable] = &[
    RetiredTable {
        key: "param_services",
        replacement: "declare the axis under `[system]` instead: \
                      `features = [\"param_services\"]` (the typed block was deprecated \
                      sugar for it since phase-261)",
    },
    RetiredTable {
        key: "deploy",
        replacement: "`[deploy.<name>]` conflated a build with a machine, and each half has \
                      its own table now: what is BUILT goes in `[image.<id>]` (`board`, \
                      `rmw`, `profile`, `features`, with `[image_defaults]` for \
                      shared keys and `[board_config.<board>]` for a board's site facts), \
                      and WHERE nodes run goes in `[host.<name>]` (`nodes`, `launch`, \
                      `domain_id`, `locator`). A `kind = \"self\"` block with no nodes is \
                      the implicit single host and needs no table at all",
    },
];

/// The refusal for top-level key `key`, or `None` when it is not retired.
#[must_use]
pub fn retired_table_message(key: &str) -> Option<String> {
    RETIRED_SYSTEM_TABLES
        .iter()
        .find(|t| t.key == key)
        .map(|t| {
            format!(
                "`[{}]` is retired (issue 1809, 0.6.0): {}",
                t.key, t.replacement
            )
        })
}

/// Refuse a parsed `system.toml` carrying any retired table, naming the file.
///
/// # Errors
/// The first retired table found, with its replacement.
pub fn refuse_retired_tables(doc: &toml::Table, file: &Path) -> Result<(), String> {
    for t in RETIRED_SYSTEM_TABLES {
        if doc.contains_key(t.key) {
            let msg = retired_table_message(t.key).unwrap_or_default();
            return Err(format!("{}: {msg}", file.display()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_retired_table_is_refused_naming_its_replacement() {
        let p = Path::new("bringup/system.toml");
        for (src, key, names) in [
            (
                "[system]\nname = \"s\"\n[param_services]\n",
                "param_services",
                "features = [\"param_services\"]",
            ),
            (
                "[system]\nname = \"s\"\n[deploy.native]\nkind = \"self\"\n",
                "deploy",
                "[image.<id>]",
            ),
        ] {
            let doc: toml::Table = toml::from_str(src).unwrap();
            let e = refuse_retired_tables(&doc, p).unwrap_err();
            assert!(e.contains(&format!("`[{key}]` is retired")), "{e}");
            assert!(e.contains(names), "{e}");
            assert!(e.contains("bringup/system.toml"), "{e}");
        }
        let deploy = retired_table_message("deploy").unwrap();
        assert!(deploy.contains("[host.<name>]"), "{deploy}");
    }

    #[test]
    fn a_live_table_is_not_refused() {
        let doc: toml::Table = toml::from_str(
            "[system]\nfeatures = [\"param_services\"]\n[image.a]\nboard = \"x\"\n[host.h]\n",
        )
        .unwrap();
        assert!(refuse_retired_tables(&doc, Path::new("s.toml")).is_ok());
        assert!(retired_table_message("image").is_none());
        assert!(retired_table_message("safety").is_none());
    }
}
