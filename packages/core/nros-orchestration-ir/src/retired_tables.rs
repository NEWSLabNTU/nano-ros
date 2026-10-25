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
    /// What retired it (`issue 1809, 0.6.0`, `RFC-0109 D6`).
    pub by: &'static str,
    /// What to write instead — the whole remedy, in the user's vocabulary.
    pub replacement: &'static str,
}

/// Every retired table. A reader refuses any of them.
pub const RETIRED_SYSTEM_TABLES: &[RetiredTable] = &[
    RetiredTable {
        key: "param_services",
        by: "issue 1809, 0.6.0",
        replacement: "declare the axis under `[system]` instead: \
                      `features = [\"param_services\"]` (the typed block was deprecated \
                      sugar for it since phase-261)",
    },
    RetiredTable {
        key: "deploy",
        by: "issue 1809, 0.6.0",
        replacement: "`[deploy.<name>]` conflated a build with a machine, and each half has \
                      its own table now: what is BUILT goes in `[image.<id>]` (`board`, \
                      `rmw`, `profile`, `features`, with `[image_defaults]` for \
                      shared keys and `[board_config.<board>]` for a board's site facts), \
                      and WHERE nodes run goes in `[host.<name>]` (`nodes`, `launch`, \
                      `domain_id`, `locator`). A `kind = \"self\"` block with no nodes is \
                      the implicit single host and needs no table at all",
    },
    RetiredTable {
        key: "network",
        by: "RFC-0109 D6, phase-514",
        replacement: "a network is a layer-1 fact, so it lives in the board network file an \
                      image names in `os_config` (RFC-0109 D4): move `subnet` / `gateway` / \
                      `dhcp` there as `[network.<name>]`, put `router` in the image's \
                      `locator`, and `attach` in `[image.<id>.run] nic`",
    },
    RetiredTable {
        key: "transport",
        by: "RFC-0109 D6, phase-514",
        replacement: "a session is a `[[domain]]` (its `rmw`, `id`, and `config = \
                      \"rmw/<file>\"`, the RMW's own file); an image with one session needs \
                      no table at all — `[system] rmw` / `domain_id` and the image's \
                      `locator` are its implicit default domain",
    },
];

/// One retired key of an `[image.<id>]` / `[image_defaults]` table.
pub type RetiredImageKey = RetiredTable;

/// Every retired image key. A reader refuses any of them.
pub const RETIRED_IMAGE_KEYS: &[RetiredImageKey] = &[
    RetiredTable {
        key: "transport",
        by: "RFC-0109 D6, phase-514",
        replacement: "an image's link is its locator's scheme (`serial/…`, `tcp/…`, `udp/…`, \
                      `custom://…`), derived, never stated beside it — drop the key",
    },
    RetiredTable {
        key: "net",
        by: "RFC-0109 D4, phase-514",
        replacement: "an image's address is a board fact: name a board network file in the \
                  image's `os_config = [\"board-net.toml\"]` and move each \
                  `[<table>.net.<if>]` there as `[<if>]` (with `network`, `host` or \
                  `address`, `mac`), and any `[network.<name>]` it sits on beside it",
    },
];

/// The refusal for key `key` of image table `at` (`image.a`,
/// `image_defaults`), or `None` when it is not retired.
#[must_use]
pub fn retired_image_key_message(at: &str, key: &str) -> Option<String> {
    RETIRED_IMAGE_KEYS.iter().find(|t| t.key == key).map(|t| {
        format!(
            "`[{at}] {}` is retired ({}): {}",
            t.key,
            t.by,
            t.replacement.replace("<table>", at)
        )
    })
}

/// Refuse an image table (`at`) carrying any retired key.
///
/// # Errors
/// The first retired key found, with its replacement.
pub fn refuse_retired_image_keys(table: Option<&toml::Table>, at: &str) -> Result<(), String> {
    let Some(t) = table else {
        return Ok(());
    };
    for k in RETIRED_IMAGE_KEYS {
        if t.contains_key(k.key) {
            return Err(retired_image_key_message(at, k.key).unwrap_or_default());
        }
    }
    Ok(())
}

/// The refusal for top-level key `key`, or `None` when it is not retired.
#[must_use]
pub fn retired_table_message(key: &str) -> Option<String> {
    RETIRED_SYSTEM_TABLES
        .iter()
        .find(|t| t.key == key)
        .map(|t| format!("`[{}]` is retired ({}): {}", t.key, t.by, t.replacement))
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
        assert!(deploy.contains("issue 1809"), "{deploy}");
    }

    /// RFC-0109 D6 (phase-514 W4) — the network and session tables moved out.
    #[test]
    fn the_network_and_transport_tables_are_refused_naming_their_homes() {
        let p = Path::new("s.toml");
        for (src, key, names) in [
            (
                "[network.lab]\nsubnet = \"192.168.1.0/24\"\n",
                "network",
                "[image.<id>.run] nic",
            ),
            ("[[transport]]\nname = \"t\"\n", "transport", "[[domain]]"),
        ] {
            let doc: toml::Table = toml::from_str(src).unwrap();
            let e = refuse_retired_tables(&doc, p).unwrap_err();
            assert!(
                e.contains(&format!("`[{key}]` is retired (RFC-0109")),
                "{e}"
            );
            assert!(e.contains(names), "{e}");
        }
        let doc: toml::Table =
            toml::from_str("[image.a.net.eth0]\nhost = 5\n[image.b]\nboard = \"x\"\n").unwrap();
        let img = |id: &str| doc["image"][id].as_table();
        let e = refuse_retired_image_keys(img("a"), "image.a").unwrap_err();
        assert!(
            e.contains("`[image.a] net` is retired") && e.contains("`[image.a.net.<if>]`"),
            "{e}"
        );
        assert!(refuse_retired_image_keys(img("b"), "image.b").is_ok());
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
